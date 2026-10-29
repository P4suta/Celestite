use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use celestite_archive::{NoteId, note_title};
use serde::Serialize;

use crate::{Document, DocumentError, MAX_FILE_BYTES, document::read_bounded};

const MAX_CANDIDATES: usize = 50;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCandidate {
    pub id: NoteId,
    pub title: Option<String>,
    pub path: String,
}

#[derive(Clone, Debug)]
struct IndexedNote {
    id: NoteId,
    title: Option<String>,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Default)]
pub(crate) struct TitleIndex {
    notes: HashMap<PathBuf, IndexedNote>,
    pub(crate) warnings: Vec<String>,
}

fn stem_identity(path: &Path) -> Option<NoteId> {
    let stem = path.file_stem()?.to_str()?;
    let id: NoteId = stem.parse().ok()?;
    (id.to_string() == stem).then_some(id)
}

fn describe(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

impl TitleIndex {
    /// Records the title of a note the editor just wrote, without rereading it.
    pub(crate) fn observe(&mut self, path: &Path, document: &Document) {
        if let Some(id) = stem_identity(path)
            && id.uuid() == document.id()
        {
            let metadata = fs::metadata(path).ok();
            self.notes.insert(
                path.to_path_buf(),
                IndexedNote {
                    id,
                    title: note_title(&document.snapshot().text),
                    len: metadata.as_ref().map_or(0, fs::Metadata::len),
                    modified: metadata.and_then(|metadata| metadata.modified().ok()),
                },
            );
        }
    }

    /// Rescans the library root, rereading only notes whose size or mtime changed.
    pub(crate) fn refresh(&mut self, library: &Path) -> Result<(), DocumentError> {
        let mut seen = HashMap::new();
        let mut warnings = Vec::new();
        let entries = match fs::read_dir(library) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.notes.clear();
                self.warnings.clear();
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("md")
                || !entry.file_type()?.is_file()
            {
                continue;
            }
            let Some(id) = stem_identity(&path) else {
                warnings.push(format!(
                    "{} is not named by a lowercase note identifier and is not offered for links.",
                    describe(&path)
                ));
                continue;
            };
            let metadata = entry.metadata()?;
            let modified = metadata.modified().ok();
            let current = match self.notes.remove(&path) {
                Some(indexed) if indexed.len == metadata.len() && indexed.modified == modified => {
                    indexed
                }
                _ => {
                    let title = match read_bounded(&path, MAX_FILE_BYTES) {
                        Ok(bytes) => match std::str::from_utf8(&bytes) {
                            Ok(text) => {
                                match celestite_archive::identity(text) {
                                    Ok(Some(declared)) if declared != id => {
                                        warnings.push(format!(
                                            "{} declares a different identity in its front matter and is not offered for links.",
                                            describe(&path)
                                        ));
                                        continue;
                                    }
                                    Ok(_) => {}
                                    Err(error) => {
                                        warnings.push(format!("{}: {error}", describe(&path)));
                                        continue;
                                    }
                                }
                                note_title(text)
                            }
                            Err(_) => {
                                warnings.push(format!(
                                    "{} is not valid UTF-8 text and is not offered for links.",
                                    describe(&path)
                                ));
                                continue;
                            }
                        },
                        Err(error) => {
                            warnings.push(format!("{}: {error}", describe(&path)));
                            continue;
                        }
                    };
                    IndexedNote {
                        id,
                        title,
                        len: metadata.len(),
                        modified,
                    }
                }
            };
            seen.insert(path, current);
        }
        self.notes = seen;
        self.warnings = warnings;
        Ok(())
    }

    /// Ranks candidates: title prefix, then title substring, then identifier prefix.
    ///
    /// Ties keep the newest note first, which is the largest UUIDv7 identifier.
    pub(crate) fn candidates(&self, query: &str) -> Vec<LinkCandidate> {
        let query = query.trim().to_lowercase();
        let mut ranked: Vec<(u8, &PathBuf, &IndexedNote)> = self
            .notes
            .iter()
            .filter_map(|(path, note)| {
                let title = note.title.as_deref().unwrap_or_default().to_lowercase();
                let rank = if query.is_empty() || title.starts_with(&query) {
                    0
                } else if title.contains(&query) {
                    1
                } else if note.id.to_string().starts_with(&query) {
                    2
                } else {
                    return None;
                };
                Some((rank, path, note))
            })
            .collect();
        ranked.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| right.2.id.cmp(&left.2.id))
        });
        ranked
            .into_iter()
            .take(MAX_CANDIDATES)
            .map(|(_, path, note)| LinkCandidate {
                id: note.id,
                title: note.title.clone(),
                path: path.to_string_lossy().into_owned(),
            })
            .collect()
    }
}
