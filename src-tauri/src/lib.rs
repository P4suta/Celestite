mod commands;
mod state;

#[cfg(target_os = "macos")]
mod native_menu;

#[cfg(test)]
mod tests;

use celestite_core::{ExitDecision, plan_exit};
use celestite_documents::{Document, NoteStore};
use tauri::{Emitter, EventTarget, Manager, RunEvent, WindowEvent};
#[cfg(target_os = "macos")]
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use state::{AppState, spawn_document};

const DEFAULT_LIBRARY_NAME: &str = "Celestite";

fn invoke_handler<R: tauri::Runtime>()
-> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        commands::bootstrap,
        commands::checkpoint_document,
        commands::save_document_as,
        commands::open_document,
        commands::follow_link,
        commands::list_library_notes,
        commands::insert_link,
        commands::store_asset,
        commands::new_document,
        commands::inspect_document,
        commands::reload_document,
        commands::set_appearance,
        commands::confirm_close,
    ]
}

fn request_close_all<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    for window in app.webview_windows().values() {
        if let Err(error) = window.emit_to(
            EventTarget::webview_window(window.label()),
            "celestite:close-requested",
            (),
        ) {
            eprintln!("Could not request closing a note: {error}");
        }
    }
}

pub fn run() -> Result<(), tauri::Error> {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app
                .get_webview_window("note-main")
                .or_else(|| app.webview_windows().into_values().next())
                && let Err(error) = window
                    .show()
                    .and_then(|()| window.unminimize())
                    .and_then(|()| window.set_focus())
            {
                eprintln!("Could not focus the existing note window: {error}");
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        );
    #[cfg(target_os = "macos")]
    let builder = builder.menu(native_menu::menu).on_menu_event(|app, event| {
        if event.id().as_ref() == native_menu::QUIT_MENU_ID {
            app.exit(0);
        }
    });
    let app = builder
        .setup(|app| {
            let store = NoteStore::new(app.path().app_local_data_dir()?)?;
            let library = store
                .load_preferences()
                .ok()
                .and_then(|preferences| preferences.library)
                .or_else(|| {
                    app.path()
                        .document_dir()
                        .ok()
                        .map(|documents| documents.join(DEFAULT_LIBRARY_NAME))
                });
            let (state, recovered) = AppState::create(store.with_library(library))?;
            app.manage(state);
            for document in recovered {
                if let Err(error) = spawn_document(app.handle(), document) {
                    eprintln!(
                        "A recovered note window could not be opened: {}",
                        error.message
                    );
                }
            }
            Ok(())
        })
        .invoke_handler(invoke_handler())
        .on_window_event(|window, event| {
            let state = window.state::<AppState>();
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    let allowed = match state.registry.lock() {
                        Ok(registry) => registry
                            .windows
                            .get(window.label())
                            .is_none_or(|entry| entry.close_allowed),
                        Err(error) => {
                            eprintln!("Could not inspect close state: {error}");
                            false
                        }
                    };
                    if !allowed {
                        api.prevent_close();
                        if let Err(error) = window.emit_to(
                            EventTarget::webview_window(window.label()),
                            "celestite:close-requested",
                            (),
                        ) {
                            eprintln!("Could not notify a note window: {error}");
                        }
                    }
                }
                WindowEvent::Destroyed => match state.registry.lock() {
                    Ok(mut registry) => {
                        registry.windows.remove(window.label());
                    }
                    Err(error) => eprintln!("Could not remove closed window state: {error}"),
                },
                WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                    let window = window.clone();
                    let paths = paths.clone();
                    tauri::async_runtime::spawn(commands::receive_dropped_files(window, paths));
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())?;
    app.run(|handle, event| match event {
        #[cfg(target_os = "macos")]
        RunEvent::Opened { urls } => {
            for url in urls {
                let Ok(path) = url.to_file_path() else {
                    continue;
                };
                let app = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let result =
                        tauri::async_runtime::spawn_blocking(move || Document::open(&path))
                            .await
                            .map_err(state::CommandError::internal)
                            .and_then(|result| result.map_err(Into::into))
                            .and_then(|document| spawn_document(&app, document));
                    if let Err(error) = result {
                        app.dialog()
                            .message(error.message)
                            .title("Couldn’t open the file")
                            .kind(MessageDialogKind::Error)
                            .show(|_| {});
                    }
                });
            }
        }
        RunEvent::ExitRequested { api, .. } => {
            let state = handle.state::<AppState>();
            let pending = match state.registry.lock() {
                Ok(registry) => !registry.pending_closes.is_empty(),
                Err(error) => {
                    eprintln!("Could not inspect pending close cleanup: {error}");
                    true
                }
            };
            match plan_exit(!handle.webview_windows().is_empty(), pending) {
                ExitDecision::CloseNotes => {
                    api.prevent_exit();
                    request_close_all(handle);
                }
                ExitDecision::WaitForCleanup => api.prevent_exit(),
                ExitDecision::Exit => {}
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } => {
            let (document, caret) = state::fresh_document(&handle.state::<AppState>().store);
            if let Err(error) = state::spawn_document_at(handle, document, caret) {
                eprintln!("Could not reopen Celestite: {}", error.message);
            }
        }
        _ => {}
    });
    Ok(())
}
