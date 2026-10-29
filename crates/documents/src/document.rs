use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub use celestite_core::MAX_FILE_BYTES;
use celestite_core::{
    CloseDecision, DiskState, EditRevision, EditTransition, NavigationDecision, OpenTarget,
    RECOVERY_FORMAT, RecoveryFacts, RevisionState, WriteDecision, admit_recovery, encoded_length,
    open_target, plan_close, plan_navigation, plan_write,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{DocumentError, LinkNavigation};

const UTF8_BOM: &str = "\u{feff}";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshot {
    pub token: String,
    pub text: String,
    pub path: Option<String>,
    pub name: String,
    pub title: Option<String>,
    pub folder: Option<String>,
    pub revision: u64,
    pub saved_revision: u64,
}

pub use celestite_archive::LinkPatch;

/// The words an editor selects for a reference link, as UTF-16 offsets into the current text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkInsertion {
    pub from: usize,
    pub to: usize,
    pub words: String,
    pub image: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkEdit {
    pub token: String,
    pub revision: u64,
    pub patches: Vec<LinkPatch>,
    pub caret: usize,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
enum LineEnding {
    #[default]
    Lf,
    CrLf,
    Cr,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
struct TextFormat {
    ending: LineEnding,
    bom: bool,
}

impl TextFormat {
    fn decode(bytes: &[u8]) -> Result<(String, Self), DocumentError> {
        let bom = bytes.starts_with(UTF8_BOM.as_bytes());
        let source = if bom {
            bytes
                .get(UTF8_BOM.len()..)
                .ok_or(DocumentError::InvalidEncoding)?
        } else {
            bytes
        };
        let source = std::str::from_utf8(source).map_err(|_| DocumentError::InvalidEncoding)?;
        let without_pairs = source.replace("\r\n", "");
        let ending = if source.contains("\r\n") && !without_pairs.contains(['\r', '\n']) {
            LineEnding::CrLf
        } else if source.contains('\r') && !source.contains('\n') {
            LineEnding::Cr
        } else {
            LineEnding::Lf
        };
        Ok((
            source.replace("\r\n", "\n").replace('\r', "\n"),
            Self { ending, bom },
        ))
    }

    fn encode(self, text: &str) -> Result<Vec<u8>, DocumentError> {
        let length = encoded_length(
            text.len(),
            text.bytes().filter(|byte| *byte == b'\n').count(),
            self.bom,
            matches!(self.ending, LineEnding::CrLf),
        )
        .ok_or(DocumentError::TooLarge)?;
        let mut result = String::with_capacity(length);
        if self.bom {
            result.push_str(UTF8_BOM);
        }
        match self.ending {
            LineEnding::Lf => result.push_str(text),
            LineEnding::CrLf => result.push_str(&text.replace('\n', "\r\n")),
            LineEnding::Cr => result.push_str(&text.replace('\n', "\r")),
        }
        Ok(result.into_bytes())
    }
}

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, DocumentError> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(DocumentError::InvalidPath);
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(DocumentError::TooLarge);
    }
    Ok(bytes)
}

fn fingerprint(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn destination(path: &Path) -> Result<PathBuf, DocumentError> {
    let filename = path.file_name().ok_or(DocumentError::InvalidPath)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let path = fs::canonicalize(parent)?.join(filename);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() => Err(DocumentError::InvalidPath),
        Ok(_) => Ok(fs::canonicalize(path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), DocumentError> {
    let parent = path.parent().ok_or(DocumentError::InvalidPath)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    match fs::metadata(path) {
        Ok(metadata) => {
            let permissions = metadata.permissions();
            if permissions.readonly() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "The destination is read-only.",
                )
                .into());
            }
            temporary.as_file().set_permissions(permissions)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| DocumentError::Io(error.error))?;
    Ok(())
}

#[derive(Debug)]
pub struct Document {
    id: Uuid,
    token: Uuid,
    recovery_key: Uuid,
    text: String,
    path: Option<PathBuf>,
    revisions: RevisionState,
    fingerprint: Option<[u8; 32]>,
    original_bytes: Option<Vec<u8>>,
    format: TextFormat,
    pub(crate) archive_base: Option<celestite_archive::ObjectId>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    pub fn new() -> Self {
        Self {
            id: Uuid::now_v7(),
            token: Uuid::new_v4(),
            recovery_key: Uuid::new_v4(),
            text: String::new(),
            path: None,
            revisions: RevisionState::EMPTY,
            fingerprint: None,
            original_bytes: None,
            format: TextFormat::default(),
            archive_base: None,
        }
    }

    /// Creates an untitled note whose text starts with its identity front matter.
    ///
    /// The note receives its `<id>.md` path in the library at its first nonempty checkpoint.
    pub fn new_in_library() -> Self {
        let mut document = Self::new();
        document.text = format!("---\nid: {}\n---\n\n", document.id);
        document
    }

    pub fn open(path: &Path) -> Result<Self, DocumentError> {
        let path = fs::canonicalize(path)?;
        let bytes = read_bounded(&path, MAX_FILE_BYTES)?;
        let (text, format) = TextFormat::decode(&bytes)?;
        let identity = celestite_archive::identity(&text)?.or_else(|| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .and_then(|name| name.parse::<celestite_archive::NoteId>().ok())
        });
        let mut document = Self::new();
        if let Some(identity) = identity {
            document.id = identity.uuid();
        }
        Ok(Self {
            path: Some(path),
            text,
            fingerprint: Some(fingerprint(&bytes)),
            original_bytes: Some(bytes),
            format,
            ..document
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn checkpoint_key(&self) -> Uuid {
        self.recovery_key
    }

    pub(crate) fn adopt_id(&mut self, id: celestite_archive::NoteId) {
        self.id = id.uuid();
    }

    pub(crate) fn archive_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        self.encoded_text()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Reports whether the Markdown after any front matter is blank.
    pub fn body_is_empty(&self) -> bool {
        self.text
            .get(celestite_archive::body_offset(&self.text)..)
            .is_none_or(|body| body.trim().is_empty())
    }

    pub fn may_reuse(&self) -> bool {
        open_target(self.path.is_some(), self.body_is_empty()) == OpenTarget::Reuse
    }

    /// Plans the patches that turn the words at `link.from..link.to` into a reference link.
    ///
    /// Positions are UTF-16 offsets into the exact current text; the document is not changed.
    pub fn insert_link(
        &self,
        token: &str,
        revision: u64,
        text: &str,
        link: &LinkInsertion,
        destination: &celestite_archive::LinkDestination,
    ) -> Result<LinkEdit, DocumentError> {
        let LinkInsertion {
            from,
            to,
            words,
            image,
        } = link;
        let (from, to, image) = (*from, *to, *image);
        let length = self.text.encode_utf16().count();
        for position in [from, to] {
            match plan_navigation(
                self.revisions.current(),
                revision,
                token == self.token.to_string(),
                text == self.text,
                position,
                length,
            ) {
                NavigationDecision::Stale => return Err(DocumentError::Stale),
                NavigationDecision::InvalidPosition => {
                    return Err(DocumentError::Link("Invalid link position."));
                }
                NavigationDecision::Follow => {}
            }
        }
        let start = crate::navigation::byte_offset(&self.text, from)
            .ok_or(DocumentError::Link("Invalid link position."))?;
        let end = crate::navigation::byte_offset(&self.text, to)
            .ok_or(DocumentError::Link("Invalid link position."))?;
        let (patches, caret) =
            celestite_archive::plan_link_patch(&self.text, start, end, words, destination, image)?;
        let units = |offset: usize| {
            self.text
                .get(..offset)
                .map(|prefix| prefix.encode_utf16().count())
                .ok_or(DocumentError::Link("Invalid link position."))
        };
        let caret = patches
            .first()
            .filter(|body| body.start == start && caret == start + body.insert.len())
            .map(|body| from + body.insert.encode_utf16().count())
            .ok_or(DocumentError::Link("Invalid link position."))?;
        let patches = patches
            .into_iter()
            .map(|patch| {
                Ok(LinkPatch {
                    start: units(patch.start)?,
                    end: units(patch.end)?,
                    insert: patch.insert,
                })
            })
            .collect::<Result<Vec<_>, DocumentError>>()?;
        Ok(LinkEdit {
            token: self.token.to_string(),
            revision,
            patches,
            caret,
        })
    }

    pub fn follow_link(
        &self,
        token: &str,
        revision: u64,
        text: &str,
        position: usize,
    ) -> Result<LinkNavigation, DocumentError> {
        match plan_navigation(
            self.revisions.current(),
            revision,
            token == self.token.to_string(),
            text == self.text,
            position,
            self.text.encode_utf16().count(),
        ) {
            NavigationDecision::Stale => return Err(DocumentError::Stale),
            NavigationDecision::InvalidPosition => {
                return Err(DocumentError::Link("Invalid link position."));
            }
            NavigationDecision::Follow => {}
        }
        let offset = crate::navigation::byte_offset(&self.text, position)
            .ok_or(DocumentError::Link("Invalid link position."))?;
        crate::navigation::resolve(&self.text, self.path(), offset)
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.revisions.is_dirty()
    }

    pub fn may_close(&self, token: &str, revision: u64, discard: bool) -> bool {
        plan_close(
            self.revisions.current(),
            revision,
            token == self.token.to_string(),
            self.has_unsaved_changes(),
            discard,
        ) == CloseDecision::Allow
    }

    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            token: self.token.to_string(),
            text: self.text.clone(),
            path: self.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            name: self.path.as_ref().and_then(|p| p.file_name()).map_or_else(
                || format!("{}.md", self.id),
                |name| name.to_string_lossy().into_owned(),
            ),
            title: celestite_archive::note_title(&self.text),
            folder: self
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.to_string_lossy().into_owned()),
            revision: self.revisions.current(),
            saved_revision: self.revisions.saved(),
        }
    }

    pub fn edit(&mut self, token: &str, revision: u64, text: String) -> Result<(), DocumentError> {
        let revisions = match self.revisions.edit(
            revision,
            token == self.token.to_string(),
            text != self.text,
            text.len(),
        ) {
            EditTransition::Accepted(revisions) => revisions,
            EditTransition::Stale => return Err(DocumentError::Stale),
            EditTransition::InvalidRevision => return Err(DocumentError::InvalidRevision),
            EditTransition::TooLarge => return Err(DocumentError::TooLarge),
        };
        if self.text != text {
            self.original_bytes = None;
        }
        self.text = text;
        self.revisions = revisions;
        Ok(())
    }

    pub fn has_external_changes(&self) -> Result<bool, DocumentError> {
        let Some(path) = &self.path else {
            return Ok(false);
        };
        match read_bounded(path, MAX_FILE_BYTES) {
            Ok(bytes) => Ok(Some(fingerprint(&bytes)) != self.fingerprint),
            Err(DocumentError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    pub fn save(&mut self) -> Result<(), DocumentError> {
        let path = self.path.as_ref().ok_or(DocumentError::InvalidPath)?;
        let disk = if self.has_external_changes()? {
            DiskState::Changed
        } else {
            DiskState::Unchanged
        };
        match plan_write(
            EditRevision::new(self.revisions.saved()),
            EditRevision::new(self.revisions.current()),
            disk,
        ) {
            WriteDecision::Stale => return Err(DocumentError::Stale),
            WriteDecision::Conflict => return Err(DocumentError::Conflict),
            WriteDecision::Write => {}
        }
        let bytes = self.encoded_text()?;
        atomic_write(path, &bytes)?;
        self.fingerprint = Some(fingerprint(&bytes));
        self.revisions = self.revisions.mark_saved();
        Ok(())
    }

    pub fn save_as(&mut self, path: &Path) -> Result<(), DocumentError> {
        let path = destination(path)?;
        if self.path.as_ref() == Some(&path) {
            return self.save();
        }
        let bytes = self.encoded_text()?;
        atomic_write(&path, &bytes)?;
        self.path = Some(path);
        self.fingerprint = Some(fingerprint(&bytes));
        self.revisions = self.revisions.mark_saved();
        Ok(())
    }

    fn encoded_text(&self) -> Result<Vec<u8>, DocumentError> {
        match &self.original_bytes {
            Some(bytes) => Ok(bytes.clone()),
            None => self.format.encode(&self.text),
        }
    }

    pub(crate) fn recovery_record(&self) -> RecoveryRecord {
        RecoveryRecord {
            format: RECOVERY_FORMAT,
            id: self.id,
            key: Some(self.recovery_key),
            text: self.text.clone(),
            path: self.path.clone(),
            revision: self.revisions.current(),
            saved_revision: self.revisions.saved(),
            fingerprint: self.fingerprint,
            text_format: self.format,
            archive_base: self.archive_base.clone(),
        }
    }

    pub(crate) fn from_recovery(record: RecoveryRecord) -> Result<Self, DocumentError> {
        let revisions = admit_recovery(RecoveryFacts {
            format: record.format,
            revision: record.revision,
            saved_revision: record.saved_revision,
            text_bytes: record.text.len(),
            has_path: record.path.is_some(),
            has_fingerprint: record.fingerprint.is_some(),
            path_absolute: record.path.as_ref().is_some_and(|path| path.is_absolute()),
        })
        .ok_or(DocumentError::InvalidRecovery)?;
        let mut document = Self {
            id: record.id,
            token: Uuid::new_v4(),
            recovery_key: record.key.unwrap_or(record.id),
            text: record.text,
            path: record.path,
            revisions,
            fingerprint: record.fingerprint,
            original_bytes: None,
            format: record.text_format,
            archive_base: record.archive_base,
        };
        if let Some(path) = &document.path
            && let Ok(bytes) = read_bounded(path, MAX_FILE_BYTES)
            && let Ok(encoded) = document.encoded_text()
            && (bytes == encoded
                || (!document.has_unsaved_changes()
                    && Some(fingerprint(&bytes)) == document.fingerprint
                    && TextFormat::decode(&bytes).is_ok_and(|(text, _)| text == document.text)))
        {
            document.fingerprint = Some(fingerprint(&bytes));
            document.revisions = document.revisions.mark_saved();
            document.original_bytes = Some(bytes);
        }
        Ok(document)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct RecoveryRecord {
    #[serde(default)]
    pub(crate) key: Option<Uuid>,
    #[serde(default)]
    pub(crate) archive_base: Option<celestite_archive::ObjectId>,
    format: u16,
    pub(crate) id: Uuid,
    text: String,
    path: Option<PathBuf>,
    revision: u64,
    saved_revision: u64,
    fingerprint: Option<[u8; 32]>,
    text_format: TextFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalize_reference(source: &str) -> String {
        let mut characters = source.chars().peekable();
        let mut normalized = String::new();
        while let Some(character) = characters.next() {
            if character == '\r' {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                normalized.push('\n');
            } else {
                normalized.push(character);
            }
        }
        normalized
    }

    #[test]
    fn every_two_byte_input_obeys_utf8_admission_and_normalization()
    -> Result<(), Box<dyn std::error::Error>> {
        for word in u16::MIN..=u16::MAX {
            let bytes = word.to_be_bytes();
            let decoded = TextFormat::decode(&bytes);
            match std::str::from_utf8(&bytes) {
                Ok(source) => {
                    let (text, format) = decoded?;
                    assert_eq!(text, normalize_reference(source), "input {word:#x}");
                    assert_eq!(TextFormat::decode(&format.encode(&text)?)?.0, text);
                }
                Err(_) => assert!(matches!(decoded, Err(DocumentError::InvalidEncoding))),
            }
        }
        Ok(())
    }

    #[test]
    fn unicode_corpus_round_trips_across_boms_and_uniform_line_endings()
    -> Result<(), Box<dyn std::error::Error>> {
        const ALPHABET: [&str; 11] = [
            "a", "日", "🦀", "e\u{301}", "\t", "\n", "#", "*", "\0", "\u{200d}", "\u{feff}",
        ];
        for first in ALPHABET {
            for second in ALPHABET {
                for third in ALPHABET {
                    let text = format!("#{first}{second}{third}\n");
                    for ending in [LineEnding::Lf, LineEnding::CrLf, LineEnding::Cr] {
                        for bom in [false, true] {
                            let format = TextFormat { ending, bom };
                            let encoded = format.encode(&text)?;
                            let (decoded, detected) = TextFormat::decode(&encoded)?;
                            assert_eq!(decoded, text);
                            assert_eq!(detected.encode(&decoded)?, encoded);
                            assert_eq!(
                                Some(encoded.len()),
                                encoded_length(
                                    text.len(),
                                    text.bytes().filter(|byte| *byte == b'\n').count(),
                                    bom,
                                    matches!(ending, LineEnding::CrLf)
                                )
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
