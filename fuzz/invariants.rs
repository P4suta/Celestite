use celestite_archive::{
    LinkDestination, NoteId, ObjectId, Record, body_offset, heading_offset, identity, link_at,
    managed_source, note_filename_identity, note_title, plan_link_patch,
};
use std::error::Error;

fn check_links(text: &str, destination: &LinkDestination) -> Result<(), Box<dyn Error>> {
    let offset = body_offset(text);
    assert!(offset <= text.len() && text.is_char_boundary(offset));
    if let Some(title) = note_title(text) {
        assert!(!title.is_empty());
        assert_eq!(title.trim(), title);
        assert!(!title.contains(['\n', '\r']));
    }
    if let Some(id) = note_filename_identity(text) {
        assert_eq!(note_filename_identity(&format!("{id}.md")), Some(id));
    }
    for (from, to) in [(0, 0), (offset, text.len()), (text.len() / 2, text.len())] {
        let target = destination.to_string();
        for image in [false, true] {
            if let Ok((patches, caret)) = plan_link_patch(text, from, to, text, destination, image)
            {
                let mut patched = text.to_owned();
                for patch in patches.iter().rev() {
                    assert!(patch.start <= patch.end && patch.end <= text.len());
                    patched.replace_range(patch.start..patch.end, &patch.insert);
                }
                assert!(patched.len() <= celestite_core::MAX_FILE_BYTES);
                assert!(patched.is_char_boundary(caret));
                assert!(patched.contains(&format!("]: {target}")));
            }
        }
    }
    Ok(())
}

fn check_text(text: &str) -> Result<(), Box<dyn Error>> {
    assert!(link_at(text, usize::MAX).is_none());
    assert_eq!(heading_offset(text, ""), Some(0));
    let destination = LinkDestination::Note("019a1234-5678-7000-8000-000000000002".parse()?);
    check_links(text, &destination)?;
    check_links(
        text,
        &LinkDestination::Asset {
            digest: ObjectId::of(text.as_bytes()),
            extension: String::from("png"),
        },
    )?;
    for position in [0, text.len() / 2, text.len()] {
        let link = link_at(text, position);
        if !text.is_char_boundary(position) {
            assert!(link.is_none());
        }
        if let Some(destination) = link
            && let Some(fragment) = destination.strip_prefix('#')
            && let Some(offset) = heading_offset(text, fragment)
        {
            assert!(offset <= text.len() && text.is_char_boundary(offset));
        }
    }
    if let Ok(note) = text.parse::<NoteId>() {
        assert!(!note.uuid().is_nil() && !note.uuid().is_max());
        assert_eq!(note.to_string().parse::<NoteId>().ok(), Some(note));
        let json = serde_json::to_string(&note)?;
        assert_eq!(serde_json::from_str::<NoteId>(&json).ok(), Some(note));
    }
    if let Ok(object) = text.parse::<ObjectId>() {
        assert_eq!(object.digest().len(), 64);
        assert!(
            object
                .digest()
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert_eq!(
            object.to_string().parse::<ObjectId>().ok(),
            Some(object.clone())
        );
        let json = serde_json::to_string(&object)?;
        assert_eq!(serde_json::from_str::<ObjectId>(&json).ok(), Some(object));
    }
    if let Ok(note) = serde_json::from_str::<NoteId>(text) {
        assert_eq!(note.to_string().parse::<NoteId>().ok(), Some(note));
    }
    if let Ok(object) = serde_json::from_str::<ObjectId>(text) {
        assert_eq!(object.to_string().parse::<ObjectId>().ok(), Some(object));
    }
    if let Ok(record) = serde_json::from_str::<Record>(text) {
        let encoded = serde_json::to_string(&record)?;
        let decoded: Record = serde_json::from_str(&encoded)?;
        assert_eq!(
            serde_json::to_value(&record)?,
            serde_json::to_value(&decoded)?
        );
    }
    let fallback: NoteId = "019a1234-5678-7000-8000-000000000001".parse()?;
    let id = match identity(text) {
        Ok(Some(id)) => id,
        _ => fallback,
    };
    if let Ok(source) = managed_source(id, text) {
        assert_eq!(identity(&source)?, Some(id));
        assert_eq!(managed_source(id, &source)?, source);
    }
    Ok(())
}

pub fn check(data: &[u8]) -> Result<(), Box<dyn Error>> {
    if let Ok(text) = std::str::from_utf8(data) {
        check_text(text)?;
        check_text(text.trim_end_matches(['\r', '\n']))?;
    }
    Ok(())
}
