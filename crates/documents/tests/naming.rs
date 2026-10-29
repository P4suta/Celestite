use std::fs;

use celestite_documents::{Document, NoteStore};
use tempfile::tempdir;
use uuid::{Uuid, Version};

#[test]
fn new_notes_have_distinct_time_ordered_uuid_filenames() -> Result<(), Box<dyn std::error::Error>> {
    let first = Document::new();
    let second = Document::new();
    let name = first.snapshot().name;
    let id = Uuid::parse_str(
        name.strip_suffix(".md")
            .ok_or("Missing Markdown extension")?,
    )?;
    assert_eq!(id, first.id());
    assert_eq!(id.get_version(), Some(Version::SortRand));
    assert!(name < second.snapshot().name);
    Ok(())
}

#[test]
fn legacy_recovery_keeps_its_existing_document_id() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store = NoteStore::new(directory.path().to_path_buf())?;
    let id = Uuid::parse_str("a1a2a3a4-b1b2-4c1c-8d1d-f1f2f3f4f5f6")?;
    let path = directory.path().join("recovery").join(format!("{id}.json"));
    let record = serde_json::json!({
        "format": 1,
        "id": id,
        "text": "legacy draft",
        "path": null,
        "revision": 1,
        "saved_revision": 0,
        "fingerprint": null,
        "text_format": { "ending": "Lf", "bom": false }
    });
    fs::write(path, serde_json::to_vec(&record)?)?;
    let report = store.recover()?;
    assert!(report.warnings.is_empty());
    let recovered = report
        .documents
        .into_iter()
        .next()
        .ok_or("Missing recovery")?;
    assert_eq!(recovered.id(), id);
    assert_eq!(recovered.snapshot().name, format!("{id}.md"));
    assert_eq!(recovered.snapshot().text, "legacy draft");
    Ok(())
}
