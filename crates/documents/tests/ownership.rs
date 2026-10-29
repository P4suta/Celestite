use celestite_documents::{Document, DocumentError, NoteStore};
use tempfile::tempdir;

#[test]
fn only_one_live_store_can_own_recovery_records() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let first = NoteStore::new(directory.path().to_path_buf())?;
    let mut document = Document::new();
    document.edit(&document.snapshot().token, 1, String::from("owned draft"))?;
    first.checkpoint(&document)?;
    assert!(matches!(
        NoteStore::new(directory.path().to_path_buf()),
        Err(DocumentError::AlreadyRunning)
    ));
    assert_eq!(first.recover()?.documents.len(), 1);
    drop(first);
    let reopened = NoteStore::new(directory.path().to_path_buf())?;
    assert_eq!(reopened.recover()?.documents.len(), 1);
    Ok(())
}
