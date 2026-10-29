use celestite_archive::{NoteId, asset_digest, note_filename_identity, note_title};
use std::error::Error;

#[test]
fn the_title_is_the_first_level_one_heading_in_the_body() {
    for (source, expected) in [
        ("# Object\n\ntext", Some("Object")),
        ("intro\n\n## Minor\n\n# Major\n", Some("Major")),
        ("Setext\n======\n", Some("Setext")),
        (
            "# `code` and *emphasis*  \ttabs \n",
            Some("code and emphasis tabs"),
        ),
        (
            "---\nid: x\n---\n# After metadata\n",
            Some("After metadata"),
        ),
        ("\u{feff}# With BOM\r\n", Some("With BOM")),
        ("---\n# Not a title\n---\nbody\n", None),
        ("```\n# fenced\n```\n", None),
        ("## Only second level\n", None),
        ("#\n", None),
        ("", None),
        ("# 日本語の　見出し\n", Some("日本語の　見出し")),
        ("# \u{b}Java\u{b}\n", Some("Java")),
        ("# \u{3000}\n", None),
    ] {
        assert_eq!(note_title(source).as_deref(), expected, "{source:?}");
    }
}

#[test]
fn only_lowercase_uuid_markdown_names_carry_identity() -> Result<(), Box<dyn Error>> {
    let id: NoteId = "019a3f25-9b0e-7d12-a3c7-5e1f8a2b6d90".parse()?;
    for destination in [
        format!("{id}.md"),
        format!("./{id}.md"),
        format!("{id}.MD"),
        format!("{id}.md#heading"),
        format!("{id}.md?q"),
        format!("{}%2D{}.md", &id.to_string()[..8], &id.to_string()[9..]),
    ] {
        assert_eq!(
            note_filename_identity(&destination),
            Some(id),
            "{destination}"
        );
    }
    for destination in [
        format!("{}.md", id.to_string().to_uppercase()),
        format!("notes/{id}.md"),
        format!("../{id}.md"),
        format!("{id}.txt"),
        format!("{id}"),
        String::from("00000000-0000-0000-0000-000000000000.md"),
        String::from("other.md"),
        String::from("https://example.com/x.md"),
    ] {
        assert_eq!(note_filename_identity(&destination), None, "{destination}");
    }
    Ok(())
}

#[test]
fn asset_paths_name_their_digest_only_in_the_exact_shape() {
    let digest = "a".repeat(64);
    assert_eq!(
        asset_digest(&format!("assets/{digest}.png")).map(str::to_owned),
        Some(digest.clone())
    );
    for path in [
        format!("assets/{digest}"),
        format!("assets/{}.png", &digest[..63]),
        format!("assets/{}.png", digest.to_uppercase()),
        format!("assets/{digest}.p ng"),
        format!("assets/sub/{digest}.png"),
        format!("other/{digest}.png"),
        format!("{digest}.png"),
    ] {
        assert_eq!(asset_digest(&path), None, "{path}");
    }
}
