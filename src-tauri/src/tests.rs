use std::{error::Error, fs};

use celestite_documents::NoteStore;
use serde_json::{Value, json};
use tauri::{
    Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets},
    webview::InvokeRequest,
};
use tempfile::TempDir;

use crate::state::AppState;

const IPC_ORIGIN: &str = "tauri://localhost";

struct Desktop {
    app: tauri::App<MockRuntime>,
    directory: TempDir,
    window: WebviewWindow<MockRuntime>,
}

impl Desktop {
    fn start() -> Result<Self, Box<dyn Error>> {
        Self::launch(false)
    }

    fn start_with_library() -> Result<Self, Box<dyn Error>> {
        Self::launch(true)
    }

    fn launch(library: bool) -> Result<Self, Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let store = NoteStore::new(directory.path().join("data"))?
            .with_library(library.then(|| directory.path().join("library")));
        let (state, _) = AppState::create(store)?;
        let app = mock_builder()
            .manage(state)
            .invoke_handler(crate::invoke_handler())
            .build(mock_context(noop_assets()))?;
        let window = WebviewWindowBuilder::new(&app, "note-main", WebviewUrl::default()).build()?;
        Ok(Self {
            app,
            directory,
            window,
        })
    }

    fn bootstrap(&self) -> Result<Value, Box<dyn Error>> {
        invoke(&self.window, "bootstrap", json!({})).map_err(|error| error.to_string().into())
    }
}

fn invoke(window: &WebviewWindow<MockRuntime>, command: &str, body: Value) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: IPC_ORIGIN
                .parse()
                .map_err(|error| json!(format!("{error}")))?,
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
    )
    .and_then(|response| {
        response
            .deserialize::<Value>()
            .map_err(|error| json!(error.to_string()))
    })
}

#[test]
fn link_ipc_checks_the_exact_source_before_opening_another_window() -> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    let initial = desktop.bootstrap()?;
    let token = &initial["document"]["token"];
    let target = desktop.directory.path().join("linked note.md");
    fs::write(&target, "🦀\n# Destination\n")?;
    let url = tauri::Url::from_file_path(&target).map_err(|()| "Invalid fixture path")?;
    let text = format!("[jump]({url}#destination)");
    let edit = json!({"token":token,"revision":1,"text":text});
    assert!(invoke(&desktop.window, "checkpoint_document", json!({"edit":edit})).is_ok());
    for stale in [
        json!({"token":"other","revision":1,"text":text}),
        json!({"token":token,"revision":0,"text":text}),
        json!({"token":token,"revision":2,"text":text}),
        json!({"token":token,"revision":1,"text":"different"}),
    ] {
        let result = invoke(
            &desktop.window,
            "follow_link",
            json!({"edit":stale,"position":2}),
        );
        assert_eq!(
            result.as_ref().err().map(|error| &error["kind"]),
            Some(&json!("stale"))
        );
        assert_eq!(desktop.app.webview_windows().len(), 1);
    }
    assert!(
        invoke(
            &desktop.window,
            "follow_link",
            json!({"edit":edit,"position":2})
        )
        .is_ok()
    );
    assert_eq!(desktop.app.webview_windows().len(), 2);
    assert_eq!(desktop.bootstrap()?["document"]["text"], text);
    let other = desktop
        .app
        .webview_windows()
        .into_values()
        .find(|window| window.label() != "note-main")
        .ok_or("Missing linked window")?;
    let opened = invoke(&other, "bootstrap", json!({})).map_err(|error| error.to_string())?;
    assert_eq!(opened["document"]["text"], "🦀\n# Destination\n");
    assert_eq!(opened["selection"], "🦀\n".encode_utf16().count());
    assert_eq!(
        invoke(&other, "bootstrap", json!({})).map_err(|error| error.to_string())?["selection"],
        Value::Null
    );
    Ok(())
}

#[test]
fn link_ipc_refuses_malformed_positions_and_closed_notes() -> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    let token = desktop.bootstrap()?["document"]["token"].clone();
    let text = "[jump](#heading)\n# Heading\n";
    let edit = json!({"token":token,"revision":1,"text":text});
    assert!(invoke(&desktop.window, "checkpoint_document", json!({"edit":edit})).is_ok());
    for position in [json!(-1), json!(1.5), json!(usize::MAX), Value::Null] {
        assert!(
            invoke(
                &desktop.window,
                "follow_link",
                json!({"edit":edit,"position":position})
            )
            .is_err()
        );
    }
    let here = invoke(
        &desktop.window,
        "follow_link",
        json!({"edit":edit,"position":2}),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(
        here["selection"],
        "[jump](#heading)\n".encode_utf16().count()
    );
    desktop
        .app
        .state::<AppState>()
        .registry
        .lock()
        .map_err(|error| error.to_string())?
        .document("note-main")
        .map_err(|error| error.message)?
        .close_allowed = true;
    assert_eq!(
        invoke(
            &desktop.window,
            "follow_link",
            json!({"edit":edit,"position":2})
        )
        .as_ref()
        .err()
        .map(|error| &error["kind"]),
        Some(&json!("stale"))
    );
    assert_eq!(desktop.app.webview_windows().len(), 1);
    Ok(())
}

#[test]
fn library_ipc_places_the_first_nonempty_checkpoint_and_completes_links()
-> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start_with_library()?;
    let initial = desktop.bootstrap()?;
    let token = initial["document"]["token"].clone();
    let id = desktop
        .app
        .state::<AppState>()
        .registry
        .lock()
        .map_err(|error| error.to_string())?
        .document("note-main")
        .map_err(|error| error.message)?
        .document
        .id();
    let front = format!("---\nid: {id}\n---\n\n");
    assert_eq!(initial["document"]["text"], front);
    assert_eq!(initial["document"]["path"], Value::Null);
    assert_eq!(initial["document"]["title"], Value::Null);
    assert_eq!(initial["selection"], json!(front.len()));
    assert_eq!(desktop.bootstrap()?["selection"], Value::Null);
    let blank = invoke(
        &desktop.window,
        "checkpoint_document",
        json!({"edit":{"token":token,"revision":1,"text":format!("{front}\n")}}),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(blank["path"], Value::Null);
    assert!(!desktop.directory.path().join("library").exists());
    let text = format!("{front}# Object\n\nSee [Obj");
    let placed = invoke(
        &desktop.window,
        "checkpoint_document",
        json!({"edit":{"token":token,"revision":2,"text":text}}),
    )
    .map_err(|error| error.to_string())?;
    let expected =
        fs::canonicalize(desktop.directory.path().join("library"))?.join(format!("{id}.md"));
    assert_eq!(placed["path"], json!(expected.to_string_lossy()));
    assert_eq!(placed["title"], json!("Object"));
    assert_eq!(placed["savedRevision"], 2);
    assert_eq!(fs::read_to_string(&expected)?, text);
    assert!(desktop.directory.path().join("library/assets").is_dir());
    assert!(
        desktop
            .app
            .state::<AppState>()
            .store
            .recover()?
            .documents
            .is_empty()
    );
    let candidates = invoke(
        &desktop.window,
        "list_library_notes",
        json!({"query":"obj"}),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(candidates[0]["title"], json!("Object"));
    assert_eq!(candidates[0]["id"], json!(id.to_string()));
    let from = text.find("[Obj").ok_or("Missing words")?;
    let edit = json!({"token":token,"revision":2,"text":text});
    let planned = invoke(
        &desktop.window,
        "insert_link",
        json!({"edit":edit,"link":{"from":from,"to":text.len(),"words":"Obj","image":false},"target":{"kind":"note","id":id.to_string()}}),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(planned["patches"][0]["insert"], json!("[Obj]"));
    assert_eq!(
        planned["patches"][1]["insert"],
        json!(format!("\n\n[Obj]: {id}.md\n"))
    );
    assert_eq!(planned["caret"], json!(from + 5));
    assert_eq!(desktop.bootstrap()?["document"]["text"], text);
    for (request, kind) in [
        (
            json!({"edit":{"token":token,"revision":1,"text":text},"link":{"from":from,"to":text.len(),"words":"Obj","image":false},"target":{"kind":"note","id":id.to_string()}}),
            "stale",
        ),
        (
            json!({"edit":edit,"link":{"from":from,"to":text.len(),"words":"","image":false},"target":{"kind":"note","id":id.to_string()}}),
            "link",
        ),
        (
            json!({"edit":edit,"link":{"from":from,"to":text.len(),"words":"Obj","image":true},"target":{"kind":"asset","digest":"short","extension":"png"}}),
            "archive",
        ),
    ] {
        assert_eq!(
            invoke(&desktop.window, "insert_link", request)
                .as_ref()
                .err()
                .map(|error| &error["kind"]),
            Some(&json!(kind))
        );
    }
    assert!(
        invoke(
            &desktop.window,
            "insert_link",
            json!({"edit":edit,"link":{"from":from,"to":text.len(),"words":"Obj","image":false},"target":{"kind":"other"}}),
        )
        .is_err()
    );
    assert!(
        invoke(
            &desktop.window,
            "insert_link",
            json!({"edit":edit,"link":{"from":from,"to":text.len(),"words":"Obj","image":false,"extra":1},"target":{"kind":"note","id":id.to_string()}}),
        )
        .is_err()
    );
    assert!(invoke(&desktop.window, "new_document", json!({})).is_ok());
    let other = desktop
        .app
        .webview_windows()
        .into_values()
        .find(|window| window.label() != desktop.window.label())
        .ok_or("Missing additional window")?;
    let fresh = invoke(&other, "bootstrap", json!({})).map_err(|error| error.to_string())?;
    assert!(
        fresh["document"]["text"]
            .as_str()
            .is_some_and(|text| text.starts_with("---\nid: "))
    );
    Ok(())
}

fn invoke_raw(
    window: &WebviewWindow<MockRuntime>,
    command: &str,
    body: Vec<u8>,
    extension: Option<&str>,
) -> Result<Value, Value> {
    let mut headers = tauri::http::HeaderMap::new();
    if let Some(extension) = extension {
        headers.insert(
            "celestite-extension",
            extension
                .parse()
                .map_err(|error| json!(format!("{error}")))?,
        );
    }
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: IPC_ORIGIN
                .parse()
                .map_err(|error| json!(format!("{error}")))?,
            body: InvokeBody::Raw(body),
            headers,
            invoke_key: INVOKE_KEY.into(),
        },
    )
    .and_then(|response| {
        response
            .deserialize::<Value>()
            .map_err(|error| json!(error.to_string()))
    })
}

#[test]
fn media_ipc_stores_pasted_bytes_once_and_completes_image_references() -> Result<(), Box<dyn Error>>
{
    let desktop = Desktop::start_with_library()?;
    let initial = desktop.bootstrap()?;
    let token = initial["document"]["token"].clone();
    let png = b"\x89PNG\r\n\x1a\n pasted".to_vec();
    let digest = celestite_archive::ObjectId::of(&png).digest().to_owned();
    let stored = invoke_raw(&desktop.window, "store_asset", png.clone(), Some("PNG"))
        .map_err(|error| error.to_string())?;
    assert_eq!(stored, json!({"digest": digest, "extension": "png"}));
    let path = desktop
        .directory
        .path()
        .join(format!("library/assets/{digest}.png"));
    assert_eq!(fs::read(&path)?, png);
    assert_eq!(
        invoke_raw(&desktop.window, "store_asset", png.clone(), Some("png")),
        Ok(stored.clone())
    );
    assert_eq!(
        fs::read_dir(desktop.directory.path().join("library/assets"))?.count(),
        1
    );
    for (body, extension, kind) in [
        (png.clone(), None, "internal"),
        (png.clone(), Some("exe"), "link"),
        (
            vec![0; crate::commands::MAX_PASTED_ASSET_BYTES + 1],
            Some("png"),
            "tooLarge",
        ),
    ] {
        assert_eq!(
            invoke_raw(&desktop.window, "store_asset", body, extension)
                .as_ref()
                .err()
                .map(|error| &error["kind"]),
            Some(&json!(kind))
        );
    }
    assert!(
        invoke(
            &desktop.window,
            "store_asset",
            json!({"bytes": [1, 2, 3], "extension": "png"})
        )
        .is_err()
    );
    let text = initial["document"]["text"]
        .as_str()
        .ok_or("Missing text")?
        .to_owned();
    let edit = json!({"token":token,"revision":0,"text":text});
    let planned = invoke(
        &desktop.window,
        "insert_link",
        json!({"edit":edit,"link":{"from":text.len(),"to":text.len(),"words":"pasted","image":true},"target":{"kind":"asset","digest":digest,"extension":"png"}}),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(planned["patches"][0]["insert"], json!("![pasted]"));
    assert_eq!(
        planned["patches"][1]["insert"],
        json!(format!("\n\n[pasted]: assets/{digest}.png\n"))
    );
    let outside = desktop.directory.path().join("outside.md");
    fs::write(&outside, "# Outside\n")?;
    assert!(
        invoke(
            &desktop.window,
            "open_document",
            json!({"token":token,"path":outside,"reuseCurrent":true}),
        )
        .map_err(|error| error.to_string())?["document"]
            .is_object()
    );
    assert_eq!(
        invoke_raw(&desktop.window, "store_asset", png, Some("png"))
            .as_ref()
            .err()
            .map(|error| &error["kind"]),
        Some(&json!("link"))
    );
    Ok(())
}

#[test]
fn ipc_refuses_a_stale_discard_without_removing_recovery() -> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    let initial = desktop.bootstrap()?;
    let token = &initial["document"]["token"];
    assert!(
        invoke(
            &desktop.window,
            "checkpoint_document",
            json!({"edit":{"token":token,"revision":2,"text":"newer draft"}})
        )
        .is_ok()
    );
    let result = invoke(
        &desktop.window,
        "confirm_close",
        json!({"token":token,"revision":1,"discard":true}),
    );
    assert_eq!(
        result.as_ref().err().map(|error| &error["kind"]),
        Some(&json!("stale"))
    );
    let state = desktop.app.state::<AppState>();
    assert_eq!(state.store.recover()?.documents.len(), 1);
    assert!(
        !state
            .registry
            .lock()
            .map_err(|error| error.to_string())?
            .document("note-main")
            .map_err(|error| error.message)?
            .close_allowed
    );
    Ok(())
}

#[test]
fn ipc_rejects_malformed_requests_and_cross_window_tokens() -> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    let initial = desktop.bootstrap()?;
    assert!(
        invoke(
            &desktop.window,
            "checkpoint_document",
            json!({"edit":{"revision":1}})
        )
        .is_err()
    );
    assert!(
        invoke(
            &desktop.window,
            "checkpoint_document",
            json!({"edit":{"token":"wrong","revision":1,"text":"wrong window"}})
        )
        .is_err()
    );
    assert_eq!(desktop.bootstrap()?["document"], initial["document"]);
    assert!(
        desktop
            .app
            .state::<AppState>()
            .store
            .recover()?
            .documents
            .is_empty()
    );
    assert!(
        fs::read_dir(desktop.directory.path().join("data/recovery"))?
            .next()
            .is_none()
    );
    Ok(())
}

#[test]
fn ipc_save_conflict_reload_and_rotated_tokens_use_real_files() -> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    let initial = desktop.bootstrap()?;
    let token = &initial["document"]["token"];
    let path = desktop.directory.path().join("note.md");
    let text = "# 日本語 🦀\n";
    let saved = invoke(
        &desktop.window,
        "save_document_as",
        json!({"edit":{"token":token,"revision":1,"text":text},"path":path}),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(saved["savedRevision"], 1);
    assert_eq!(fs::read_to_string(&path)?, text);
    let mut reopened = celestite_documents::Document::open(&path)?;
    desktop
        .app
        .state::<AppState>()
        .store
        .bind_document(&mut reopened)?;
    let id = desktop
        .app
        .state::<AppState>()
        .registry
        .lock()
        .map_err(|error| error.to_string())?
        .document("note-main")
        .map_err(|error| error.message)?
        .document
        .id();
    assert_eq!(reopened.id(), id);
    assert_eq!(
        invoke(&desktop.window, "inspect_document", json!({"token":token})),
        Ok(json!(false))
    );
    fs::write(&path, "external\n")?;
    assert_eq!(
        invoke(&desktop.window, "inspect_document", json!({"token":token})),
        Ok(json!(true))
    );
    let conflict = invoke(
        &desktop.window,
        "checkpoint_document",
        json!({"edit":{"token":token,"revision":2,"text":"local draft"}}),
    );
    assert_eq!(
        conflict.as_ref().err().map(|error| &error["kind"]),
        Some(&json!("conflict"))
    );
    assert_eq!(fs::read_to_string(&path)?, "external\n");
    assert_eq!(
        desktop
            .app
            .state::<AppState>()
            .store
            .recover()?
            .documents
            .first()
            .ok_or("Missing conflicting draft")?
            .snapshot()
            .text,
        "local draft"
    );
    let reloaded = invoke(&desktop.window, "reload_document", json!({"token":token}))
        .map_err(|error| error.to_string())?;
    assert_eq!(reloaded["text"], "external\n");
    assert_ne!(&reloaded["token"], token);
    assert!(
        desktop
            .app
            .state::<AppState>()
            .store
            .recover()?
            .documents
            .is_empty()
    );
    assert!(
        invoke(
            &desktop.window,
            "checkpoint_document",
            json!({"edit":{"token":token,"revision":3,"text":"obsolete"}})
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn ipc_isolates_windows_and_only_discards_the_confirmed_note() -> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    assert!(invoke(&desktop.window, "new_document", json!({})).is_ok());
    let other = desktop
        .app
        .webview_windows()
        .into_values()
        .find(|window| window.label() != desktop.window.label())
        .ok_or("Missing additional window")?;
    let first = desktop.bootstrap()?;
    let second = invoke(&other, "bootstrap", json!({})).map_err(|error| error.to_string())?;
    assert_ne!(first["document"]["token"], second["document"]["token"]);
    assert!(
        invoke(
            &other,
            "checkpoint_document",
            json!({"edit":{"token":first["document"]["token"],"revision":1,"text":"wrong note"}})
        )
        .is_err()
    );
    for (window, note, text) in [
        (&desktop.window, &first, "first"),
        (&other, &second, "second"),
    ] {
        assert!(
            invoke(
                window,
                "checkpoint_document",
                json!({"edit":{"token":note["document"]["token"],"revision":1,"text":text}})
            )
            .is_ok()
        );
    }
    assert!(
        invoke(
            &desktop.window,
            "confirm_close",
            json!({"token":first["document"]["token"],"revision":1,"discard":false})
        )
        .is_err()
    );
    assert_eq!(
        desktop
            .app
            .state::<AppState>()
            .store
            .recover()?
            .documents
            .len(),
        2
    );
    assert!(
        invoke(
            &desktop.window,
            "confirm_close",
            json!({"token":first["document"]["token"],"revision":1,"discard":true})
        )
        .is_ok()
    );
    let recovered = desktop.app.state::<AppState>().store.recover()?;
    assert_eq!(recovered.documents.len(), 1);
    assert_eq!(
        recovered
            .documents
            .first()
            .ok_or("Missing other draft")?
            .snapshot()
            .text,
        "second"
    );
    Ok(())
}

#[test]
fn ipc_open_preserves_nonempty_notes_and_reuses_empty_notes() -> Result<(), Box<dyn Error>> {
    for edited in [false, true] {
        let desktop = Desktop::start()?;
        let initial = desktop.bootstrap()?;
        let token = &initial["document"]["token"];
        if edited {
            assert!(
                invoke(
                    &desktop.window,
                    "checkpoint_document",
                    json!({"edit":{"token":token,"revision":1,"text":"keep draft"}})
                )
                .is_ok()
            );
        }
        let path = desktop.directory.path().join("opened.md");
        fs::write(&path, "\u{feff}opened\r\n")?;
        let result = invoke(
            &desktop.window,
            "open_document",
            json!({"token":token,"path":path,"reuseCurrent":true}),
        )
        .map_err(|error| error.to_string())?;
        if edited {
            assert!(result["document"].is_null());
            assert_eq!(desktop.app.webview_windows().len(), 2);
            assert_eq!(desktop.bootstrap()?["document"]["text"], "keep draft");
        } else {
            assert_eq!(result["document"]["text"], "opened\n");
            assert_ne!(&result["document"]["token"], token);
            assert_eq!(desktop.app.webview_windows().len(), 1);
        }
    }
    Ok(())
}

#[test]
fn ipc_appearance_is_validated_and_persisted() -> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    assert!(
        invoke(
            &desktop.window,
            "set_appearance",
            json!({"appearance":"invalid"})
        )
        .is_err()
    );
    assert_eq!(desktop.bootstrap()?["preferences"]["appearance"], "light");
    let dark = invoke(
        &desktop.window,
        "set_appearance",
        json!({"appearance":"dark"}),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(dark["appearance"], "dark");
    assert_eq!(
        desktop
            .app
            .state::<AppState>()
            .store
            .load_preferences()?
            .appearance,
        celestite_documents::Appearance::Dark
    );
    Ok(())
}

#[test]
fn a_failed_native_close_preserves_the_acknowledged_checkpoint_and_revokes_permission()
-> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    let initial = desktop.bootstrap()?;
    let token = initial["document"]["token"]
        .as_str()
        .ok_or("Missing token")?
        .to_owned();
    assert!(
        invoke(
            &desktop.window,
            "checkpoint_document",
            json!({"edit":{"token":token,"revision":1,"text":"keep after a failed close"}})
        )
        .is_ok()
    );
    let state = desktop.app.state::<AppState>();
    let result = tauri::async_runtime::block_on(crate::commands::close_note(
        &state,
        "note-main".into(),
        token,
        1,
        true,
        || {
            Err(crate::state::CommandError {
                kind: "window",
                message: "Injected native close failure".into(),
            })
        },
    ));
    assert_eq!(
        result.as_ref().err().map(|error| error.kind),
        Some("window")
    );
    let recovered = state.store.recover()?;
    assert_eq!(recovered.documents.len(), 1);
    assert_eq!(
        recovered
            .documents
            .first()
            .ok_or("Missing retained checkpoint")?
            .snapshot()
            .text,
        "keep after a failed close"
    );
    assert!(
        !state
            .registry
            .lock()
            .map_err(|error| error.to_string())?
            .document("note-main")
            .map_err(|error| error.message)?
            .close_allowed
    );
    assert!(
        state
            .registry
            .lock()
            .map_err(|error| error.to_string())?
            .pending_closes
            .is_empty()
    );
    Ok(())
}

#[test]
fn native_acceptance_precedes_checkpoint_cleanup_and_exit_permission() -> Result<(), Box<dyn Error>>
{
    let desktop = Desktop::start()?;
    let initial = desktop.bootstrap()?;
    let token = initial["document"]["token"]
        .as_str()
        .ok_or("Missing token")?
        .to_owned();
    assert!(
        invoke(
            &desktop.window,
            "checkpoint_document",
            json!({"edit":{"token":token,"revision":1,"text":"retain until native acceptance"}})
        )
        .is_ok()
    );
    let state = desktop.app.state::<AppState>();
    let result = tauri::async_runtime::block_on(crate::commands::close_note(
        &state,
        "note-main".into(),
        token,
        1,
        true,
        || {
            assert_eq!(
                state
                    .store
                    .recover()
                    .map_err(crate::state::CommandError::from)?
                    .documents
                    .len(),
                1
            );
            let mut registry = state
                .registry
                .lock()
                .map_err(crate::state::CommandError::internal)?;
            assert_eq!(registry.pending_closes.len(), 1);
            registry.windows.remove("note-main");
            assert_eq!(
                celestite_core::plan_exit(false, !registry.pending_closes.is_empty()),
                celestite_core::ExitDecision::WaitForCleanup
            );
            Ok(())
        },
    ));
    result.map_err(|error| error.message)?;
    assert!(state.store.recover()?.documents.is_empty());
    assert!(
        state
            .registry
            .lock()
            .map_err(|error| error.to_string())?
            .pending_closes
            .is_empty()
    );
    assert_eq!(
        celestite_core::plan_exit(false, false),
        celestite_core::ExitDecision::Exit
    );
    Ok(())
}

#[test]
fn duplicate_close_requests_cannot_reuse_an_in_flight_permission() -> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    let initial = desktop.bootstrap()?;
    let token = initial["document"]["token"]
        .as_str()
        .ok_or("Missing token")?
        .to_owned();
    assert!(
        invoke(
            &desktop.window,
            "checkpoint_document",
            json!({"edit":{"token":token,"revision":1,"text":"one close attempt"}})
        )
        .is_ok()
    );
    let state = desktop.app.state::<AppState>();
    {
        let mut registry = state.registry.lock().map_err(|error| error.to_string())?;
        let entry = registry
            .document("note-main")
            .map_err(|error| error.message)?;
        entry.close_allowed = true;
        let id = entry.document.checkpoint_key();
        registry.pending_closes.insert(id);
    }
    let mut called = false;
    let result = tauri::async_runtime::block_on(crate::commands::close_note(
        &state,
        "note-main".into(),
        token,
        1,
        true,
        || {
            called = true;
            Ok(())
        },
    ));
    assert_eq!(result.as_ref().err().map(|error| error.kind), Some("stale"));
    assert!(!called);
    assert_eq!(state.store.recover()?.documents.len(), 1);
    Ok(())
}

#[test]
fn quit_requests_notify_each_webview_exactly_once() -> Result<(), Box<dyn Error>> {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tauri::Listener;
    let desktop = Desktop::start()?;
    assert!(invoke(&desktop.window, "new_document", json!({})).is_ok());
    let other = desktop
        .app
        .webview_windows()
        .into_values()
        .find(|window| window.label() != "note-main")
        .ok_or("Missing other window")?;
    let first = Arc::new(AtomicUsize::new(0));
    let second = Arc::new(AtomicUsize::new(0));
    let first_count = Arc::clone(&first);
    let second_count = Arc::clone(&second);
    desktop
        .window
        .listen("celestite:close-requested", move |_| {
            first_count.fetch_add(1, Ordering::SeqCst);
        });
    other.listen("celestite:close-requested", move |_| {
        second_count.fetch_add(1, Ordering::SeqCst);
    });
    crate::request_close_all(desktop.app.handle());
    assert_eq!(first.load(Ordering::SeqCst), 1);
    assert_eq!(second.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn a_close_permission_refuses_late_editor_writes_without_changing_history()
-> Result<(), Box<dyn Error>> {
    let desktop = Desktop::start()?;
    let initial = desktop.bootstrap()?;
    let state = desktop.app.state::<AppState>();
    let before = state.store.archive().current()?;
    state
        .registry
        .lock()
        .map_err(|error| error.to_string())?
        .document("note-main")
        .map_err(|error| error.message)?
        .close_allowed = true;
    let result = invoke(
        &desktop.window,
        "checkpoint_document",
        json!({
            "edit": {"token": initial["document"]["token"], "revision": 1, "text": "too late"}
        }),
    );
    assert_eq!(
        result.as_ref().err().map(|error| &error["kind"]),
        Some(&json!("stale"))
    );
    assert_eq!(desktop.bootstrap()?["document"], initial["document"]);
    assert_eq!(state.store.archive().current()?, before);
    assert!(state.store.recover()?.documents.is_empty());
    Ok(())
}

#[test]
fn a_failed_rollback_preserves_the_primary_close_error_and_recovery() -> Result<(), Box<dyn Error>>
{
    let desktop = Desktop::start()?;
    let initial = desktop.bootstrap()?;
    let token = initial["document"]["token"]
        .as_str()
        .ok_or("Missing token")?
        .to_owned();
    assert!(
        invoke(
            &desktop.window,
            "checkpoint_document",
            json!({"edit":{"token":token,"revision":1,"text":"retained during rollback failure"}})
        )
        .is_ok()
    );
    let state = desktop.app.state::<AppState>();
    let registry = std::sync::Arc::clone(&state.registry);
    let result = tauri::async_runtime::block_on(crate::commands::close_note(
        &state,
        "note-main".into(),
        token,
        1,
        true,
        move || {
            let poisoned = std::thread::spawn(move || {
                if let Ok(_guard) = registry.lock() {
                    panic!("Injected registry poison");
                }
            });
            assert!(poisoned.join().is_err());
            Err(crate::state::CommandError {
                kind: "window",
                message: "Primary native close failure".into(),
            })
        },
    ));
    let error = result.err().ok_or("Missing native close error")?;
    assert_eq!(error.kind, "window");
    assert_eq!(error.message, "Primary native close failure");
    assert!(state.registry.is_poisoned());
    let restored = state.store.recover()?;
    assert_eq!(
        restored
            .documents
            .first()
            .ok_or("Missing preserved draft")?
            .snapshot()
            .text,
        "retained during rollback failure"
    );
    Ok(())
}
