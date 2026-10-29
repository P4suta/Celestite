use celestite_archive::{Entry, NoteId, Vault};
use std::{collections::BTreeMap, error::Error, fs, process::Command};

#[test]
fn archive_restores_without_the_application_or_index_and_portable_links_resolve()
-> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let input = area.path().join("input");
    fs::create_dir(&input)?;
    fs::write(
        input.join("a.md"),
        "# A\n\n[second](b.md#heading)\n![file][media]\n\n[media]: file.bin\n",
    )?;
    fs::write(input.join("b.md"), "# heading\n日本語 🦀\n")?;
    fs::write(input.join("file.bin"), [0_u8, 255, 128, 10])?;
    let vault = Vault::init(&area.path().join("vault"))?;
    let receipt = vault.import_directory(&input, None)?;
    assert_eq!(vault.search(&receipt.snapshot, "日本語")?.len(), 1);
    fs::remove_file(vault.root().join("local/index.sqlite"))?;
    assert_eq!(vault.search(&receipt.snapshot, "日本語")?.len(), 1);
    let bag = area.path().join("bag");
    vault.export_bag(&bag)?;
    Vault::verify_bag(&bag)?;
    let restored = Vault::restore_bag(&bag, &area.path().join("restored"))?;
    assert_eq!(restored.heads()?, vault.heads()?);
    assert!(!restored.root().join("local/index.sqlite").exists());
    let output = area.path().join("output");
    let report = restored.checkout(&receipt.snapshot, &output)?;
    assert_eq!(
        (report.notes, report.media, report.unresolved.len()),
        (2, 1, 0)
    );
    let mut notes = 0;
    for note in fs::read_dir(&output)? {
        let note = note?.path();
        if note.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        notes += 1;
        let source = fs::read_to_string(&note)?;
        for event in pulldown_cmark::Parser::new(&source) {
            if let pulldown_cmark::Event::Start(
                pulldown_cmark::Tag::Link { dest_url, .. }
                | pulldown_cmark::Tag::Image { dest_url, .. },
            ) = event
            {
                let path = dest_url.split('#').next().ok_or("Missing destination")?;
                assert!(
                    output.join(path).is_file(),
                    "Missing portable target: {path}"
                );
                assert!(
                    !path.starts_with("../"),
                    "Portable destinations stay inside the library: {path}"
                );
            }
        }
    }
    assert_eq!(notes, 2);
    for (id, entry) in vault.notes(&receipt.snapshot)? {
        let Entry::Live { record } = entry else {
            return Err("Unexpected conflict".into());
        };
        let note = vault.note(&record)?;
        let original = vault.read_bytes(&note.original, 4096)?;
        let restored_note = restored.note(&record)?;
        assert_eq!(note, restored_note);
        assert_eq!(
            restored.read_bytes(&restored_note.original, 4096)?,
            original
        );
        assert!(output.join(format!("{id}.md")).exists());
    }
    Ok(())
}

#[test]
fn bags_reject_payload_changes_missing_files_extra_files_and_traversal_before_restore()
-> Result<(), Box<dyn Error>> {
    for mutation in ["modify", "missing", "extra", "traversal", "tag"] {
        let area = tempfile::tempdir()?;
        let vault = Vault::init(&area.path().join("vault"))?;
        vault.capture(NoteId::new(), "note.md", b"hello", None, None)?;
        let bag = area.path().join("bag");
        vault.export_bag(&bag)?;
        match mutation {
            "modify" => fs::write(bag.join("data/FORMAT.md"), "changed")?,
            "missing" => fs::remove_file(bag.join("data/FORMAT.md"))?,
            "extra" => fs::write(bag.join("data/extra"), "extra")?,
            "traversal" => fs::write(
                bag.join("manifest-sha256.txt"),
                format!("{}  ../outside\n", "a".repeat(64)),
            )?,
            "tag" => fs::write(bag.join("bagit.txt"), "changed")?,
            _ => return Err("Unknown test mutation".into()),
        }
        let target = area.path().join("restore");
        assert!(Vault::restore_bag(&bag, &target).is_err());
        assert!(!target.exists());
    }
    Ok(())
}

#[test]
fn existing_checkout_data_is_never_overwritten_and_conflicts_require_selection()
-> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let vault = Vault::init(&area.path().join("vault"))?;
    let id = NoteId::new();
    let first = vault.capture(id, "note.md", b"first", None, None)?;
    vault.capture(id, "note.md", b"second", None, Some(first.snapshot.clone()))?;
    let conflict = vault.capture(id, "note.md", b"third", None, Some(first.snapshot))?;
    let target = area.path().join("output");
    assert!(vault.checkout(&conflict.snapshot, &target).is_err());
    assert!(!target.exists());
    let notes = vault.notes(&conflict.snapshot)?;
    let choice = notes
        .get(&id)
        .ok_or("Missing conflict")?
        .alternatives()
        .into_iter()
        .flatten()
        .next()
        .ok_or("No alternative")?;
    vault.checkout_selected(
        &conflict.snapshot,
        &target,
        &BTreeMap::from([(id, Some(choice))]),
    )?;
    fs::write(target.join("keep"), "original")?;
    assert!(
        vault
            .checkout_selected(&conflict.snapshot, &target, &BTreeMap::from([(id, None)]))
            .is_err()
    );
    assert_eq!(fs::read_to_string(target.join("keep"))?, "original");
    Ok(())
}

#[test]
fn independent_device_archives_converge_without_a_clock_based_winner() -> Result<(), Box<dyn Error>>
{
    let area = tempfile::tempdir()?;
    let a = Vault::init(&area.path().join("a"))?;
    let id = NoteId::new();
    let base = a.capture(id, "note.md", b"base", None, None)?;
    let first_bag = area.path().join("first-bag");
    a.export_bag(&first_bag)?;
    let b = Vault::restore_bag(&first_bag, &area.path().join("b"))?;
    a.capture(id, "note.md", b"left", None, Some(base.snapshot.clone()))?;
    b.capture(id, "note.md", b"right", None, Some(base.snapshot))?;
    let a_bag = area.path().join("a-bag");
    let b_bag = area.path().join("b-bag");
    a.export_bag(&a_bag)?;
    b.export_bag(&b_bag)?;
    let a_results = a.ingest_bag(&b_bag)?;
    let b_results = b.ingest_bag(&a_bag)?;
    let a_head = &a_results.last().ok_or("Missing merge")?.snapshot;
    let b_head = &b_results.last().ok_or("Missing merge")?.snapshot;
    assert_eq!(a.notes(a_head)?, b.notes(b_head)?);
    assert!(matches!(
        a.notes(a_head)?.get(&id),
        Some(Entry::Conflict { .. })
    ));
    let merged_bag = area.path().join("merged-bag");
    a.export_bag(&merged_bag)?;
    let restored = Vault::restore_bag(&merged_bag, &area.path().join("restored-merge"))?;
    assert_eq!(restored.current()?, Some(a_head.clone()));
    Ok(())
}

#[test]
fn standalone_cli_captures_verifies_exports_and_restores_real_files() -> Result<(), Box<dyn Error>>
{
    let area = tempfile::tempdir()?;
    let input = area.path().join("input");
    fs::create_dir(&input)?;
    fs::write(input.join("note.md"), "# CLI\n日本語\n")?;
    let executable = env!("CARGO_BIN_EXE_celestite-archive");
    let vault = area.path().join("vault");
    let bag = area.path().join("bag");
    let restored = area.path().join("restored");
    let output = area.path().join("output");
    for arguments in [
        vec!["init".as_ref(), vault.as_os_str()],
        vec!["import".as_ref(), vault.as_os_str(), input.as_os_str()],
        vec!["verify".as_ref(), vault.as_os_str()],
        vec!["export".as_ref(), vault.as_os_str(), bag.as_os_str()],
        vec!["restore".as_ref(), bag.as_os_str(), restored.as_os_str()],
        vec![
            "checkout".as_ref(),
            restored.as_os_str(),
            output.as_os_str(),
        ],
    ] {
        let result = Command::new(executable).args(arguments).output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert_eq!(
        fs::read_dir(&output)?
            .filter(|entry| {
                entry.as_ref().is_ok_and(|entry| {
                    entry.path().extension().and_then(|value| value.to_str()) == Some("md")
                })
            })
            .count(),
        1
    );
    Ok(())
}
