use std::{fs, path::Path};

use celestite_documents::{Document, DocumentError};
use tempfile::tempdir;

#[test]
fn saves_unicode_markdown_without_changing_the_file_format()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("note.md");
    fs::write(&path, "\u{feff}# Notes\r\n日本語とRust 🦀\r\n")?;
    let mut document = Document::open(&path)?;
    let snapshot = document.snapshot();
    assert_eq!(snapshot.text, "# Notes\n日本語とRust 🦀\n");
    document.edit(&snapshot.token, 1, format!("{}More.\n", snapshot.text))?;
    document.save()?;
    assert_eq!(
        fs::read_to_string(path)?,
        "\u{feff}# Notes\r\n日本語とRust 🦀\r\nMore.\r\n"
    );
    assert_eq!(document.snapshot().saved_revision, 1);
    Ok(())
}

#[test]
fn refuses_external_changes_and_preserves_both_buffers() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("note.md");
    fs::write(&path, "original")?;
    let mut document = Document::open(&path)?;
    document.edit(&document.snapshot().token, 1, String::from("my edit"))?;
    fs::write(&path, "external edit")?;
    assert!(matches!(document.save(), Err(DocumentError::Conflict)));
    assert_eq!(fs::read_to_string(path)?, "external edit");
    assert_eq!(document.snapshot().text, "my edit");
    Ok(())
}

#[test]
fn rejects_stale_revisions_before_they_can_replace_a_newer_save()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("note.md");
    fs::write(&path, "original")?;
    let mut document = Document::open(&path)?;
    let token = document.snapshot().token;
    document.edit(&token, 2, String::from("newer"))?;
    document.save()?;
    assert!(matches!(
        document.edit(&token, 1, String::from("older")),
        Err(DocumentError::Stale)
    ));
    assert_eq!(fs::read_to_string(path)?, "newer");
    Ok(())
}

#[test]
fn a_failed_save_as_keeps_the_original_path_and_unsaved_text()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("note.md");
    fs::write(&path, "original")?;
    let mut document = Document::open(&path)?;
    let original = document.snapshot();
    document.edit(&original.token, 1, String::from("my edit"))?;
    assert!(document.save_as(directory.path()).is_err());
    assert_eq!(document.snapshot().path, original.path);
    assert_eq!(document.snapshot().text, "my edit");
    assert_eq!(fs::read_to_string(path)?, "original");
    Ok(())
}

#[test]
fn an_untitled_note_can_be_saved_as_a_plain_file() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("first.md");
    let mut document = Document::new();
    document.edit(&document.snapshot().token, 1, String::from("# Hello\n"))?;
    document.save_as(&path)?;
    assert_eq!(fs::read_to_string(path)?, "# Hello\n");
    assert_eq!(document.snapshot().name, "first.md");
    Ok(())
}

#[test]
fn an_old_window_token_cannot_edit_a_different_document() {
    let mut document = Document::new();
    assert!(matches!(
        document.edit("old-token", 1, String::from("wrong note")),
        Err(DocumentError::Stale)
    ));
    assert!(document.snapshot().text.is_empty());
}

#[test]
fn a_deleted_file_is_not_silently_recreated() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("note.md");
    fs::write(&path, "original")?;
    let mut document = Document::open(&path)?;
    document.edit(&document.snapshot().token, 1, String::from("my edit"))?;
    fs::remove_file(&path)?;
    assert!(matches!(document.save(), Err(DocumentError::Conflict)));
    assert!(!Path::new(&path).exists());
    Ok(())
}

#[test]
fn encoded_size_limit_preserves_the_original_and_refuses_an_unreadable_copy()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("windows-line-endings.md");
    let copy = directory.path().join("copy.md");
    fs::write(&path, "original\r\n")?;
    let mut document = Document::open(&path)?;
    let text = "\n".repeat(celestite_documents::MAX_FILE_BYTES / 2 + 1);
    document.edit(&document.snapshot().token, 1, text)?;
    assert!(matches!(document.save(), Err(DocumentError::TooLarge)));
    assert!(matches!(
        document.save_as(&copy),
        Err(DocumentError::TooLarge)
    ));
    assert_eq!(fs::read_to_string(path)?, "original\r\n");
    assert!(!copy.exists());
    assert!(document.has_unsaved_changes());
    Ok(())
}
