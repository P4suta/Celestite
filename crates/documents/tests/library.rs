use std::{error::Error, fs, thread, time::Duration};

use celestite_documents::{
    Document, DocumentError, LinkDestination, LinkInsertion, LinkNavigation, NoteStore,
};
use tempfile::tempdir;

fn store_with_library(area: &std::path::Path) -> Result<NoteStore, DocumentError> {
    Ok(NoteStore::new(area.join("state"))?.with_library(Some(area.join("library"))))
}

#[test]
fn an_untitled_note_is_written_into_the_library_at_its_first_nonempty_checkpoint()
-> Result<(), Box<dyn Error>> {
    let area = tempdir()?;
    let store = store_with_library(area.path())?;
    let mut note = Document::new_in_library();
    let token = note.snapshot().token;
    assert!(note.body_is_empty());
    assert!(note.may_reuse());
    assert!(!store.place_in_library(&mut note)?);
    assert!(note.path().is_none());
    assert!(!area.path().join("library").exists());
    note.edit(&token, 1, format!("{}# Object\n", note.snapshot().text))?;
    assert!(!note.may_reuse());
    assert!(store.place_in_library(&mut note)?);
    store.checkpoint(&note)?;
    let expected = area
        .path()
        .join("library")
        .join(format!("{}.md", note.id()));
    assert_eq!(note.path(), Some(fs::canonicalize(&expected)?.as_path()));
    assert_eq!(
        fs::read_to_string(&expected)?,
        format!("---\nid: {}\n---\n\n# Object\n", note.id())
    );
    assert!(area.path().join("library/assets").is_dir());
    assert_eq!(note.snapshot().title.as_deref(), Some("Object"));
    assert!(!store.place_in_library(&mut note)?);
    let head = store.archive().current()?.ok_or("Missing capture")?;
    let notes = store.archive().notes(&head)?;
    let celestite_archive::Entry::Live { record } = notes
        .get(&note.id().to_string().parse()?)
        .ok_or("Missing note")?
    else {
        return Err("Unexpected conflict".into());
    };
    assert_eq!(
        store.archive().note(record)?.path,
        format!("{}.md", note.id())
    );
    let candidates = store.library_notes("obj")?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].title.as_deref(), Some("Object"));
    assert_eq!(candidates[0].id.uuid(), note.id());
    Ok(())
}

#[test]
fn standalone_notes_and_stores_without_a_library_keep_the_save_as_model()
-> Result<(), Box<dyn Error>> {
    let area = tempdir()?;
    let store = NoteStore::new(area.path().join("state"))?;
    let mut note = Document::new();
    note.edit(&note.snapshot().token, 1, String::from("text\n"))?;
    assert!(!store.place_in_library(&mut note)?);
    assert!(note.path().is_none());
    assert!(store.library_notes("")?.is_empty());
    drop(store);
    let store = store_with_library(area.path())?;
    let path = area.path().join("elsewhere.md");
    fs::write(&path, "# Elsewhere\n")?;
    let mut opened = Document::open(&path)?;
    store.bind_document(&mut opened)?;
    assert!(!store.place_in_library(&mut opened)?);
    assert_eq!(opened.path(), Some(fs::canonicalize(&path)?.as_path()));
    Ok(())
}

#[test]
fn the_title_index_follows_external_changes_and_ranks_titles_first() -> Result<(), Box<dyn Error>> {
    let area = tempdir()?;
    let store = store_with_library(area.path())?;
    let library = area.path().join("library");
    fs::create_dir_all(&library)?;
    let older: celestite_archive::NoteId = "019a3f1e-7c2d-7b8a-9e21-0f4b6c8d1a2e".parse()?;
    let newer: celestite_archive::NoteId = "019a3f25-9b0e-7d12-a3c7-5e1f8a2b6d90".parse()?;
    let third: celestite_archive::NoteId = "019a3f21-13aa-7c01-8c55-2b9e7d0f3c44".parse()?;
    fs::write(library.join(format!("{older}.md")), "# Object\n")?;
    fs::write(library.join(format!("{newer}.md")), "# Object\n")?;
    fs::write(
        library.join(format!("{third}.md")),
        "# Subject and object\n",
    )?;
    fs::write(library.join("readme.md"), "# Not a note identifier\n")?;
    fs::write(
        library.join(format!("{}.md", "019a3f26-0000-7000-8000-000000000001")),
        format!("---\nid: {older}\n---\n# Impostor\n"),
    )?;
    let candidates = store.library_notes("obj")?;
    assert_eq!(
        candidates.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![newer, older, third]
    );
    assert_eq!(store.library_notes("")?.len(), 3);
    assert_eq!(store.library_notes("019a3f21")?[0].id, third);
    assert!(store.library_notes("nothing")?.is_empty());
    let warnings = store.library_warnings();
    assert!(warnings.iter().any(|warning| warning.contains("readme.md")));
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("different identity"))
    );
    thread::sleep(Duration::from_millis(20));
    fs::write(library.join(format!("{third}.md")), "# Renamed\n")?;
    fs::remove_file(library.join(format!("{older}.md")))?;
    let candidates = store.library_notes("ren")?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].title.as_deref(), Some("Renamed"));
    assert_eq!(store.library_notes("")?.len(), 2);
    Ok(())
}

#[test]
fn reference_links_between_library_notes_open_by_identity() -> Result<(), Box<dyn Error>> {
    let area = tempdir()?;
    let store = store_with_library(area.path())?;
    let library = area.path().join("library");
    fs::create_dir_all(library.join("assets"))?;
    let target = Document::new_in_library();
    let target_path = library.join(format!("{}.md", target.id()));
    fs::write(&target_path, "# Target\n\n## Second\n")?;
    let digest = celestite_archive::ObjectId::of(b"pixels");
    let asset = format!("assets/{}.png", digest.digest());
    fs::write(library.join(&asset), b"pixels")?;
    let mut source = Document::new_in_library();
    let token = source.snapshot().token;
    source.edit(&token, 1, String::from("See [Target] and ![pic]\n"))?;
    store.place_in_library(&mut source)?;
    let words = |from: usize, to: usize, words: &str, image: bool| LinkInsertion {
        from,
        to,
        words: words.to_owned(),
        image,
    };
    let edit = source.insert_link(
        &token,
        1,
        &source.snapshot().text,
        &words(4, 12, "Target", false),
        &LinkDestination::Note(target.id().to_string().parse()?),
    )?;
    assert_eq!(edit.patches.len(), 2);
    let mut text = source.snapshot().text;
    for patch in edit.patches.iter().rev() {
        text.replace_range(patch.start..patch.end, &patch.insert);
    }
    assert!(text.ends_with(&format!("[Target]: {}.md\n", target.id())));
    let edit = source.insert_link(
        &token,
        1,
        &source.snapshot().text,
        &words(17, 23, "pic", true),
        &LinkDestination::Asset {
            digest: digest.clone(),
            extension: String::from("png"),
        },
    )?;
    let mut image_text = source.snapshot().text;
    for patch in edit.patches.iter().rev() {
        image_text.replace_range(patch.start..patch.end, &patch.insert);
    }
    assert!(image_text.ends_with(&format!("[pic]: {asset}\n")));
    let combined = format!(
        "See [Target] and ![pic]\n\n[Target]: {}.md#second\n[pic]: {asset}\n",
        target.id()
    );
    source.edit(&token, 2, combined.clone())?;
    store.checkpoint(&source)?;
    let snapshot = source.snapshot();
    let position = combined.find("Target]").ok_or("Missing link")?;
    let LinkNavigation::Note {
        document,
        selection,
    } = source.follow_link(&snapshot.token, snapshot.revision, &combined, position)?
    else {
        return Err("Expected a note window".into());
    };
    assert_eq!(document.id(), target.id());
    assert_eq!(selection, Some("# Target\n\n".encode_utf16().count()));
    let position = combined.find("pic]").ok_or("Missing image")?;
    assert!(matches!(
        source.follow_link(&snapshot.token, snapshot.revision, &combined, position)?,
        LinkNavigation::Attachment { .. }
    ));
    fs::remove_file(&target_path)?;
    let position = combined.find("Target]").ok_or("Missing link")?;
    let error = source
        .follow_link(&snapshot.token, snapshot.revision, &combined, position)
        .err()
        .ok_or("Expected a missing note error")?;
    assert_eq!(error.to_string(), "This note is not in the library.");
    assert!(matches!(
        source.insert_link(
            &token,
            1,
            "stale",
            &words(0, 0, "x", false),
            &LinkDestination::Note(target.id().to_string().parse()?),
        ),
        Err(DocumentError::Stale)
    ));
    assert!(matches!(
        source.insert_link(
            &token,
            2,
            &combined,
            &words(5, 3, "x", false),
            &LinkDestination::Note(target.id().to_string().parse()?),
        ),
        Err(DocumentError::LinkEdit(_))
    ));
    Ok(())
}
