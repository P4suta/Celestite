use std::error::Error;

use celestite_archive::Vault;
use celestite_archive::{ArchiveError, Entry, LinkTarget, NoteId, ObjectId, PublicationStage};
use std::{collections::BTreeMap, fs};

#[test]
fn a_new_vault_has_a_self_describing_format_and_no_committed_notes() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let _vault = Vault::init(directory.path())?;
    let format: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("format.json"))?)?;
    assert_eq!(format["format"], "celestite.archive");
    assert_eq!(format["version"], 1);
    assert!(directory.path().join("FORMAT.md").is_file());
    Ok(())
}

fn live(vault: &Vault, id: NoteId, text: &str) -> Result<Entry, Box<dyn Error>> {
    Ok(Entry::Live {
        record: vault.prepare_note(
            id,
            &format!("{id}.md"),
            text.as_bytes(),
            None,
            &BTreeMap::new(),
            None,
        )?,
    })
}

#[test]
fn imported_unicode_originals_media_and_reference_links_remain_source_bound()
-> Result<(), Box<dyn Error>> {
    let archive = tempfile::tempdir()?;
    let input = tempfile::tempdir()?;
    let vault = Vault::init(archive.path())?;
    let original = "\u{feff}# 日本語 e\u{301}\r\n\r\n[other](other.md)\r\n![image][figure]\r\n\r\n[figure]: image.bin\r\n";
    let media = [0_u8, 255, 13, 10, 128];
    fs::write(input.path().join("first.md"), original)?;
    fs::write(input.path().join("other.md"), "# Second\n")?;
    fs::write(input.path().join("image.bin"), media)?;
    let receipt = vault.import_directory(input.path(), None)?;
    let notes = vault.notes(&receipt.snapshot)?;
    assert_eq!(notes.len(), 2);
    let mut found = false;
    for entry in notes.values() {
        let Entry::Live { record } = entry else {
            return Err("Unexpected conflict".into());
        };
        let note = vault.note(record)?;
        if note.path != "first.md" {
            continue;
        }
        found = true;
        assert_eq!(vault.read_bytes(&note.original, 4096)?, original.as_bytes());
        let source = vault.read_bytes(&note.source, 4096)?;
        let source = std::str::from_utf8(&source)?;
        assert_eq!(celestite_archive::identity(source)?, Some(note.id));
        assert!(source.contains("e\u{301}\r\n"));
        assert_eq!(note.links.len(), 2);
        for link in &note.links {
            assert_eq!(
                source.get(link.start..link.end),
                Some(link.destination.as_str())
            );
        }
        assert!(matches!(note.links[0].target, LinkTarget::Note { .. }));
        let LinkTarget::File { object, .. } = &note.links[1].target else {
            return Err("Media was not captured".into());
        };
        assert_eq!(vault.read_bytes(object, media.len())?, media);
    }
    assert!(found);
    assert_eq!(
        fs::read(input.path().join("first.md"))?,
        original.as_bytes()
    );
    assert_eq!(vault.verify(&receipt.snapshot)?.unresolved_links, 0);
    Ok(())
}

#[test]
fn stale_edits_become_conflicts_and_deletion_does_not_win_over_concurrent_text()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let vault = Vault::init(directory.path())?;
    let id = NoteId::new();
    let first = vault.commit_notes(None, BTreeMap::from([(id, live(&vault, id, "base")?)]))?;
    let left = live(&vault, id, "left 🦀")?;
    let right = live(&vault, id, "right 日本語")?;
    vault.commit_notes(
        Some(first.snapshot.clone()),
        BTreeMap::from([(id, left.clone())]),
    )?;
    let conflict = vault.commit_notes(
        Some(first.snapshot.clone()),
        BTreeMap::from([(id, right.clone())]),
    )?;
    assert_eq!(conflict.conflicts, vec![id]);
    let merged = vault.notes(&conflict.snapshot)?;
    let alternatives = merged.get(&id).ok_or("Missing note")?.alternatives();
    assert!(
        left.alternatives()
            .iter()
            .all(|value| alternatives.contains(value))
    );
    assert!(
        right
            .alternatives()
            .iter()
            .all(|value| alternatives.contains(value))
    );
    let deletion = vault.commit_notes(
        Some(first.snapshot.clone()),
        BTreeMap::from([(id, Entry::Deleted)]),
    )?;
    let alternatives = vault
        .notes(&deletion.snapshot)?
        .get(&id)
        .ok_or("Missing deletion conflict")?
        .alternatives();
    assert!(alternatives.contains(&None));
    assert!(
        left.alternatives()
            .iter()
            .all(|value| alternatives.contains(value))
    );
    vault.verify(&first.snapshot)?;
    vault.verify(&deletion.snapshot)?;
    Ok(())
}

#[test]
fn independent_notes_merge_without_dropping_either_branch() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let vault = Vault::init(directory.path())?;
    let a = NoteId::new();
    let b = NoteId::new();
    let base = vault.commit_notes(None, BTreeMap::new())?;
    vault.commit_notes(
        Some(base.snapshot.clone()),
        BTreeMap::from([(a, live(&vault, a, "A")?)]),
    )?;
    let result = vault.commit_notes(
        Some(base.snapshot),
        BTreeMap::from([(b, live(&vault, b, "B")?)]),
    )?;
    assert!(result.conflicts.is_empty());
    assert_eq!(vault.notes(&result.snapshot)?.len(), 2);
    Ok(())
}

#[test]
fn every_publication_failure_preserves_the_previously_acknowledged_snapshot()
-> Result<(), Box<dyn Error>> {
    for stage in [
        PublicationStage::ObjectsSynced,
        PublicationStage::SnapshotSynced,
        PublicationStage::ReferenceReplaced,
        PublicationStage::ReferenceSynced,
    ] {
        let directory = tempfile::tempdir()?;
        let vault = Vault::init(directory.path())?;
        let id = NoteId::new();
        let first = vault.commit_notes(
            None,
            BTreeMap::from([(id, live(&vault, id, "acknowledged")?)]),
        )?;
        let change = live(&vault, id, "pending")?;
        let result = vault.commit_with_fault(
            Some(first.snapshot.clone()),
            BTreeMap::from([(id, change)]),
            &mut |observed| {
                if observed == stage {
                    Err(ArchiveError::Invalid(String::from("Injected interruption")))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        vault.verify(&first.snapshot)?;
        let reopened = Vault::open(directory.path())?;
        reopened.verify(&reopened.current()?.ok_or("Lost current reference")?)?;
        if matches!(
            stage,
            PublicationStage::ObjectsSynced | PublicationStage::SnapshotSynced
        ) {
            assert_eq!(reopened.current()?, Some(first.snapshot));
        }
    }
    Ok(())
}

#[test]
fn malformed_ids_missing_objects_and_unknown_generations_never_admit_data()
-> Result<(), Box<dyn Error>> {
    for value in ["", "sha256:../x", "sha256:ABC", "md5:0123"] {
        assert!(value.parse::<ObjectId>().is_err());
    }
    assert!(
        "00000000-0000-0000-0000-000000000000"
            .parse::<NoteId>()
            .is_err()
    );
    let directory = tempfile::tempdir()?;
    let vault = Vault::init(directory.path())?;
    let id = NoteId::new();
    let entry = live(&vault, id, "retain")?;
    let Entry::Live { record } = &entry else {
        return Err("Missing note".into());
    };
    let note = vault.note(record)?;
    let receipt = vault.commit_notes(None, BTreeMap::from([(id, entry)]))?;
    fs::write(vault.object_path(&note.source), b"corrupt")?;
    assert!(vault.verify(&receipt.snapshot).is_err());
    fs::write(
        directory.path().join("format.json"),
        br#"{"format":"celestite.archive","version":99}"#,
    )?;
    assert!(matches!(
        Vault::open(directory.path()),
        Err(ArchiveError::Unsupported(99))
    ));
    assert_eq!(fs::read(vault.object_path(&note.source))?, b"corrupt");
    Ok(())
}

#[test]
fn invalid_structural_types_and_duplicate_conflict_alternatives_are_refused()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let vault = Vault::init(directory.path())?;
    let id = NoteId::new();
    let entry = live(&vault, id, "body")?;
    if let Entry::Live { record } = &entry {
        assert!(
            vault.verify(record).is_err(),
            "A note record cannot stand in for a snapshot root"
        );
    }
    let alternatives = entry.alternatives();
    let invalid = Entry::Conflict {
        alternatives: vec![alternatives[0].clone(), alternatives[0].clone()],
    };
    assert!(
        vault
            .commit_notes(None, BTreeMap::from([(id, invalid)]))
            .is_err()
    );
    assert_eq!(vault.current()?, None);
    let source = ObjectId::of(b"raw");
    assert!(vault.record(&source).is_err());
    assert!(matches!(entry, Entry::Live { .. }));
    Ok(())
}

#[test]
fn overlapping_source_bindings_are_refused_before_publication() -> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let vault = Vault::init(area.path())?;
    let id = NoteId::new();
    let Entry::Live { record } = live(&vault, id, "[link](other.md)")? else {
        return Err("Missing note".into());
    };
    let mut note = vault.note(&record)?;
    note.links
        .push(note.links.first().ok_or("Missing link")?.clone());
    let bytes = serde_json::to_vec(&celestite_archive::Record::Note { note })?;
    let object = ObjectId::of(&bytes);
    let path = vault.object_path(&object);
    fs::create_dir_all(path.parent().ok_or("Missing object parent")?)?;
    fs::write(path, bytes)?;
    assert!(
        vault
            .commit_notes(None, BTreeMap::from([(id, Entry::Live { record: object })]))
            .is_err()
    );
    assert_eq!(vault.current()?, None);
    Ok(())
}

#[test]
fn unchanged_capture_reports_conflicts_in_other_notes() -> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let vault = Vault::init(area.path())?;
    let a = NoteId::new();
    let b = NoteId::new();
    let initial = vault.capture(a, "a.md", b"base", None, None)?;
    vault.capture(a, "a.md", b"left", None, Some(initial.snapshot.clone()))?;
    let conflict = vault.capture(a, "a.md", b"right", None, Some(initial.snapshot))?;
    let added = vault.capture(b, "b.md", b"unchanged", None, Some(conflict.snapshot))?;
    let same = vault.capture(b, "b.md", b"unchanged", None, Some(added.snapshot.clone()))?;
    assert_eq!(same.snapshot, added.snapshot);
    assert_eq!(same.conflicts, vec![a]);
    Ok(())
}
