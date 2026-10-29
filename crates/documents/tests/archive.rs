use celestite_documents::{Document, NoteStore};

#[test]
fn an_unseen_archive_revision_cannot_be_acknowledged_as_the_editor_text()
-> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let file = area.path().join("note.md");
    fs::write(&file, "base\n")?;
    let store = NoteStore::new(area.path().join("state"))?;
    let mut editor = Document::open(&file)?;
    store.bind_document(&mut editor)?;
    let base = store
        .archive()
        .current()?
        .ok_or("Missing initial capture")?;
    store.archive().capture_file(
        editor.id().to_string().parse()?,
        Some(&file),
        b"remote\n",
        Some(base),
        None,
    )?;
    assert!(
        store.checkpoint(&editor).is_err(),
        "An old unchanged buffer cannot acknowledge another branch's text"
    );
    editor.edit(&editor.snapshot().token, 1, String::from("local\n"))?;
    assert!(
        store.checkpoint(&editor).is_err(),
        "The unseen remote base must remain a conflict"
    );
    Ok(())
}
use std::{error::Error, fs};

#[test]
fn importing_before_editing_preserves_original_bytes_and_persistent_identity()
-> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let path = area.path().join("original.md");
    let original = b"\xef\xbb\xbf# Original\r\nSecond\nThird\r";
    fs::write(&path, original)?;
    let store = NoteStore::new(area.path().join("state"))?;
    let mut first = Document::open(&path)?;
    store.bind_document(&mut first)?;
    let id = first.id();
    let base = store.archive().current()?.ok_or("Missing import")?;
    first.edit(&first.snapshot().token, 1, String::from("changed\n"))?;
    store.checkpoint(&first)?;
    first.save()?;
    let mut reopened = Document::open(&path)?;
    store.bind_document(&mut reopened)?;
    assert_eq!(reopened.id(), id);
    let old = store.archive().notes(&base)?;
    let entry = old
        .get(&id.to_string().parse()?)
        .ok_or("Missing preserved note")?;
    let record = entry
        .alternatives()
        .into_iter()
        .flatten()
        .next()
        .ok_or("Missing original record")?;
    let note = store.archive().note(&record)?;
    assert_eq!(
        store.archive().read_bytes(&note.original, original.len())?,
        original
    );
    Ok(())
}

#[test]
fn windows_of_the_same_note_keep_separate_recovery_records_and_conflicting_versions()
-> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let path = area.path().join("note.md");
    fs::write(&path, "base\n")?;
    let store = NoteStore::new(area.path().join("state"))?;
    let mut a = Document::open(&path)?;
    store.bind_document(&mut a)?;
    let mut b = Document::open(&path)?;
    store.bind_document(&mut b)?;
    assert_eq!(a.id(), b.id());
    assert_ne!(a.checkpoint_key(), b.checkpoint_key());
    a.edit(&a.snapshot().token, 1, String::from("left\n"))?;
    store.checkpoint(&a)?;
    b.edit(&b.snapshot().token, 1, String::from("right\n"))?;
    assert!(store.checkpoint(&b).is_err());
    assert!(
        store.checkpoint(&b).is_err(),
        "Repeating a conflicted checkpoint must not imply resolution"
    );
    let recovery = store.recover()?;
    assert_eq!(recovery.documents.len(), 2);
    let text: Vec<_> = recovery
        .documents
        .iter()
        .map(|note| note.snapshot().text)
        .collect();
    assert!(text.contains(&String::from("left\n")) && text.contains(&String::from("right\n")));
    store.remove_checkpoint(a.checkpoint_key())?;
    assert_eq!(store.recover()?.documents.len(), 1);
    let head = store.archive().current()?.ok_or("Missing archive")?;
    let notes = store.archive().notes(&head)?;
    assert!(
        notes
            .get(&a.id().to_string().parse()?)
            .ok_or("Missing note")?
            .alternatives()
            .len()
            >= 2
    );
    Ok(())
}
