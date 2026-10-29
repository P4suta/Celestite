use std::{error::Error, fs};

use celestite_documents::{Document, DocumentError, NoteStore};
use serde_json::{Value, json};

#[test]
fn every_truncation_is_reported_without_deleting_or_rewriting_recovery()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let store = NoteStore::new(directory.path().to_path_buf())?;
    let mut note = Document::new();
    note.edit(&note.snapshot().token, 7, "# Keep 日本語 🦀\n\u{0}".into())?;
    store.checkpoint(&note)?;
    let path = directory
        .path()
        .join("recovery")
        .join(format!("{}.json", note.checkpoint_key()));
    let complete = fs::read(&path)?;
    for length in 0..complete.len() {
        let truncated = complete.get(..length).ok_or("Invalid truncation")?;
        fs::write(&path, truncated)?;
        let report = store.recover()?;
        assert!(
            report.documents.is_empty(),
            "accepted truncated record of {length} bytes"
        );
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(fs::read(&path)?, truncated);
    }
    fs::write(&path, &complete)?;
    let restored = store.recover()?;
    assert!(restored.warnings.is_empty());
    assert_eq!(
        restored
            .documents
            .first()
            .ok_or("Missing recovery")?
            .snapshot()
            .text,
        note.snapshot().text
    );
    Ok(())
}

#[test]
fn malformed_metadata_never_restores_a_note_or_modifies_the_record() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let store = NoteStore::new(directory.path().to_path_buf())?;
    let note = Document::new();
    store.checkpoint(&note)?;
    let path = directory
        .path()
        .join("recovery")
        .join(format!("{}.json", note.checkpoint_key()));
    let valid: Value = serde_json::from_slice(&fs::read(&path)?)?;
    for (field, value) in [
        ("format", json!(0)),
        ("format", json!(2)),
        ("format", json!("1")),
        ("id", json!("invalid")),
        ("id", json!(uuid::Uuid::new_v4())),
        ("revision", json!(-1)),
        ("revision", json!(9_007_199_254_740_992_u64)),
        ("saved_revision", json!(1)),
        ("text", json!(null)),
        ("path", json!("relative.md")),
        ("fingerprint", json!(vec![0; 32])),
        ("text_format", json!({"ending":"Unknown","bom":false})),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        let bytes = serde_json::to_vec(&invalid)?;
        fs::write(&path, &bytes)?;
        let report = store.recover()?;
        assert!(report.documents.is_empty(), "accepted invalid {field}");
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(fs::read(&path)?, bytes);
    }
    Ok(())
}

#[test]
fn preferences_are_bounded_before_json_decoding() -> Result<(), Box<dyn Error>> {
    const PREFERENCES_LIMIT_BYTES: u64 = 1024 * 1024;
    let directory = tempfile::tempdir()?;
    let store = NoteStore::new(directory.path().to_path_buf())?;
    let path = directory.path().join("preferences.json");
    fs::File::create(&path)?.set_len(PREFERENCES_LIMIT_BYTES + 1)?;
    assert!(matches!(
        store.load_preferences(),
        Err(DocumentError::TooLarge)
    ));
    assert_eq!(fs::metadata(path)?.len(), PREFERENCES_LIMIT_BYTES + 1);
    Ok(())
}
