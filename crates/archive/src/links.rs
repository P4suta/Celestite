use crate::storage::{invalid, read_bounded, relative_path};
use crate::{
    ArchiveError, CommitReceipt, Entry, LinkBinding, LinkTarget, NoteId, NoteRecord, ObjectId,
    Record, Vault,
};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, LinkType, Parser, Tag, TagEnd};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    ops::Range,
    path::Path,
};

const BOM: &str = "\u{feff}";
const FRONT_DELIMITER: &str = "---";
const ID_KEY: &str = "id:";
const INLINE_DELIMITER: &str = "](";
const REFERENCE_DELIMITER: &str = "]:";

fn metadata(source: &str) -> Option<Range<usize>> {
    let offset = source.strip_prefix(BOM).map_or(0, |_| BOM.len());
    let tail = source.get(offset..)?;
    let mut lines = tail.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end_matches(['\r', '\n']) != FRONT_DELIMITER {
        return None;
    }
    let start = offset + first.len();
    let mut end = start;
    for line in lines {
        if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            return Some(start..end);
        }
        end += line.len();
    }
    None
}

pub fn identity(source: &str) -> Result<Option<NoteId>, ArchiveError> {
    let Some(range) = metadata(source) else {
        return Ok(None);
    };
    let mut found = None;
    for line in source[range].lines() {
        if let Some(value) = line.strip_prefix(ID_KEY) {
            if found.is_some() {
                return invalid("Duplicate note identity metadata");
            }
            found = Some(value.trim().trim_matches(['\'', '"']).parse()?);
        }
    }
    Ok(found)
}

pub fn managed_source(id: NoteId, source: &str) -> Result<String, ArchiveError> {
    if let Some(existing) = identity(source)? {
        if existing != id {
            return invalid("Note identity differs from its source");
        }
        return Ok(source.to_owned());
    }
    let ending = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if let Some(range) = metadata(source) {
        return Ok(format!(
            "{}id: {id}{ending}{}",
            &source[..range.start],
            &source[range.start..]
        ));
    }
    let (prefix, body) = source
        .strip_prefix(BOM)
        .map_or(("", source), |body| (BOM, body));
    Ok(format!(
        "{prefix}---{ending}id: {id}{ending}---{ending}{body}"
    ))
}

fn destination_span(
    source: &str,
    range: Range<usize>,
    url: &str,
    reference: bool,
) -> Option<Range<usize>> {
    let text = source.get(range.clone())?;
    let delimiter = if reference {
        REFERENCE_DELIMITER
    } else {
        INLINE_DELIMITER
    };
    let mut candidates = BTreeSet::new();
    for (offset, _) in text.match_indices(delimiter) {
        let after = offset + delimiter.len();
        let rest = text.get(after..)?.trim_start();
        let leading = text.get(after..)?.len() - rest.len();
        let angle = usize::from(rest.starts_with('<'));
        let start = range.start + after + leading + angle;
        if source.get(start..start.checked_add(url.len())?) == Some(url) {
            candidates.insert((start, start + url.len()));
        }
    }
    if candidates.len() != 1 {
        return None;
    }
    candidates.into_iter().next().map(|(start, end)| start..end)
}

/// Returns the byte offset where Markdown begins, after any BOM and front matter.
pub fn body_offset(source: &str) -> usize {
    metadata(source).map_or_else(
        || source.strip_prefix(BOM).map_or(0, |_| BOM.len()),
        |range| {
            range.end
                + source
                    .get(range.end..)
                    .and_then(|tail| tail.split_inclusive('\n').next())
                    .map_or(0, str::len)
        },
    )
}

/// Returns the note identity named by a relative destination of the form `<uuid>.md`.
///
/// An optional `./` prefix, a `#fragment` or `?query` suffix, percent encoding,
/// and an ASCII case-insensitive extension are accepted; the stem must be the lowercase hyphenated spelling of a nonreserved UUID.
pub fn note_filename_identity(destination: &str) -> Option<NoteId> {
    let path = destination.split(['#', '?']).next().unwrap_or_default();
    let decoded = percent_decode_str(path).decode_utf8().ok()?;
    let name = decoded.strip_prefix("./").unwrap_or(&decoded);
    if name.contains(['/', '\\']) {
        return None;
    }
    let (stem, extension) = name.rsplit_once('.')?;
    if !extension.eq_ignore_ascii_case("md") {
        return None;
    }
    let id: NoteId = stem.parse().ok()?;
    (id.to_string() == stem).then_some(id)
}

fn heading_text(
    source: &str,
) -> impl Iterator<Item = (usize, pulldown_cmark::HeadingLevel, String)> {
    let offset = body_offset(source);
    let mut heading: Option<(usize, pulldown_cmark::HeadingLevel, String)> = None;
    let mut headings = Vec::new();
    if let Some(body) = source.get(offset..) {
        for (event, range) in Parser::new(body).into_offset_iter() {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    heading = Some((range.start + offset, level, String::new()));
                }
                Event::Text(text) | Event::Code(text) => {
                    if let Some((_, _, title)) = &mut heading {
                        title.push_str(&text);
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    if let Some((_, _, title)) = &mut heading {
                        title.push(' ');
                    }
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(found) = heading.take() {
                        headings.push(found);
                    }
                }
                _ => {}
            }
        }
    }
    headings.into_iter()
}

/// Returns the note's title: its first level-one heading, trimmed, with ASCII whitespace runs collapsed.
pub fn note_title(source: &str) -> Option<String> {
    heading_text(source)
        .find(|(_, level, _)| *level == pulldown_cmark::HeadingLevel::H1)
        .map(|(_, _, title)| {
            title
                .split([' ', '\t', '\n', '\r'])
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_owned()
        })
        .filter(|title| !title.is_empty())
}

pub fn link_at(source: &str, position: usize) -> Option<String> {
    if !source.is_char_boundary(position) {
        return None;
    }
    let offset = body_offset(source);
    let mut ending = None;
    for (event, range) in Parser::new(source.get(offset..)?).into_offset_iter() {
        if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) = event {
            let range = range.start + offset..range.end + offset;
            if range.contains(&position) {
                return Some(dest_url.into_string());
            }
            if range.end == position {
                ending = Some(dest_url.into_string());
            }
        }
    }
    ending
}

pub fn heading_offset(source: &str, fragment: &str) -> Option<usize> {
    if fragment.is_empty() {
        return Some(0);
    }
    let mut used = BTreeSet::new();
    for (position, _, title) in heading_text(source) {
        let base: String = title
            .chars()
            .flat_map(char::to_lowercase)
            .filter_map(|value| {
                if value.is_alphanumeric() || matches!(value, '-' | '_') {
                    Some(value)
                } else if value.is_whitespace() {
                    Some('-')
                } else {
                    None
                }
            })
            .collect();
        let mut id = base.clone();
        let mut suffix = 0;
        while !used.insert(id.clone()) {
            suffix += 1;
            id = format!("{base}-{suffix}");
        }
        if id == fragment || title == fragment {
            return Some(position);
        }
    }
    None
}

fn destinations(source: &str) -> Vec<(Range<usize>, String, bool)> {
    let offset = body_offset(source);
    let mut parser = Parser::new(&source[offset..]).into_offset_iter();
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    while let Some((event, range)) = parser.next() {
        let Event::Start(
            Tag::Link {
                link_type,
                dest_url,
                id,
                ..
            }
            | Tag::Image {
                link_type,
                dest_url,
                id,
                ..
            },
        ) = event
        else {
            continue;
        };
        let reference = matches!(
            link_type,
            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
        );
        let target_range = if reference {
            parser
                .reference_definitions()
                .get(id.as_ref())
                .map(|value| value.span.clone())
                .unwrap_or(range)
        } else {
            range
        };
        let target_range = target_range.start + offset..target_range.end + offset;
        let external = matches!(link_type, LinkType::Autolink | LinkType::Email);
        if let Some(span) = destination_span(source, target_range.clone(), &dest_url, reference) {
            if seen.insert((span.start, span.end)) {
                result.push((span, dest_url.to_string(), external));
            }
        } else if seen.insert((target_range.start, target_range.start)) {
            result.push((
                target_range.start..target_range.start,
                dest_url.to_string(),
                external,
            ));
        }
    }
    let mut unused: Vec<_> = parser
        .reference_definitions()
        .iter()
        .map(|(_, definition)| {
            (
                definition.span.start + offset..definition.span.end + offset,
                definition.dest.to_string(),
            )
        })
        .collect();
    unused.sort_by_key(|(range, _)| range.start);
    for (range, destination) in unused {
        if let Some(span) = destination_span(source, range.clone(), &destination, true) {
            if seen.insert((span.start, span.end)) {
                result.push((span, destination, false));
            }
        } else if seen.insert((range.start, range.start)) {
            result.push((range.start..range.start, destination, false));
        }
    }
    result.sort_by_key(|(range, _, _)| range.start);
    result
}

pub(crate) fn linked_path(note_path: &str, destination: &str) -> Result<String, ArchiveError> {
    let path = destination.split(['#', '?']).next().unwrap_or_default();
    let decoded = percent_decode_str(path)
        .decode_utf8()
        .map_err(|error| ArchiveError::Invalid(error.to_string()))?;
    let parent = Path::new(note_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    relative_path(&parent.join(decoded.as_ref()))
}

pub const ASSET_DIRECTORY: &str = "assets";

/// Returns the digest named by a logical path of the form `assets/<64 hex>.<ext>`.
pub fn asset_digest(path: &str) -> Option<&str> {
    let name = path.strip_prefix(ASSET_DIRECTORY)?.strip_prefix('/')?;
    let (stem, extension) = name.rsplit_once('.')?;
    let digest_shape = stem.len() == celestite_archive_core::DIGEST_LENGTH
        && stem
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    let extension_shape =
        !extension.is_empty() && extension.bytes().all(|byte| byte.is_ascii_alphanumeric());
    (digest_shape && extension_shape && !name.contains('/')).then_some(stem)
}

fn external(destination: &str) -> bool {
    destination.starts_with('#')
        || destination.starts_with("//")
        || destination
            .split('/')
            .next()
            .is_some_and(|part| part.contains(':'))
}

impl Vault {
    /// Captures one editor buffer.
    ///
    /// A file directly inside `library` keeps its bare filename as its logical path and resolves relative links against the library root; any other file uses the private `files/{id}/{name}` path and its own directory.
    pub fn capture_file(
        &self,
        id: NoteId,
        file: Option<&Path>,
        bytes: &[u8],
        base: Option<ObjectId>,
        library: Option<&Path>,
    ) -> Result<CommitReceipt, ArchiveError> {
        let name = file
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{id}.md"));
        let notes = match &base {
            Some(base) => self.notes(base)?,
            None => BTreeMap::new(),
        };
        let previous = match notes.get(&id) {
            Some(Entry::Live { record }) => Some(self.note(record)?),
            _ => None,
        };
        let in_library = match (file.and_then(Path::parent), library) {
            (Some(parent), Some(library)) => parent == library,
            _ => false,
        };
        let root = if in_library {
            library
        } else {
            file.and_then(Path::parent)
        };
        let prepared =
            self.prepare_note(id, &name, bytes, root, &BTreeMap::new(), previous.as_ref())?;
        let mut note = self.note(&prepared)?;
        if !in_library {
            note.path = format!("files/{id}/{name}");
        }
        note.origin = file.map(|value| value.to_string_lossy().into_owned());
        let record = self.put_record(&Record::Note { note })?;
        let entry = match notes.get(&id) {
            Some(Entry::Conflict { alternatives }) => {
                let mut alternatives = alternatives.clone();
                alternatives.push(Some(record));
                alternatives.sort();
                alternatives.dedup();
                Entry::Conflict { alternatives }
            }
            _ => Entry::Live { record },
        };
        if notes.get(&id) == Some(&entry)
            && self.current()? == base
            && let Some(snapshot) = base
        {
            return Ok(CommitReceipt {
                snapshot,
                conflicts: crate::storage::conflict_ids(&notes),
                local_committed: true,
            });
        }
        self.commit_notes(base, BTreeMap::from([(id, entry)]))
    }

    pub fn prepare_note(
        &self,
        id: NoteId,
        path: &str,
        original: &[u8],
        input_root: Option<&Path>,
        paths: &BTreeMap<String, NoteId>,
        previous: Option<&NoteRecord>,
    ) -> Result<ObjectId, ArchiveError> {
        let path = relative_path(Path::new(path))?;
        if original.len() > celestite_core::MAX_FILE_BYTES {
            return Err(ArchiveError::TooLarge);
        }
        let source = managed_source(id, std::str::from_utf8(original)?)?;
        if source.len() > celestite_core::MAX_FILE_BYTES {
            return Err(ArchiveError::TooLarge);
        }
        let original = self.put_bytes(original)?;
        let source_id = self.put_bytes(source.as_bytes())?;
        let mut links = Vec::new();
        for (span, destination, autolink) in destinations(&source) {
            let target = if autolink || external(&destination) {
                LinkTarget::External
            } else if span.is_empty() {
                LinkTarget::Unresolved {
                    reason: String::from("Destination has no unambiguous source span"),
                }
            } else {
                self.resolve_link(&path, &destination, input_root, paths, previous)?
            };
            links.push(LinkBinding {
                start: span.start,
                end: span.end,
                destination,
                target,
            });
        }
        self.put_record(&Record::Note {
            note: NoteRecord {
                id,
                path,
                source: source_id,
                original,
                origin: None,
                links,
            },
        })
    }

    fn resolve_link(
        &self,
        note_path: &str,
        destination: &str,
        root: Option<&Path>,
        paths: &BTreeMap<String, NoteId>,
        previous: Option<&NoteRecord>,
    ) -> Result<LinkTarget, ArchiveError> {
        let unresolved = |reason: &str| LinkTarget::Unresolved {
            reason: reason.to_owned(),
        };
        let Ok(path) = linked_path(note_path, destination) else {
            return Ok(unresolved(
                "Link escapes its workspace or has invalid encoding",
            ));
        };
        if let Some(id) = note_filename_identity(destination) {
            return Ok(LinkTarget::Note { id });
        }
        let old = previous
            .into_iter()
            .flat_map(|note| &note.links)
            .filter(|link| link.destination == destination)
            .filter_map(|link| match link.target {
                LinkTarget::Note { id } => Some(id),
                LinkTarget::Ambiguous { previous, .. } => Some(previous),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        if old.len() == 1
            && let Some(id) = old.iter().next().copied()
        {
            match paths.get(&path) {
                Some(current) if *current != id => {
                    return Ok(LinkTarget::Ambiguous {
                        previous: id,
                        current: Some(*current),
                    });
                }
                None => {
                    return Ok(LinkTarget::Note { id });
                }
                _ => {}
            }
        }
        if let Some(id) = paths.get(&path) {
            return Ok(LinkTarget::Note { id: *id });
        }
        let Some(root) = root else {
            return Ok(unresolved("The destination has not been captured"));
        };
        let candidate = root.join(&path);
        match fs::canonicalize(&candidate) {
            Ok(canonical)
                if canonical.starts_with(fs::canonicalize(root)?)
                    && fs::symlink_metadata(&candidate)?.is_file() =>
            {
                if candidate
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("md"))
                {
                    return Ok(unresolved(
                        "The target Markdown note needs an identity-aware import",
                    ));
                }
                let object = self.put_file(&candidate)?;
                if let Some(named) = asset_digest(&path)
                    && named != object.digest()
                {
                    return Ok(unresolved("The asset name does not match its content"));
                }
                Ok(LinkTarget::File { path, object })
            }
            Ok(_) => Ok(unresolved("The destination leaves the selected workspace")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(unresolved("The destination is missing"))
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn capture(
        &self,
        id: NoteId,
        path: &str,
        bytes: &[u8],
        root: Option<&Path>,
        base: Option<ObjectId>,
    ) -> Result<CommitReceipt, ArchiveError> {
        let notes = match &base {
            Some(base) => self.notes(base)?,
            None => BTreeMap::new(),
        };
        let paths = self.path_map(&notes)?;
        let previous = match notes.get(&id) {
            Some(Entry::Live { record }) => Some(self.note(record)?),
            _ => None,
        };
        let record = self.prepare_note(id, path, bytes, root, &paths, previous.as_ref())?;
        let entry = match notes.get(&id) {
            Some(Entry::Conflict { alternatives }) => {
                let mut alternatives = alternatives.clone();
                alternatives.push(Some(record));
                alternatives.sort();
                alternatives.dedup();
                Entry::Conflict { alternatives }
            }
            _ => Entry::Live { record },
        };
        if notes.get(&id) == Some(&entry)
            && self.current()? == base
            && let Some(snapshot) = base
        {
            return Ok(CommitReceipt {
                snapshot,
                conflicts: crate::storage::conflict_ids(&notes),
                local_committed: true,
            });
        }
        self.commit_notes(base, BTreeMap::from([(id, entry)]))
    }

    pub(crate) fn path_map(
        &self,
        notes: &BTreeMap<NoteId, Entry>,
    ) -> Result<BTreeMap<String, NoteId>, ArchiveError> {
        let mut paths = BTreeMap::new();
        for (id, entry) in notes {
            if let Entry::Live { record } = entry {
                let note = self.note(record)?;
                if paths.insert(note.path, *id).is_some() {
                    return invalid("Multiple notes occupy the same logical path");
                }
            }
        }
        Ok(paths)
    }

    pub fn import_directory(
        &self,
        input: &Path,
        base: Option<ObjectId>,
    ) -> Result<CommitReceipt, ArchiveError> {
        let input = fs::canonicalize(input)?;
        if self.root.starts_with(&input) || input.starts_with(&self.root) {
            return invalid("The input and archive must be separate directories");
        }
        let before = match &base {
            Some(base) => self.notes(base)?,
            None => BTreeMap::new(),
        };
        let previous_paths = self.path_map(&before)?;
        let mut files = Vec::new();
        let mut directories = vec![input.clone()];
        while let Some(directory) = directories.pop() {
            for item in fs::read_dir(directory)? {
                let item = item?;
                let kind = item.file_type()?;
                if kind.is_dir() {
                    directories.push(item.path());
                } else if kind.is_file()
                    && item
                        .path()
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| value.eq_ignore_ascii_case("md"))
                {
                    files.push(item.path());
                }
                if files.len() + directories.len() > crate::model::MAX_GRAPH_OBJECTS {
                    return Err(ArchiveError::TooLarge);
                }
            }
        }
        files.sort();
        let mut inputs = Vec::new();
        let mut paths = BTreeMap::new();
        for file in files {
            let logical = relative_path(
                file.strip_prefix(&input)
                    .map_err(|error| ArchiveError::Invalid(error.to_string()))?,
            )?;
            let bytes = read_bounded(&file, celestite_core::MAX_FILE_BYTES)?;
            let id = identity(std::str::from_utf8(&bytes)?)?
                .or_else(|| previous_paths.get(&logical).copied())
                .or_else(|| {
                    file.file_stem()
                        .and_then(|value| value.to_str())
                        .and_then(|value| value.parse().ok())
                })
                .unwrap_or_default();
            paths.insert(logical.clone(), id);
            inputs.push((file, logical, id, bytes));
        }
        let mut changes: BTreeMap<NoteId, Entry> = BTreeMap::new();
        for (file, logical, id, bytes) in inputs {
            let previous = match before.get(&id) {
                Some(Entry::Live { record }) => Some(self.note(record)?),
                _ => None,
            };
            let record = self.prepare_note(
                id,
                &logical,
                &bytes,
                Some(&input),
                &paths,
                previous.as_ref(),
            )?;
            if read_bounded(&file, celestite_core::MAX_FILE_BYTES)? != bytes {
                return invalid("A source file changed during import");
            }
            let entry = match changes.remove(&id) {
                None => Entry::Live { record },
                Some(existing) => {
                    let mut alternatives = existing.alternatives();
                    alternatives.push(Some(record));
                    alternatives.sort();
                    alternatives.dedup();
                    if alternatives.len() == 1 {
                        Entry::Live {
                            record: alternatives[0].clone().ok_or_else(|| {
                                ArchiveError::Invalid(String::from("Missing note alternative"))
                            })?,
                        }
                    } else {
                        Entry::Conflict { alternatives }
                    }
                }
            };
            changes.insert(id, entry);
        }
        self.commit_notes(base, changes)
    }
}
