use std::fs;

use celestite_documents::{Appearance, Document, DocumentError, NoteStore, Preferences};
use tempfile::tempdir;

#[test]
fn restores_an_untitled_draft_with_a_fresh_window_token() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempdir()?;
    let store = NoteStore::new(directory.path().to_path_buf())?;
    let mut document = Document::new();
    let token = document.snapshot().token;
    document.edit(&token, 3, String::from("# Unfinished\n日本語 🦀\n"))?;
    store.checkpoint(&document)?;
    let restored = store.recover()?;
    assert!(restored.warnings.is_empty());
    assert_eq!(restored.documents.len(), 1);
    let mut recovered = restored
        .documents
        .into_iter()
        .next()
        .ok_or("Missing recovery")?;
    assert_eq!(recovered.id(), document.id());
    assert_eq!(recovered.snapshot().name, document.snapshot().name);
    assert_ne!(recovered.snapshot().token, token);
    assert_eq!(recovered.snapshot().text, document.snapshot().text);
    assert_eq!(recovered.snapshot().revision, 3);
    assert!(recovered.has_unsaved_changes());
    assert!(matches!(
        recovered.edit(&token, 4, String::from("stale")),
        Err(DocumentError::Stale)
    ));
    store.remove_checkpoint(recovered.checkpoint_key())?;
    assert!(store.recover()?.documents.is_empty());
    Ok(())
}

#[test]
fn recovery_cannot_overwrite_a_file_changed_after_the_checkpoint()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("note.md");
    fs::write(&path, "original")?;
    let store = NoteStore::new(directory.path().join("data"))?;
    let mut document = Document::open(&path)?;
    document.edit(&document.snapshot().token, 1, String::from("local draft"))?;
    store.checkpoint(&document)?;
    fs::write(&path, "external edit")?;
    let mut restored = store
        .recover()?
        .documents
        .into_iter()
        .next()
        .ok_or("Missing recovery")?;
    assert!(matches!(restored.save(), Err(DocumentError::Conflict)));
    assert_eq!(restored.snapshot().text, "local draft");
    assert_eq!(fs::read_to_string(path)?, "external edit");
    Ok(())
}

#[test]
fn recognizes_a_save_completed_before_recovery_cleanup() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("note.md");
    fs::write(&path, "original")?;
    let store = NoteStore::new(directory.path().join("data"))?;
    let mut document = Document::open(&path)?;
    document.edit(
        &document.snapshot().token,
        1,
        String::from("saved before a crash"),
    )?;
    store.checkpoint(&document)?;
    document.save()?;
    let mut restored = store
        .recover()?
        .documents
        .into_iter()
        .next()
        .ok_or("Missing recovery")?;
    assert!(!restored.has_unsaved_changes());
    restored.edit(&restored.snapshot().token, 2, String::from("continued"))?;
    restored.save()?;
    assert_eq!(fs::read_to_string(path)?, "continued");
    Ok(())
}

#[test]
fn reports_corrupt_recovery_without_deleting_it_or_valid_drafts()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store = NoteStore::new(directory.path().to_path_buf())?;
    let mut document = Document::new();
    document.edit(&document.snapshot().token, 1, String::from("valid"))?;
    store.checkpoint(&document)?;
    let corrupt = directory.path().join("recovery/broken.json");
    fs::write(&corrupt, b"{broken")?;
    let restored = store.recover()?;
    assert_eq!(restored.documents.len(), 1);
    assert_eq!(restored.warnings.len(), 1);
    assert_eq!(fs::read(corrupt)?, b"{broken");
    Ok(())
}

#[test]
fn preferences_round_trip_and_corruption_is_an_explicit_error()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store = NoteStore::new(directory.path().to_path_buf())?;
    assert_eq!(store.load_preferences()?.appearance, Appearance::Light);
    store.save_preferences(&Preferences {
        appearance: Appearance::Dark,
        recent_files: vec![directory.path().join("note.md")],
        library: Some(directory.path().join("library")),
    })?;
    let restored = store.load_preferences()?;
    assert_eq!(restored.appearance, Appearance::Dark);
    assert_eq!(restored.recent_files.len(), 1);
    assert_eq!(restored.library, Some(directory.path().join("library")));
    fs::write(directory.path().join("preferences.json"), b"not json")?;
    assert!(store.load_preferences().is_err());
    Ok(())
}

#[test]
fn a_read_only_note_cannot_be_replaced_by_autosave() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("read-only.md");
    fs::write(&path, "original")?;
    let mut document = Document::open(&path)?;
    document.edit(&document.snapshot().token, 1, String::from("local edit"))?;
    let mut permissions = fs::metadata(&path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions)?;
    assert!(document.save().is_err());
    assert_eq!(fs::read_to_string(path)?, "original");
    assert!(document.has_unsaved_changes());
    Ok(())
}

#[test]
fn rejects_non_utf8_and_oversized_files_without_writing() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempdir()?;
    let invalid = directory.path().join("invalid.md");
    fs::write(&invalid, [0xff, 0xfe, 0x00])?;
    assert!(matches!(
        Document::open(&invalid),
        Err(DocumentError::InvalidEncoding)
    ));
    let large = directory.path().join("large.md");
    fs::File::create(&large)?.set_len(celestite_documents::MAX_FILE_BYTES as u64 + 1)?;
    assert!(matches!(
        Document::open(&large),
        Err(DocumentError::TooLarge)
    ));
    Ok(())
}
