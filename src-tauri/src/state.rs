use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
};

use celestite_documents::{Document, DocumentError, DocumentSnapshot, NoteStore, Preferences};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;

const MAX_RECENT_FILES: usize = 12;
const DEFAULT_WINDOW_WIDTH: f64 = 1280.0;
const DEFAULT_WINDOW_HEIGHT: f64 = 800.0;
const MIN_WINDOW_WIDTH: f64 = 640.0;
const MIN_WINDOW_HEIGHT: f64 = 480.0;

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub kind: &'static str,
    pub message: String,
}

impl CommandError {
    pub fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            kind: "internal",
            message: error.to_string(),
        }
    }
}

impl From<DocumentError> for CommandError {
    fn from(error: DocumentError) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

impl From<tauri::Error> for CommandError {
    fn from(error: tauri::Error) -> Self {
        Self {
            kind: "window",
            message: error.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct WindowDocument {
    pub document: Document,
    pub close_allowed: bool,
    pub initial_selection: Option<usize>,
}

#[derive(Debug)]
pub struct Registry {
    pub windows: HashMap<String, WindowDocument>,
    pub pending_closes: HashSet<Uuid>,
    pub preferences: Preferences,
    pub warnings: Vec<String>,
}

impl Registry {
    pub fn active_document(&mut self, label: &str) -> Result<&mut WindowDocument, CommandError> {
        let entry = self.document(label)?;
        if entry.close_allowed {
            return Err(DocumentError::Stale.into());
        }
        Ok(entry)
    }

    pub fn document(&mut self, label: &str) -> Result<&mut WindowDocument, CommandError> {
        self.windows
            .get_mut(label)
            .ok_or_else(|| CommandError::internal("This note window is no longer available."))
    }

    pub fn remember(&mut self, store: &NoteStore, path: &Path) {
        let mut next = self.preferences.clone();
        next.recent_files.retain(|p| p != path);
        next.recent_files.insert(0, path.to_path_buf());
        next.recent_files.truncate(MAX_RECENT_FILES);
        match store.save_preferences(&next) {
            Ok(()) => self.preferences = next,
            Err(error) => self
                .warnings
                .push(format!("Could not remember recent files: {error}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppState {
    pub registry: Arc<Mutex<Registry>>,
    pub store: Arc<NoteStore>,
}

impl AppState {
    pub fn create(store: NoteStore) -> Result<(Self, Vec<Document>), DocumentError> {
        let mut warnings = Vec::new();
        let preferences = match store.load_preferences() {
            Ok(preferences) => preferences,
            Err(error) => {
                warnings.push(format!("Could not read preferences: {error}"));
                Preferences::default()
            }
        };
        let mut report = store.recover()?;
        for document in &mut report.documents {
            store.bind_document(document)?;
        }
        warnings.extend(report.warnings);
        let mut documents = report.documents.into_iter();
        let (first, initial_selection) = match documents.next() {
            Some(recovered) => (recovered, None),
            None => fresh_document(&store),
        };
        let mut windows = HashMap::new();
        windows.insert(
            String::from("note-main"),
            WindowDocument {
                document: first,
                close_allowed: false,
                initial_selection,
            },
        );
        Ok((
            Self {
                registry: Arc::new(Mutex::new(Registry {
                    windows,
                    pending_closes: HashSet::new(),
                    preferences,
                    warnings,
                })),
                store: Arc::new(store),
            },
            documents.collect(),
        ))
    }
}

/// Creates the document a New command or an empty launch shows, with its initial caret.
///
/// With a library, the note starts with its identity front matter, the caret follows that front matter, and the note lands in the library at its first nonempty checkpoint; otherwise it stays an untitled file with the caret at the start.
pub fn fresh_document(store: &NoteStore) -> (Document, Option<usize>) {
    if store.library().is_some() {
        let document = Document::new_in_library();
        let caret = document.snapshot().text.encode_utf16().count();
        (document, Some(caret))
    } else {
        (Document::new(), None)
    }
}

pub fn display_name(snapshot: &DocumentSnapshot) -> &str {
    snapshot.title.as_deref().unwrap_or(&snapshot.name)
}

pub async fn with_registry<T: Send + 'static>(
    state: &AppState,
    operation: impl FnOnce(&mut Registry, &NoteStore) -> Result<T, CommandError> + Send + 'static,
) -> Result<T, CommandError> {
    let registry = Arc::clone(&state.registry);
    let store = Arc::clone(&state.store);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = registry.lock().map_err(CommandError::internal)?;
        operation(&mut guard, &store)
    })
    .await
    .map_err(CommandError::internal)?
}

pub fn spawn_document<R: Runtime>(
    app: &AppHandle<R>,
    document: Document,
) -> Result<(), CommandError> {
    spawn_document_at(app, document, None)
}

pub fn spawn_document_at<R: Runtime>(
    app: &AppHandle<R>,
    mut document: Document,
    selection: Option<usize>,
) -> Result<(), CommandError> {
    let state = app.state::<AppState>();
    state.store.bind_document(&mut document)?;
    let label = format!("note-{}", Uuid::new_v4());
    let name = display_name(&document.snapshot()).to_owned();
    state
        .registry
        .lock()
        .map_err(CommandError::internal)?
        .windows
        .insert(
            label.clone(),
            WindowDocument {
                document,
                close_allowed: false,
                initial_selection: selection,
            },
        );
    let result = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(format!("{name} — Celestite"))
        .inner_size(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
        .min_inner_size(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)
        .decorations(false)
        .resizable(true)
        .center()
        .build();
    match result {
        Ok(_) => Ok(()),
        Err(error) => {
            match state.registry.lock() {
                Ok(mut registry) => {
                    registry.windows.remove(&label);
                }
                Err(cleanup) => eprintln!("Window cleanup failed: {cleanup}"),
            }
            Err(error.into())
        }
    }
}

pub fn broadcast_preferences<R: Runtime>(app: &AppHandle<R>, preferences: &Preferences) {
    if let Err(error) = app.emit("celestite:preferences", preferences) {
        eprintln!("Could not notify note windows about preferences: {error}");
    }
}

pub fn update_title<R: Runtime>(window: &tauri::WebviewWindow<R>, snapshot: &DocumentSnapshot) {
    let marker = if snapshot.revision == snapshot.saved_revision {
        ""
    } else {
        " *"
    };
    if let Err(error) = window.set_title(&format!("{}{marker} — Celestite", display_name(snapshot)))
    {
        eprintln!("Could not update a note window title: {error}");
    }
}
