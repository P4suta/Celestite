use celestite_archive::{heading_offset, link_at};
use std::error::Error;

#[test]
fn commonmark_links_use_source_positions_and_resolve_reference_definitions()
-> Result<(), Box<dyn Error>> {
    for (source, needle, destination) in [
        ("[label](target.md)", "label", "target.md"),
        ("[label](<a b.md> \"title\")", "label", "a b.md"),
        ("[label](a\\(b\\).md)", "label", "a(b).md"),
        ("[label][ref]\n\n[ref]: target.md", "label", "target.md"),
        ("[label][]\n\n[label]: target.md", "label", "target.md"),
        ("[label]\n\n[label]: target.md", "label", "target.md"),
        ("![label](image.png)", "label", "image.png"),
        ("<https://example.com>", "example", "https://example.com"),
        ("🦀 [日本語](%E6%97%A5.md)", "日本語", "%E6%97%A5.md"),
        (
            "\u{feff}---\r\nid: ignored\r\n---\r\n[label](target.md)",
            "label",
            "target.md",
        ),
    ] {
        let position = source.find(needle).ok_or("Missing fixture marker")?;
        assert_eq!(
            link_at(source, position).as_deref(),
            Some(destination),
            "{source}"
        );
    }
    assert_eq!(
        link_at("[a](one)[b](two)", "[a](one)".len()).as_deref(),
        Some("two")
    );
    assert_eq!(
        link_at("[a](one)", "[a](one)".len()).as_deref(),
        Some("one")
    );
    Ok(())
}

#[test]
fn source_code_html_metadata_and_invalid_offsets_are_not_links() -> Result<(), Box<dyn Error>> {
    for source in [
        "`[label](target.md)`",
        "```\n[label](target.md)\n```",
        "    [label](target.md)",
        "<a href='target.md'>label</a>",
        "---\nname: '[label](target.md)'\n---",
        "[label][undefined]",
    ] {
        assert!(
            link_at(
                source,
                source.find("label").ok_or("Missing fixture marker")?
            )
            .is_none(),
            "{source}"
        );
    }
    assert!(link_at("[日](target.md)", 2).is_none());
    assert!(link_at("[a](one)", usize::MAX).is_none());
    Ok(())
}

#[test]
fn headings_have_stable_unicode_slugs_and_disambiguate_all_previous_ids() {
    let source = "---\nname: hidden\n---\n🦀\n# Hello, *World*!\n# 日本語\n# Hello, World!\n# hello-world-1\nSetext `code`\n===\n";
    for (fragment, marker) in [
        ("hello-world", "# Hello, *World*!"),
        ("日本語", "# 日本語"),
        ("hello-world-1", "# Hello, World!"),
        ("hello-world-1-1", "# hello-world-1"),
        ("setext-code", "Setext `code`"),
        ("Hello, World!", "# Hello, *World*!"),
    ] {
        assert_eq!(
            heading_offset(source, fragment),
            source.find(marker),
            "{fragment}"
        );
    }
    assert_eq!(heading_offset(source, ""), Some(0));
    assert_eq!(heading_offset(source, "missing"), None);
    assert_eq!(heading_offset("```\n# Hidden\n```", "hidden"), None);
}
