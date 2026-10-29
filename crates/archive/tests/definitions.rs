use celestite_archive::{
    LinkDestination, LinkEditError, LinkPatch, NoteId, ObjectId, plan_link_patch,
};
use std::error::Error;

fn apply(source: &str, patches: &[LinkPatch]) -> String {
    let mut text = source.to_owned();
    for patch in patches.iter().rev() {
        text.replace_range(patch.start..patch.end, &patch.insert);
    }
    text
}

fn note() -> (NoteId, LinkDestination) {
    let id: NoteId = "019a3f25-9b0e-7d12-a3c7-5e1f8a2b6d90"
        .parse()
        .unwrap_or_default();
    (id, LinkDestination::Note(id))
}

#[test]
fn a_first_link_gets_a_definition_after_a_blank_line() -> Result<(), Box<dyn Error>> {
    let (id, destination) = note();
    for (source, expected) in [
        (
            "# Note\n\nSee [Obj",
            format!("# Note\n\nSee [Obj]\n\n[Obj]: {id}.md\n"),
        ),
        (
            "# Note\n\nSee [Obj\n",
            format!("# Note\n\nSee [Obj]\n\n[Obj]: {id}.md\n"),
        ),
        (
            "# Note\n\nSee [Obj\n\n",
            format!("# Note\n\nSee [Obj]\n\n[Obj]: {id}.md\n"),
        ),
        (
            "---\nid: 019a3f1e-7c2d-7b8a-9e21-0f4b6c8d1a2e\n---\n[Obj",
            format!(
                "---\nid: 019a3f1e-7c2d-7b8a-9e21-0f4b6c8d1a2e\n---\n[Obj]\n\n[Obj]: {id}.md\n"
            ),
        ),
        (
            "See [Obj\r\n",
            format!("See [Obj]\r\n\r\n[Obj]: {id}.md\r\n"),
        ),
        ("See [Obj\n   ", format!("See [Obj]\n   \n[Obj]: {id}.md\n")),
    ] {
        let from = source.find("[Obj").ok_or("Missing words")?;
        let to = from + "[Obj".len();
        let (patches, caret) = plan_link_patch(source, from, to, "Obj", &destination, false)?;
        assert_eq!(apply(source, &patches), expected, "{source:?}");
        assert_eq!(caret, from + "[Obj]".len());
    }
    Ok(())
}

#[test]
fn a_second_definition_joins_the_trailing_block_without_a_blank_line() -> Result<(), Box<dyn Error>>
{
    let (id, destination) = note();
    let other: NoteId = "019a3f21-13aa-7c01-8c55-2b9e7d0f3c44".parse()?;
    let source = format!("[A] and [B\n\n[A]: {other}.md\n");
    let from = source.find("[B").ok_or("Missing words")?;
    let (patches, _) = plan_link_patch(&source, from, from + 2, "B", &destination, false)?;
    assert_eq!(
        apply(&source, &patches),
        format!("[A] and [B]\n\n[A]: {other}.md\n[B]: {id}.md\n")
    );
    let source = format!("[A] and [B\n\n[A]: {other}.md");
    let from = source.find("[B").ok_or("Missing words")?;
    let (patches, _) = plan_link_patch(&source, from, from + 2, "B", &destination, false)?;
    assert_eq!(
        apply(&source, &patches),
        format!("[A] and [B]\n\n[A]: {other}.md\n[B]: {id}.md\n")
    );
    Ok(())
}

#[test]
fn an_existing_label_is_kept_for_the_same_target_and_refused_for_another()
-> Result<(), Box<dyn Error>> {
    let (id, destination) = note();
    let source = format!("[Object] first.\nThen [object\n\n[Object]: {id}.md\n");
    let from = source.find("[object").ok_or("Missing words")?;
    let (patches, _) = plan_link_patch(&source, from, from + 7, "object", &destination, false)?;
    assert_eq!(patches.len(), 1);
    assert_eq!(
        apply(&source, &patches),
        format!("[Object] first.\nThen [object]\n\n[Object]: {id}.md\n")
    );
    let other: NoteId = "019a3f21-13aa-7c01-8c55-2b9e7d0f3c44".parse()?;
    let result = plan_link_patch(
        &source,
        from,
        from + 7,
        "object",
        &LinkDestination::Note(other),
        false,
    );
    assert!(matches!(result, Err(LinkEditError::Duplicate { .. })));
    let source = format!("[Meeting   Notes\n\n[meeting notes]: {other}.md\n");
    let result = plan_link_patch(&source, 0, 16, "Meeting   Notes", &destination, false);
    assert!(matches!(result, Err(LinkEditError::Duplicate { .. })));
    Ok(())
}

#[test]
fn images_and_assets_use_the_same_definition_block() -> Result<(), Box<dyn Error>> {
    let digest = ObjectId::of(b"pixels");
    let destination = LinkDestination::Asset {
        digest: digest.clone(),
        extension: String::from("png"),
    };
    let source = "Figure: ![diagram";
    let from = source.find("![").ok_or("Missing words")?;
    let (patches, caret) =
        plan_link_patch(source, from, source.len(), "diagram", &destination, true)?;
    assert_eq!(
        apply(source, &patches),
        format!(
            "Figure: ![diagram]\n\n[diagram]: assets/{}.png\n",
            digest.digest()
        )
    );
    assert_eq!(caret, source.len() + 1);
    Ok(())
}

#[test]
fn labels_are_escaped_japanese_and_bounded() -> Result<(), Box<dyn Error>> {
    let (id, destination) = note();
    let source = "x [a]b";
    let (patches, _) = plan_link_patch(source, 2, 6, "[a]b", &destination, false)?;
    assert_eq!(
        apply(source, &patches),
        format!("x [\\[a\\]b]\n\n[\\[a\\]b]: {id}.md\n")
    );
    let source = "見出し [日本語 メモ";
    let from = source.find('[').ok_or("Missing words")?;
    let (patches, _) = plan_link_patch(
        source,
        from,
        source.len(),
        "日本語 メモ",
        &destination,
        false,
    )?;
    assert_eq!(
        apply(source, &patches),
        format!("見出し [日本語 メモ]\n\n[日本語 メモ]: {id}.md\n")
    );
    for words in ["", "  ", "a\nb", "^foot"] {
        assert!(matches!(
            plan_link_patch("[x", 0, 2, words, &destination, false),
            Err(LinkEditError::Label(_))
        ));
    }
    let long = "字".repeat(1000);
    assert!(matches!(
        plan_link_patch("[x", 0, 2, &long, &destination, false),
        Err(LinkEditError::Label(_))
    ));
    Ok(())
}

#[test]
fn links_are_refused_in_code_front_matter_and_invalid_positions() -> Result<(), Box<dyn Error>> {
    let (_, destination) = note();
    let fenced = "```\n[x\n```\n";
    let from = fenced.find("[x").ok_or("Missing words")?;
    assert!(matches!(
        plan_link_patch(fenced, from, from + 2, "x", &destination, false),
        Err(LinkEditError::Position)
    ));
    let span = "`[x` tail";
    assert!(matches!(
        plan_link_patch(span, 1, 3, "x", &destination, false),
        Err(LinkEditError::Position)
    ));
    let front = "---\nid: 019a3f1e-7c2d-7b8a-9e21-0f4b6c8d1a2e\n---\nbody";
    assert!(matches!(
        plan_link_patch(front, 4, 6, "id", &destination, false),
        Err(LinkEditError::Position)
    ));
    let open = "text\n```\n[x";
    let from = open.find("[x").ok_or("Missing words")?;
    assert!(matches!(
        plan_link_patch(open, from, from + 2, "x", &destination, false),
        Err(LinkEditError::Position)
    ));
    assert!(matches!(
        plan_link_patch("ab", 2, 1, "x", &destination, false),
        Err(LinkEditError::Position)
    ));
    assert!(matches!(
        plan_link_patch("日", 1, 1, "x", &destination, false),
        Err(LinkEditError::Position)
    ));
    assert!(matches!(
        plan_link_patch("ab", 0, 3, "x", &destination, false),
        Err(LinkEditError::Position)
    ));
    Ok(())
}

#[test]
fn a_link_that_would_not_parse_leaves_the_note_unchanged() -> Result<(), Box<dyn Error>> {
    let (_, destination) = note();
    for source in ["[x(after)", "[x[other]", "[x[]"] {
        assert!(
            matches!(
                plan_link_patch(source, 0, 2, "x", &destination, false),
                Err(LinkEditError::Unparsed)
            ),
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn the_editing_limit_counts_the_appended_definition() -> Result<(), Box<dyn Error>> {
    let (id, destination) = note();
    let definition = format!("\n\n[x]: {id}.md\n");
    let mut source = "a".repeat(celestite_core::MAX_FILE_BYTES - definition.len() - 3);
    source.push_str("[x");
    let (patches, _) = plan_link_patch(
        &source,
        source.len() - 2,
        source.len(),
        "x",
        &destination,
        false,
    )?;
    assert_eq!(
        apply(&source, &patches).len(),
        celestite_core::MAX_FILE_BYTES
    );
    source.push('a');
    let from = source.len() - 3;
    assert!(matches!(
        plan_link_patch(&source, from, from + 2, "x", &destination, false),
        Err(LinkEditError::TooLarge)
    ));
    Ok(())
}
