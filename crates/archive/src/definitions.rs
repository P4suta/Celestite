use crate::links::{ASSET_DIRECTORY, body_offset};
use crate::{NoteId, ObjectId};
use celestite_core::{
    DefinitionAction, LabelVerdict, MAX_FILE_BYTES, Separator, checked_byte_length,
    definition_separator, label_verdict, plan_definition,
};
use pulldown_cmark::{Event, LinkType, Parser, Tag};
use serde::Serialize;
use std::{fmt, ops::Range};

const REFERENCE_SEPARATOR: &str = ": ";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkDestination {
    Note(NoteId),
    Asset { digest: ObjectId, extension: String },
}

impl fmt::Display for LinkDestination {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Note(id) => write!(formatter, "{id}.md"),
            Self::Asset { digest, extension } => {
                write!(
                    formatter,
                    "{ASSET_DIRECTORY}/{}.{extension}",
                    digest.digest()
                )
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPatch {
    pub start: usize,
    pub end: usize,
    pub insert: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkEditError {
    #[error("{0}")]
    Label(&'static str),
    #[error("This note already links \"{label}\" to {existing}; choose different words.")]
    Duplicate { label: String, existing: String },
    #[error("Place the link in the Markdown body, outside code and HTML.")]
    Position,
    #[error("This note exceeds the 16 MiB editing limit.")]
    TooLarge,
    #[error("The inserted link did not parse as a reference link; the note was left unchanged.")]
    Unparsed,
}

fn label_error(verdict: LabelVerdict) -> &'static str {
    match verdict {
        LabelVerdict::Valid => "The link words are valid.",
        LabelVerdict::Empty => "Type a word or two for the link first.",
        LabelVerdict::LineEnding => "Link words must stay on one line.",
        LabelVerdict::UnescapedBracket => "Link words cannot contain unescaped brackets.",
        LabelVerdict::Reserved => "Link words cannot start with a caret.",
        LabelVerdict::TooLong => "Link words are limited to 999 characters.",
    }
}

/// Escapes the three characters that would change a label's bracket structure.
pub fn escape_label(words: &str) -> String {
    let mut label = String::with_capacity(words.len());
    for character in words.chars() {
        if matches!(character, '[' | ']' | '\\') {
            label.push('\\');
        }
        label.push(character);
    }
    label
}

fn collapse_label(label: &str) -> String {
    label
        .split([' ', '\t'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn excluded_ranges(body: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for (event, range) in Parser::new(body).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::MetadataBlock(_)) => {
                ranges.push(range);
            }
            Event::Code(_) | Event::Html(_) | Event::InlineHtml(_) | Event::InlineMath(_) => {
                ranges.push(range);
            }
            _ => {}
        }
    }
    ranges
}

fn definition_at(body: &str, label: &str) -> Option<String> {
    let mut parser = Parser::new(body).into_offset_iter();
    for _ in parser.by_ref() {}
    parser
        .reference_definitions()
        .get(label)
        .map(|definition| definition.dest.to_string())
}

fn last_line_is_definition(body: &str, line_start: usize) -> bool {
    let mut parser = Parser::new(body).into_offset_iter();
    for _ in parser.by_ref() {}
    parser
        .reference_definitions()
        .iter()
        .any(|(_, definition)| definition.span.contains(&line_start))
}

fn separator(body: &str, ending: &str) -> String {
    let trimmed = body.strip_suffix('\n').unwrap_or(body);
    let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
    let line_start = trimmed.rfind('\n').map_or(0, |offset| offset + 1);
    let last_line = trimmed.get(line_start..).unwrap_or_default();
    match definition_separator(
        body.is_empty(),
        body.ends_with('\n'),
        last_line.trim().is_empty(),
        !last_line.trim().is_empty() && last_line_is_definition(body, line_start),
    ) {
        Separator::None => String::new(),
        Separator::Newline => ending.to_owned(),
        Separator::BlankLine => format!("{ending}{ending}"),
    }
}

fn links_at(text: &str, position: usize, destination: &str, image: bool) -> usize {
    let offset = body_offset(text);
    let Some(body) = text.get(offset..) else {
        return 0;
    };
    Parser::new(body)
        .into_offset_iter()
        .filter(|(event, range)| {
            range.start + offset == position
                && match event {
                    Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        ..
                    }) => {
                        !image
                            && *link_type == LinkType::Shortcut
                            && dest_url.as_ref() == destination
                    }
                    Event::Start(Tag::Image {
                        link_type,
                        dest_url,
                        ..
                    }) => {
                        image
                            && *link_type == LinkType::Shortcut
                            && dest_url.as_ref() == destination
                    }
                    _ => false,
                }
        })
        .count()
}

/// Plans the byte patches that turn `source[from..to]` into a shortcut reference link.
///
/// The result replaces the selected words with `[label]` or `![label]` and appends `[label]: destination` to the trailing definition block when the label is new.
/// The patches are returned only when the patched text parses as intended.
pub fn plan_link_patch(
    source: &str,
    from: usize,
    to: usize,
    words: &str,
    destination: &LinkDestination,
    image: bool,
) -> Result<(Vec<LinkPatch>, usize), LinkEditError> {
    let offset = body_offset(source);
    if from > to
        || to > source.len()
        || from < offset
        || !source.is_char_boundary(from)
        || !source.is_char_boundary(to)
    {
        return Err(LinkEditError::Position);
    }
    let body = source.get(offset..).ok_or(LinkEditError::Position)?;
    if excluded_ranges(body)
        .iter()
        .any(|range| range.start + offset < to && from < range.end + offset)
    {
        return Err(LinkEditError::Position);
    }
    let label = escape_label(words.trim());
    match label_verdict(label.as_bytes()) {
        LabelVerdict::Valid => {}
        verdict => return Err(LinkEditError::Label(label_error(verdict))),
    }
    let target = destination.to_string();
    let collapsed = collapse_label(&label);
    let existing = definition_at(body, &collapsed);
    let action = plan_definition(
        existing.is_some(),
        existing.as_deref() == Some(target.as_str()),
    );
    let body_insert = if image {
        format!("![{label}]")
    } else {
        format!("[{label}]")
    };
    let mut patches = vec![LinkPatch {
        start: from,
        end: to,
        insert: body_insert.clone(),
    }];
    match action {
        DefinitionAction::Refuse => {
            return Err(LinkEditError::Duplicate {
                label: collapsed,
                existing: existing.unwrap_or_default(),
            });
        }
        DefinitionAction::Keep => {}
        DefinitionAction::Append => {
            let ending = if source.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let insert = format!(
                "{}[{label}]{REFERENCE_SEPARATOR}{target}{ending}",
                separator(body, ending)
            );
            patches.push(LinkPatch {
                start: source.len(),
                end: source.len(),
                insert,
            });
        }
    }
    let added: usize = patches.iter().map(|patch| patch.insert.len()).sum();
    checked_byte_length(source.len() - (to - from), added, MAX_FILE_BYTES)
        .ok_or(LinkEditError::TooLarge)?;
    let mut patched = source.to_owned();
    for patch in patches.iter().rev() {
        patched.replace_range(patch.start..patch.end, &patch.insert);
    }
    if links_at(&patched, from, &target, image) != 1
        || definition_at(
            patched.get(body_offset(&patched)..).unwrap_or_default(),
            &collapsed,
        )
        .as_deref()
            != Some(target.as_str())
    {
        return Err(LinkEditError::Unparsed);
    }
    Ok((patches, from + body_insert.len()))
}
