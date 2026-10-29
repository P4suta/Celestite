use celestite_documents::{Document, DocumentError, LinkNavigation};
use std::{error::Error, fs};
use tempfile::tempdir;

fn follow(source: &Document, text: &str, marker: &str) -> Result<LinkNavigation, DocumentError> {
    let snapshot = source.snapshot();
    let offset = text.find(marker).ok_or(DocumentError::InvalidPath)?;
    let position = text[..offset].encode_utf16().count();
    source.follow_link(&snapshot.token, snapshot.revision, text, position)
}

#[test]
fn a_relative_note_opens_with_its_heading_and_leaves_the_source_unchanged()
-> Result<(), Box<dyn Error>> {
    let root = tempdir()?;
    let source_path = root.path().join("source.md");
    let target_path = root.path().join("日本語 note.md");
    let text = "🦀 [continue](日本語%20note.md#second-heading)\n";
    fs::write(&source_path, text)?;
    fs::write(
        &target_path,
        "# First\r\n\r\n## Second *heading*\r\nbody\r\n",
    )?;
    let source = Document::open(&source_path)?;
    let before = source.snapshot();
    let LinkNavigation::Note {
        document,
        selection,
    } = follow(&source, text, "continue")?
    else {
        return Err("Expected a note window".into());
    };
    assert_eq!(
        document.path(),
        Some(fs::canonicalize(target_path)?.as_path())
    );
    assert_eq!(selection, Some("# First\n\n".encode_utf16().count()));
    assert_eq!(source.snapshot().text, before.text);
    assert_eq!(source.snapshot().revision, before.revision);
    assert_eq!(fs::read_to_string(source_path)?, text);
    Ok(())
}

#[test]
fn reference_links_and_duplicate_unicode_headings_resolve_without_rendering()
-> Result<(), Box<dyn Error>> {
    let text = "[jump][ref]\n\n[ref]: #日本語-1\n\n# 日本語\n# 日本語\n";
    let mut source = Document::new();
    let token = source.snapshot().token;
    source.edit(&token, 1, text.into())?;
    let LinkNavigation::Here { selection } = follow(&source, text, "jump")? else {
        return Err("Expected an in-note selection".into());
    };
    assert_eq!(
        selection,
        text[..text.rfind("# 日本語").ok_or("Missing heading")?]
            .encode_utf16()
            .count()
    );
    Ok(())
}

#[test]
fn current_file_links_use_the_current_buffer_including_unsaved_headings()
-> Result<(), Box<dyn Error>> {
    let root = tempdir()?;
    let path = root.path().join("source.md");
    fs::write(&path, "disk copy")?;
    let mut source = Document::open(&path)?;
    let text = "[jump](./source.md#edited)\n# Edited\n";
    source.edit(&source.snapshot().token, 1, text.into())?;
    assert!(matches!(
        follow(&source, text, "jump")?,
        LinkNavigation::Here { .. }
    ));
    assert_eq!(fs::read_to_string(path)?, "disk copy");
    Ok(())
}

#[test]
fn web_links_allow_only_browser_protocols() -> Result<(), Box<dyn Error>> {
    for (destination, allowed) in [
        ("https://example.com/a?q=b#c", true),
        ("http://example.com", true),
        ("javascript:alert(1)", false),
        ("data:text/html,hello", false),
        ("tauri://localhost", false),
        ("mailto:someone@example.com", false),
        ("file://remote-host/note.md", false),
    ] {
        let text = format!("[jump](<{destination}>)");
        let mut source = Document::new();
        source.edit(&source.snapshot().token, 1, text.clone())?;
        let result = follow(&source, &text, "jump");
        if allowed {
            assert!(matches!(result?, LinkNavigation::Web { .. }));
        } else {
            assert!(result.is_err(), "Admitted {destination}");
        }
    }
    Ok(())
}

#[test]
fn scheme_less_domains_open_with_https_without_rewriting_markdown() -> Result<(), Box<dyn Error>> {
    for (destination, expected) in [
        ("google.com", "https://google.com/"),
        (
            "www.google.com/search?q=notes#results",
            "https://www.google.com/search?q=notes#results",
        ),
        ("google.com:443/search", "https://google.com/search"),
        ("GOOGLE.COM", "https://google.com/"),
        ("google.com.", "https://google.com./"),
        ("google.com#results", "https://google.com/#results"),
        (
            "google.com?next=https://example.com/",
            "https://google.com/?next=https://example.com/",
        ),
        (
            "google.com/docs/http://example.com",
            "https://google.com/docs/http://example.com",
        ),
        (
            "google.com#https://example.com/",
            "https://google.com/#https://example.com/",
        ),
        ("例え.テスト/path", "https://xn--r8jz45g.xn--zckzah/path"),
    ] {
        let text = format!("[google.com]({destination})");
        let mut source = Document::new();
        source.edit(&source.snapshot().token, 1, text.clone())?;
        let LinkNavigation::Web { url } = follow(&source, &text, "google.com")
            .map_err(|error| format!("{destination}: {error}"))?
        else {
            return Err(format!("Expected a Web link for {destination}").into());
        };
        assert_eq!(url, expected);
        assert_eq!(source.snapshot().text, text);
    }
    Ok(())
}

#[test]
fn a_saved_note_resolves_bare_domains_but_preserves_local_file_intent() -> Result<(), Box<dyn Error>>
{
    let root = tempdir()?;
    fs::write(root.path().join("other.md"), "# Local note")?;
    fs::write(root.path().join("photo.png"), b"image fixture")?;
    fs::write(root.path().join("existing.com"), b"never execute")?;
    fs::create_dir(root.path().join("local.example"))?;
    fs::write(root.path().join("local.example/other.md"), "# Local note")?;
    for (destination, expected) in [
        ("google.com", "web"),
        ("google.com/manual.md", "web"),
        ("other.md", "note"),
        ("photo.png", "attachment"),
        ("existing.com", "error"),
        ("local.example/other.md", "note"),
        ("./google.com", "error"),
        ("../google.com", "error"),
        ("missing.md", "error"),
        ("local.example/missing.md", "error"),
    ] {
        let path = root.path().join("source.md");
        let text = format!("[jump]({destination})");
        fs::write(&path, &text)?;
        let source = Document::open(&path)?;
        match (expected, follow(&source, &text, "jump")) {
            ("web", Ok(LinkNavigation::Web { .. }))
            | ("note", Ok(LinkNavigation::Note { .. }))
            | ("attachment", Ok(LinkNavigation::Attachment { .. }))
            | ("error", Err(_)) => {}
            (_, result) => return Err(format!("Unexpected {destination}: {result:?}").into()),
        }
        assert_eq!(fs::read_to_string(path)?, text);
    }
    Ok(())
}

#[test]
fn domain_inference_refuses_credentials_paths_and_unsupported_protocols()
-> Result<(), Box<dyn Error>> {
    for destination in [
        "javascript:google.com",
        "javascript:alert(1)",
        "data:text/html,google.com",
        "ftp://google.com",
        "custom.scheme://google.com",
        "custom.scheme:///google.com",
        "google.com@other.example",
        "user:password@google.com",
        "google.com:65536",
        "google.com:not-a-port",
        "google..com",
        "-google.com",
        "google-.com",
        "with_under.example",
        "google.com\\path",
        "other.md",
        "photo.jpg",
        "folder/google.com",
    ] {
        let text = format!("[jump](<{destination}>)");
        let mut source = Document::new();
        source.edit(&source.snapshot().token, 1, text.clone())?;
        assert!(
            follow(&source, &text, "jump").is_err(),
            "Admitted {destination}"
        );
    }
    Ok(())
}

#[test]
fn supported_local_extensions_never_become_web_domains_in_untitled_notes()
-> Result<(), Box<dyn Error>> {
    for extension in [
        "md", "markdown", "txt", "pdf", "png", "jpg", "jpeg", "gif", "webp", "avif", "heic", "mp3",
        "m4a", "wav", "mp4", "mov", "MD", "PNG",
    ] {
        let text = format!("[jump](missing.{extension})");
        let mut source = Document::new();
        source.edit(&source.snapshot().token, 1, text.clone())?;
        assert!(
            follow(&source, &text, "jump").is_err(),
            "Admitted {extension}"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn existing_symbolic_links_keep_local_precedence_including_broken_links()
-> Result<(), Box<dyn Error>> {
    let root = tempdir()?;
    fs::write(root.path().join("target.md"), "# Target")?;
    std::os::unix::fs::symlink("target.md", root.path().join("linked.example"))?;
    std::os::unix::fs::symlink("missing.md", root.path().join("broken.example"))?;
    for (destination, expected_note) in [("linked.example", true), ("broken.example", false)] {
        let path = root.path().join("source.md");
        let text = format!("[jump]({destination})");
        fs::write(&path, &text)?;
        let source = Document::open(&path)?;
        let result = follow(&source, &text, "jump");
        if expected_note {
            assert!(matches!(result?, LinkNavigation::Note { .. }));
        } else {
            assert!(result.is_err());
        }
    }
    Ok(())
}

#[test]
fn a_local_inspection_error_does_not_trigger_web_inference() -> Result<(), Box<dyn Error>> {
    let root = tempdir()?;
    let parent = root.path().join("notes");
    fs::create_dir(&parent)?;
    let path = parent.join("source.md");
    let text = "[google.com](google.com)";
    fs::write(&path, text)?;
    let source = Document::open(&path)?;
    fs::rename(&parent, root.path().join("moved"))?;
    fs::write(&parent, b"not a directory")?;
    assert!(matches!(
        follow(&source, text, "google.com"),
        Err(DocumentError::Io(_))
    ));
    assert_eq!(source.snapshot().text, text);
    Ok(())
}

#[test]
fn invalid_or_stale_requests_never_resolve_a_target() -> Result<(), Box<dyn Error>> {
    let text = "🦀 [jump](#heading)\n# Heading\n";
    let mut source = Document::new();
    let token = source.snapshot().token;
    source.edit(&token, 1, text.into())?;
    for (requested_token, revision, requested_text, position) in [
        ("other", 1, text, 4),
        (token.as_str(), 0, text, 4),
        (token.as_str(), 2, text, 4),
        (token.as_str(), 1, "different", 4),
        (token.as_str(), 1, text, usize::MAX),
        (token.as_str(), 1, text, 1),
    ] {
        assert!(
            source
                .follow_link(requested_token, revision, requested_text, position)
                .is_err()
        );
    }
    assert_eq!(source.snapshot().text, text);
    Ok(())
}

#[test]
fn code_frontmatter_missing_targets_and_untitled_relative_links_fail_safely()
-> Result<(), Box<dyn Error>> {
    for (text, marker) in [
        ("`[jump](#heading)`\n# Heading", "jump"),
        ("```md\n[jump](#heading)\n```\n# Heading", "jump"),
        ("---\nlabel: '[jump](#heading)'\n---\n# Heading", "jump"),
        ("[jump](#missing)\n# Heading", "jump"),
        ("[jump](other.md)", "jump"),
        ("plain text", "text"),
    ] {
        let mut source = Document::new();
        source.edit(&source.snapshot().token, 1, text.into())?;
        assert!(follow(&source, text, marker).is_err(), "Admitted {text}");
        assert_eq!(source.snapshot().text, text);
    }
    Ok(())
}

#[test]
fn parent_paths_missing_files_and_attachment_types_use_the_source_directory()
-> Result<(), Box<dyn Error>> {
    let root = tempdir()?;
    let notes = root.path().join("notes");
    fs::create_dir(&notes)?;
    fs::write(root.path().join("photo.PNG"), b"media fixture")?;
    fs::write(root.path().join("script.command"), b"never execute")?;
    fs::write(root.path().join("target.markdown"), "# Target")?;
    for (destination, expected) in [
        ("../photo.PNG", "attachment"),
        ("../target.markdown", "note"),
        ("../script.command", "error"),
        ("../missing.md", "error"),
        ("../", "error"),
        ("../target.markdown?query", "error"),
        ("../target.markdown#missing", "error"),
    ] {
        let path = notes.join("source.md");
        let text = format!("[jump]({destination})");
        fs::write(&path, &text)?;
        let source = Document::open(&path)?;
        match (expected, follow(&source, &text, "jump")) {
            ("attachment", Ok(LinkNavigation::Attachment { path })) => {
                assert_eq!(path, fs::canonicalize(root.path().join("photo.PNG"))?);
            }
            (
                "note",
                Ok(LinkNavigation::Note {
                    selection: None, ..
                }),
            )
            | ("error", Err(_)) => {}
            (_, result) => return Err(format!("Unexpected {destination}: {result:?}").into()),
        }
        assert_eq!(fs::read_to_string(path)?, text);
    }
    Ok(())
}

#[test]
fn encoded_fragments_empty_destinations_and_link_endpoints_use_utf16_positions()
-> Result<(), Box<dyn Error>> {
    for (text, fragment) in [
        (
            "🦀 [jump](#%E6%97%A5%E6%9C%AC%E8%AA%9E)\n# 日本語",
            "# 日本語",
        ),
        ("🦀 [jump](#)\n# Heading", ""),
        ("🦀 [jump]()\n# Heading", ""),
    ] {
        let mut source = Document::new();
        let token = source.snapshot().token;
        source.edit(&token, 1, text.into())?;
        let start = text.find('[').ok_or("Missing link")?;
        let end = text.find(')').ok_or("Missing link ending")? + 1;
        for offset in [start, end] {
            let LinkNavigation::Here { selection } =
                source.follow_link(&token, 1, text, text[..offset].encode_utf16().count())?
            else {
                return Err("Expected current-note navigation".into());
            };
            assert_eq!(
                selection,
                text[..text.find(fragment).ok_or("Missing heading")?]
                    .encode_utf16()
                    .count()
            );
        }
    }
    Ok(())
}
