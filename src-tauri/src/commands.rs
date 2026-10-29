use std::path::PathBuf;

use celestite_documents::{
    Appearance, Document, DocumentSnapshot, LinkCandidate, LinkDestination, LinkEdit,
    LinkInsertion, LinkNavigation, Preferences,
};

const MAX_ASSET_EXTENSION_LENGTH: usize = 16;
use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

use crate::state::{
    AppState, CommandError, broadcast_preferences, fresh_document, spawn_document,
    spawn_document_at, update_title, with_registry,
};

#[derive(Debug, Deserialize)]
pub struct EditRequest {
    token: String,
    revision: u64,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum LinkTargetRequest {
    Note { id: String },
    Asset { digest: String, extension: String },
}

impl TryFrom<LinkTargetRequest> for LinkDestination {
    type Error = CommandError;
    fn try_from(request: LinkTargetRequest) -> Result<Self, Self::Error> {
        match request {
            LinkTargetRequest::Note { id } => Ok(Self::Note(
                id.parse()
                    .map_err(celestite_documents::DocumentError::Archive)?,
            )),
            LinkTargetRequest::Asset { digest, extension } => {
                if extension.is_empty()
                    || extension.len() > MAX_ASSET_EXTENSION_LENGTH
                    || !extension
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                {
                    return Err(CommandError::internal("Invalid asset extension."));
                }
                Ok(Self::Asset {
                    digest: format!("sha256:{digest}")
                        .parse()
                        .map_err(celestite_documents::DocumentError::Archive)?,
                    extension,
                })
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Bootstrap {
    document: DocumentSnapshot,
    preferences: Preferences,
    warnings: Vec<String>,
    selection: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct OpenResult {
    document: Option<DocumentSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct LinkResult {
    token: String,
    revision: u64,
    selection: Option<usize>,
}

#[tauri::command]
pub async fn bootstrap<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
) -> Result<Bootstrap, CommandError> {
    let label = window.label().to_owned();
    let result = with_registry(&state, move |registry, _| {
        let entry = registry.document(&label)?;
        let document = entry.document.snapshot();
        let selection = entry.initial_selection.take();
        Ok(Bootstrap {
            document,
            preferences: registry.preferences.clone(),
            warnings: registry.warnings.clone(),
            selection,
        })
    })
    .await?;
    update_title(&window, &result.document);
    Ok(result)
}

#[tauri::command]
pub async fn checkpoint_document<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    edit: EditRequest,
) -> Result<DocumentSnapshot, CommandError> {
    let label = window.label().to_owned();
    let (snapshot, preferences) = with_registry(&state, move |registry, store| {
        let entry = registry.active_document(&label)?;
        entry.document.edit(&edit.token, edit.revision, edit.text)?;
        let placed = store.place_in_library(&mut entry.document)?;
        store.checkpoint(&entry.document)?;
        if entry.document.path().is_some() {
            if !placed {
                entry.document.save()?;
            }
            if let Err(error) = store.remove_checkpoint(entry.document.checkpoint_key()) {
                eprintln!("A saved recovery record could not be removed: {error}");
            }
        }
        let snapshot = entry.document.snapshot();
        if let Some(path) = &snapshot.path {
            registry.remember(store, &PathBuf::from(path));
        }
        Ok((snapshot, registry.preferences.clone()))
    })
    .await?;
    update_title(&window, &snapshot);
    broadcast_preferences(window.app_handle(), &preferences);
    Ok(snapshot)
}

#[tauri::command]
pub async fn list_library_notes<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<LinkCandidate>, CommandError> {
    let label = window.label().to_owned();
    with_registry(&state, move |registry, store| {
        registry.active_document(&label)?;
        Ok(store.library_notes(&query)?)
    })
    .await
}

#[tauri::command]
pub async fn insert_link<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    edit: EditRequest,
    link: LinkInsertion,
    target: LinkTargetRequest,
) -> Result<LinkEdit, CommandError> {
    let label = window.label().to_owned();
    let destination = LinkDestination::try_from(target)?;
    with_registry(&state, move |registry, _| {
        let entry = registry.active_document(&label)?;
        Ok(entry.document.insert_link(
            &edit.token,
            edit.revision,
            &edit.text,
            &link,
            &destination,
        )?)
    })
    .await
}

#[tauri::command]
pub async fn save_document_as<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    edit: EditRequest,
    path: String,
) -> Result<DocumentSnapshot, CommandError> {
    let label = window.label().to_owned();
    let (snapshot, preferences) = with_registry(&state, move |registry, store| {
        let entry = registry.active_document(&label)?;
        entry.document.edit(&edit.token, edit.revision, edit.text)?;
        store.checkpoint(&entry.document)?;
        entry.document.save_as(&PathBuf::from(path))?;
        store.checkpoint(&entry.document)?;
        if let Err(error) = store.remove_checkpoint(entry.document.checkpoint_key()) {
            eprintln!("A saved recovery record could not be removed: {error}");
        }
        let snapshot = entry.document.snapshot();
        if let Some(path) = &snapshot.path {
            registry.remember(store, &PathBuf::from(path));
        }
        Ok((snapshot, registry.preferences.clone()))
    })
    .await?;
    update_title(&window, &snapshot);
    broadcast_preferences(window.app_handle(), &preferences);
    Ok(snapshot)
}

#[tauri::command]
pub async fn open_document<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    path: String,
    token: String,
    reuse_current: bool,
) -> Result<OpenResult, CommandError> {
    let mut document =
        tauri::async_runtime::spawn_blocking(move || Document::open(&PathBuf::from(path)))
            .await
            .map_err(CommandError::internal)??;
    let label = window.label().to_owned();
    let (snapshot, additional, preferences) = with_registry(&state, move |registry, store| {
        let current = registry.active_document(&label)?;
        if current.document.snapshot().token != token {
            return Err(celestite_documents::DocumentError::Stale.into());
        }
        store.bind_document(&mut document)?;
        let may_reuse = reuse_current && current.document.may_reuse();
        let path = document.path().map(PathBuf::from);
        let result = if may_reuse {
            store.remove_checkpoint(current.document.checkpoint_key())?;
            current.document = document;
            (Some(current.document.snapshot()), None)
        } else {
            (None, Some(document))
        };
        if let Some(path) = path {
            registry.remember(store, &path);
        }
        Ok((result.0, result.1, registry.preferences.clone()))
    })
    .await?;
    if let Some(document) = additional {
        spawn_document(window.app_handle(), document)?;
    }
    if let Some(snapshot) = &snapshot {
        update_title(&window, snapshot);
    }
    broadcast_preferences(window.app_handle(), &preferences);
    Ok(OpenResult { document: snapshot })
}

#[tauri::command]
pub async fn new_document<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let (document, caret) = fresh_document(&state.store);
    spawn_document_at(window.app_handle(), document, caret)
}

#[tauri::command]
pub async fn follow_link<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    edit: EditRequest,
    position: usize,
) -> Result<LinkResult, CommandError> {
    let label = window.label().to_owned();
    let token = edit.token.clone();
    let revision = edit.revision;
    let navigation = with_registry(&state, move |registry, _| {
        let entry = registry.active_document(&label)?;
        Ok(entry
            .document
            .follow_link(&edit.token, edit.revision, &edit.text, position)?)
    })
    .await?;
    let selection = match navigation {
        LinkNavigation::Here { selection } => Some(selection),
        LinkNavigation::Note {
            document,
            selection,
        } => {
            let path = document.path().map(PathBuf::from);
            spawn_document_at(window.app_handle(), *document, selection)?;
            let preferences = with_registry(&state, move |registry, store| {
                if let Some(path) = path {
                    registry.remember(store, &path);
                }
                Ok(registry.preferences.clone())
            })
            .await?;
            broadcast_preferences(window.app_handle(), &preferences);
            None
        }
        LinkNavigation::Web { url } => {
            window
                .app_handle()
                .opener()
                .open_url(url, None::<&str>)
                .map_err(CommandError::internal)?;
            None
        }
        LinkNavigation::Attachment { path } => {
            window
                .app_handle()
                .opener()
                .open_path(path.to_string_lossy(), None::<&str>)
                .map_err(CommandError::internal)?;
            None
        }
    };
    Ok(LinkResult {
        token,
        revision,
        selection,
    })
}

#[tauri::command]
pub async fn inspect_document<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    token: String,
) -> Result<bool, CommandError> {
    let label = window.label().to_owned();
    with_registry(&state, move |registry, _| {
        let entry = registry.document(&label)?;
        if entry.document.snapshot().token != token {
            return Err(celestite_documents::DocumentError::Stale.into());
        }
        Ok(entry.document.has_external_changes()?)
    })
    .await
}

#[tauri::command]
pub async fn reload_document<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    token: String,
) -> Result<DocumentSnapshot, CommandError> {
    let label = window.label().to_owned();
    let snapshot = with_registry(&state, move |registry, store| {
        let entry = registry.active_document(&label)?;
        if entry.document.snapshot().token != token {
            return Err(celestite_documents::DocumentError::Stale.into());
        }
        let path = entry
            .document
            .path()
            .ok_or(celestite_documents::DocumentError::InvalidPath)?;
        let mut replacement = Document::open(path)?;
        store.bind_document(&mut replacement)?;
        store.remove_checkpoint(entry.document.checkpoint_key())?;
        entry.document = replacement;
        Ok(entry.document.snapshot())
    })
    .await?;
    update_title(&window, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn set_appearance<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    appearance: Appearance,
) -> Result<Preferences, CommandError> {
    let preferences = with_registry(&state, move |registry, store| {
        let mut next = registry.preferences.clone();
        next.appearance = appearance;
        store.save_preferences(&next)?;
        registry.preferences = next;
        Ok(registry.preferences.clone())
    })
    .await?;
    broadcast_preferences(window.app_handle(), &preferences);
    Ok(preferences)
}

#[tauri::command]
pub async fn confirm_close<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    token: String,
    revision: u64,
    discard: bool,
) -> Result<(), CommandError> {
    let result = close_note(
        &state,
        window.label().to_owned(),
        token,
        revision,
        discard,
        || window.close().map_err(Into::into),
    )
    .await;
    if window.app_handle().webview_windows().is_empty() {
        window.app_handle().exit(0);
    }
    result
}

pub(crate) async fn close_note(
    state: &AppState,
    label: String,
    token: String,
    revision: u64,
    discard: bool,
    close: impl FnOnce() -> Result<(), CommandError> + Send,
) -> Result<(), CommandError> {
    let rollback_label = label.clone();
    let id = with_registry(state, move |registry, _| {
        let entry = registry.document(&label)?;
        if entry.close_allowed || !entry.document.may_close(&token, revision, discard) {
            return Err(celestite_documents::DocumentError::Stale.into());
        }
        entry.close_allowed = true;
        let id = entry.document.checkpoint_key();
        registry.pending_closes.insert(id);
        Ok(id)
    })
    .await?;
    if let Err(error) = close() {
        if let Err(rollback) = with_registry(state, move |registry, _| {
            registry.pending_closes.remove(&id);
            registry.document(&rollback_label)?.close_allowed = false;
            Ok(())
        })
        .await
        {
            eprintln!("Could not revoke a failed close: {}", rollback.message);
        }
        return Err(error);
    }
    with_registry(state, move |registry, store| {
        let result = store.remove_checkpoint(id);
        registry.pending_closes.remove(&id);
        result?;
        Ok(())
    })
    .await
    .inspect_err(|error| {
        eprintln!(
            "Could not remove a closed note's recovery record: {}",
            error.message
        )
    })
}
