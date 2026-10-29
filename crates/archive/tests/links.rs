use celestite_archive::{Entry, LinkTarget, NoteId, Vault};
use std::{error::Error, fs};

#[test]
fn renamed_targets_keep_identity_and_path_reuse_stays_ambiguous_on_repeated_import()
-> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let input = area.path().join("input");
    fs::create_dir(&input)?;
    let a = NoteId::new();
    let b = NoteId::new();
    let other = NoteId::new();
    fs::write(
        input.join("a.md"),
        format!("---\nid: {a}\n---\n[other](b.md)\n"),
    )?;
    fs::write(input.join("b.md"), format!("---\nid: {b}\n---\nB\n"))?;
    let vault = Vault::init(&area.path().join("vault"))?;
    let first = vault.import_directory(&input, None)?;
    fs::rename(input.join("b.md"), input.join("c.md"))?;
    let renamed = vault.import_directory(&input, Some(first.snapshot))?;
    let a_record = vault
        .notes(&renamed.snapshot)?
        .get(&a)
        .cloned()
        .ok_or("Missing note")?;
    let Entry::Live { record } = a_record else {
        return Err("Unexpected note conflict".into());
    };
    assert_eq!(
        vault.note(&record)?.links[0].target,
        LinkTarget::Note { id: b }
    );
    fs::write(
        input.join("b.md"),
        format!("---\nid: {other}\n---\nNew unrelated note\n"),
    )?;
    let mut base = renamed.snapshot;
    for _ in 0..2 {
        let receipt = vault.import_directory(&input, Some(base))?;
        let notes = vault.notes(&receipt.snapshot)?;
        let Some(Entry::Live { record }) = notes.get(&a) else {
            return Err("Missing link source".into());
        };
        assert_eq!(
            vault.note(record)?.links[0].target,
            LinkTarget::Ambiguous {
                previous: b,
                current: Some(other)
            }
        );
        base = receipt.snapshot;
    }
    let output = area.path().join("output");
    let report = vault.checkout(&base, &output)?;
    assert_eq!(report.unresolved.len(), 1);
    assert!(fs::read_to_string(output.join(format!("{a}.md")))?.contains("(b.md)"));
    Ok(())
}

#[test]
fn metadata_code_and_html_are_not_rewritten_as_markdown_destinations() -> Result<(), Box<dyn Error>>
{
    let area = tempfile::tempdir()?;
    let vault = Vault::init(area.path())?;
    let id = NoteId::new();
    let source = format!(
        "---\nid: {id}\nproperty: '[metadata](meta.md)'\n---\n`[code](code.md)`\n\n```md\n[code](code.md)\n```\n\n<a href=\"raw.md\">HTML</a>\n\n[real](real.md)\n"
    );
    let object = vault.prepare_note(
        id,
        "source.md",
        source.as_bytes(),
        None,
        &Default::default(),
        None,
    )?;
    let note = vault.note(&object)?;
    assert_eq!(note.links.len(), 1);
    assert_eq!(note.links[0].destination, "real.md");
    Ok(())
}

#[test]
fn uuid_filenames_bind_by_identity_before_capture_and_survive_retitling()
-> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let library = area.path().join("library");
    fs::create_dir(&library)?;
    let vault = Vault::init(&area.path().join("vault"))?;
    let referrer = NoteId::new();
    let target = NoteId::new();
    let source = format!(
        "---\nid: {referrer}\n---\n# Java\n\nSee [Object].\n\n[Object]: {target}.md#heading\n[Unused]: {target}.md\n"
    );
    let referrer_file = library.join(format!("{referrer}.md"));
    fs::write(&referrer_file, &source)?;
    let first = vault.capture_file(
        referrer,
        Some(&referrer_file),
        source.as_bytes(),
        None,
        Some(&library),
    )?;
    let notes = vault.notes(&first.snapshot)?;
    let Some(Entry::Live { record }) = notes.get(&referrer) else {
        return Err("Missing referrer".into());
    };
    let before = vault.note(record)?;
    assert_eq!(before.path, format!("{referrer}.md"));
    assert_eq!(before.links.len(), 2);
    for link in &before.links {
        assert_eq!(link.target, LinkTarget::Note { id: target });
        assert_eq!(
            source.get(link.start..link.end),
            Some(link.destination.as_str())
        );
    }
    let target_file = library.join(format!("{target}.md"));
    for title in ["Object", "Object (philosophy)"] {
        let text = format!("---\nid: {target}\n---\n# {title}\n");
        fs::write(&target_file, &text)?;
        vault.capture_file(
            target,
            Some(&target_file),
            text.as_bytes(),
            vault.current()?,
            Some(&library),
        )?;
    }
    let head = vault.current()?.ok_or("Missing head")?;
    let notes = vault.notes(&head)?;
    let Some(Entry::Live { record: unchanged }) = notes.get(&referrer) else {
        return Err("Missing referrer".into());
    };
    assert_eq!(unchanged, record);
    let output = area.path().join("output");
    let report = vault.checkout(&head, &output)?;
    assert!(report.unresolved.is_empty());
    assert_eq!(
        fs::read_to_string(output.join(format!("{referrer}.md")))?,
        source
    );
    Ok(())
}

#[test]
fn library_assets_bind_by_digest_and_mismatched_names_stay_unresolved() -> Result<(), Box<dyn Error>>
{
    let area = tempfile::tempdir()?;
    let library = area.path().join("library");
    fs::create_dir_all(library.join("assets"))?;
    let vault = Vault::init(&area.path().join("vault"))?;
    let id = NoteId::new();
    let bytes = [0_u8, 255, 128, 10];
    let digest = celestite_archive::ObjectId::of(&bytes);
    let named = format!("assets/{}.png", digest.digest());
    let wrong = format!("assets/{}.png", "0".repeat(64));
    fs::write(library.join(&named), bytes)?;
    fs::write(library.join(&wrong), bytes)?;
    fs::write(library.join("assets/photo.png"), bytes)?;
    let source = format!(
        "---\nid: {id}\n---\n![a]\n![b]\n![c]\n\n[a]: {named}\n[b]: {wrong}\n[c]: assets/photo.png\n"
    );
    let file = library.join(format!("{id}.md"));
    fs::write(&file, &source)?;
    let receipt = vault.capture_file(id, Some(&file), source.as_bytes(), None, Some(&library))?;
    let notes = vault.notes(&receipt.snapshot)?;
    let Some(Entry::Live { record }) = notes.get(&id) else {
        return Err("Missing note".into());
    };
    let note = vault.note(record)?;
    assert_eq!(
        note.links
            .iter()
            .map(|link| &link.target)
            .collect::<Vec<_>>(),
        vec![
            &LinkTarget::File {
                path: named.clone(),
                object: digest.clone()
            },
            &LinkTarget::Unresolved {
                reason: String::from("The asset name does not match its content")
            },
            &LinkTarget::File {
                path: String::from("assets/photo.png"),
                object: digest.clone()
            },
        ]
    );
    let output = area.path().join("output");
    let report = vault.checkout(&receipt.snapshot, &output)?;
    assert_eq!((report.media, report.unresolved.len()), (1, 1));
    let portable = fs::read_to_string(output.join(format!("{id}.md")))?;
    assert!(portable.contains(&format!("[a]: {named}\n")));
    assert!(portable.contains(&format!("[c]: {named}\n")));
    assert!(output.join(&named).is_file());
    Ok(())
}
