use celestite_archive::{ArchiveError, NoteId, Vault};
use std::{
    error::Error,
    fs::{self, OpenOptions},
};

#[test]
fn cache_queries_and_rebuilds_share_archive_writer_ownership() -> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let vault = Vault::init(area.path())?;
    let first = vault.capture(NoteId::new(), "note.md", b"alpha", None, None)?;
    vault.rebuild_index(&first.snapshot)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(vault.root().join("local/writer.lock"))?;
    lock.try_lock()?;
    assert!(matches!(
        vault.search(&first.snapshot, "alpha"),
        Err(ArchiveError::Locked)
    ));
    assert!(matches!(
        vault.rebuild_index(&first.snapshot),
        Err(ArchiveError::Locked)
    ));
    lock.unlock()?;
    assert_eq!(vault.search(&first.snapshot, "alpha")?.len(), 1);
    Ok(())
}

#[test]
fn search_selects_the_requested_snapshot_and_treats_queries_as_literal_text()
-> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let vault = Vault::init(area.path())?;
    let id = NoteId::new();
    let alpha = vault.capture(id, "alpha.md", b"alpha 100% a_b", None, None)?;
    let beta = vault.capture(
        id,
        "beta.md",
        b"beta plain",
        None,
        Some(alpha.snapshot.clone()),
    )?;
    for (snapshot, present, absent, path) in [
        (&alpha.snapshot, "alpha", "beta", "alpha.md"),
        (&beta.snapshot, "beta", "alpha", "beta.md"),
        (&alpha.snapshot, "alpha", "beta", "alpha.md"),
    ] {
        let hits = vault.search(snapshot, present)?;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits.first().ok_or("Missing search result")?.path, path);
        assert!(vault.search(snapshot, absent)?.is_empty());
    }
    for query in ["%", "_"] {
        assert_eq!(vault.search(&alpha.snapshot, query)?.len(), 1);
        assert!(vault.search(&beta.snapshot, query)?.is_empty());
    }
    assert!(vault.search(&alpha.snapshot, "' OR 1=1 --")?.is_empty());
    fs::write(vault.root().join("local/index.sqlite"), b"damaged cache")?;
    assert_eq!(vault.search(&alpha.snapshot, "alpha")?.len(), 1);
    Ok(())
}
