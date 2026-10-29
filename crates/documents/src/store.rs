use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    sync::Mutex,
};

use celestite_core::{AssetStore, library_write, media_link_allowed, plan_asset_store};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{
    Document, DocumentError, MAX_FILE_BYTES,
    document::{RecoveryRecord, atomic_write, read_bounded},
    library::{LinkCandidate, TitleIndex},
    navigation::asset_extension,
};

/// The bytes of a media file to store, either in memory or on disk.
#[derive(Clone, Copy, Debug)]
pub enum AssetSource<'a> {
    Bytes(&'a [u8]),
    File(&'a Path),
}

/// The library name of stored media: `assets/<digest>.<extension>`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetReceipt {
    pub digest: String,
    pub extension: String,
}

const MAX_JSON_STRING_BYTE_EXPANSION: usize = 6;
const RECOVERY_METADATA_RESERVE_BYTES: usize = 8192;
const MAX_RECOVERY_RECORD_BYTES: usize =
    MAX_FILE_BYTES * MAX_JSON_STRING_BYTE_EXPANSION + RECOVERY_METADATA_RESERVE_BYTES;
const MAX_PREFERENCES_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub appearance: Appearance,
    pub recent_files: Vec<PathBuf>,
    #[serde(default)]
    pub library: Option<PathBuf>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            appearance: Appearance::Light,
            recent_files: Vec::new(),
            library: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub documents: Vec<Document>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct NoteStore {
    root: PathBuf,
    _lease: File,
    archive: celestite_archive::Vault,
    archive_sessions: Mutex<HashMap<Uuid, Option<celestite_archive::ObjectId>>>,
    library: Option<PathBuf>,
    titles: Mutex<TitleIndex>,
}

impl NoteStore {
    pub fn new(root: PathBuf) -> Result<Self, DocumentError> {
        fs::create_dir_all(root.join("recovery"))?;
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".owner.lock"))?;
        lease.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => DocumentError::AlreadyRunning,
            TryLockError::Error(error) => DocumentError::Io(error),
        })?;
        Ok(Self {
            archive: celestite_archive::Vault::init(&root.join("archive"))?,
            archive_sessions: Mutex::new(HashMap::new()),
            root,
            _lease: lease,
            library: None,
            titles: Mutex::new(TitleIndex::default()),
        })
    }

    /// Selects the flat library folder that receives new notes and link candidates.
    ///
    /// The folder is created on first use; selecting `None` keeps the standalone model.
    pub fn with_library(mut self, library: Option<PathBuf>) -> Self {
        self.library = library;
        self.titles = Mutex::new(TitleIndex::default());
        self
    }

    pub fn library(&self) -> Option<&Path> {
        self.library.as_deref()
    }

    /// Returns the canonical library path when the folder exists.
    fn canonical_library(&self) -> Option<PathBuf> {
        self.library
            .as_deref()
            .and_then(|library| fs::canonicalize(library).ok())
    }

    fn ensure_library(&self) -> Result<Option<PathBuf>, DocumentError> {
        let Some(library) = self.library.as_deref() else {
            return Ok(None);
        };
        fs::create_dir_all(library.join(celestite_archive::ASSET_DIRECTORY))?;
        Ok(Some(fs::canonicalize(library)?))
    }

    /// Writes an untitled note with a nonempty body into the library as `<id>.md`.
    ///
    /// Returns whether the document received its library path during this call.
    pub fn place_in_library(&self, document: &mut Document) -> Result<bool, DocumentError> {
        if !library_write(
            document.path().is_some(),
            self.library.is_some(),
            document.body_is_empty(),
        ) {
            return Ok(false);
        }
        let library = self.ensure_library()?.ok_or(DocumentError::InvalidPath)?;
        document.save_as(&library.join(format!("{}.md", document.id())))?;
        Ok(true)
    }

    /// Lists library notes whose title or identifier matches the query, newest first.
    pub fn library_notes(&self, query: &str) -> Result<Vec<LinkCandidate>, DocumentError> {
        let Some(library) = self.canonical_library() else {
            return Ok(Vec::new());
        };
        let mut titles = self
            .titles
            .lock()
            .map_err(|_| DocumentError::InvalidRecovery)?;
        titles.refresh(&library)?;
        Ok(titles.candidates(query))
    }

    /// Reports whether a media link from this note can resolve: a library exists and the note has no path yet or sits inside it.
    pub fn may_link_media(&self, document: &Document) -> bool {
        let library = self.canonical_library();
        let in_library = match (document.path().and_then(Path::parent), library.as_deref()) {
            (Some(parent), Some(library)) => parent == library,
            _ => false,
        };
        media_link_allowed(
            self.library.is_some(),
            document.path().is_some(),
            in_library,
        )
    }

    /// Stores media under `assets/<digest>.<extension>` in the library, once per content.
    ///
    /// An identical existing file is kept, and a different file under the same digest name is refused.
    pub fn store_asset(
        &self,
        source: AssetSource<'_>,
        extension: &str,
    ) -> Result<AssetReceipt, DocumentError> {
        let extension = asset_extension(extension).ok_or(DocumentError::Link(
            "This file type is not supported as media.",
        ))?;
        let library = self
            .ensure_library()?
            .ok_or(DocumentError::Link("Media need a library folder."))?;
        let assets = library.join(celestite_archive::ASSET_DIRECTORY);
        let mut temporary = NamedTempFile::new_in(&assets)?;
        let digest = match source {
            AssetSource::Bytes(mut bytes) => {
                celestite_archive::copy_hashed(&mut bytes, temporary.as_file_mut())?
            }
            AssetSource::File(path) => {
                if !fs::symlink_metadata(path)?.is_file() {
                    return Err(DocumentError::InvalidPath);
                }
                celestite_archive::copy_hashed(&mut File::open(path)?, temporary.as_file_mut())?
            }
        };
        let target = assets.join(format!("{}.{extension}", digest.digest()));
        let exists = fs::symlink_metadata(&target).is_ok();
        let same_content = exists
            && celestite_archive::copy_hashed(&mut File::open(&target)?, &mut std::io::sink())?
                == digest;
        match plan_asset_store(exists, same_content) {
            AssetStore::Write => {
                temporary.as_file().sync_all()?;
                temporary
                    .persist_noclobber(&target)
                    .map_err(|error| error.error)?;
                File::open(&assets)?.sync_all()?;
            }
            AssetStore::Keep => {}
            AssetStore::Refuse => {
                return Err(DocumentError::Link(
                    "A different file already uses this content digest in the library.",
                ));
            }
        }
        Ok(AssetReceipt {
            digest: digest.digest().to_owned(),
            extension: extension.to_owned(),
        })
    }

    pub fn library_warnings(&self) -> Vec<String> {
        self.titles
            .lock()
            .map(|titles| titles.warnings.clone())
            .unwrap_or_default()
    }

    pub fn load_preferences(&self) -> Result<Preferences, DocumentError> {
        match read_bounded(&self.root.join("preferences.json"), MAX_PREFERENCES_BYTES) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(DocumentError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Preferences::default())
            }
            Err(error) => Err(error),
        }
    }

    pub fn save_preferences(&self, preferences: &Preferences) -> Result<(), DocumentError> {
        atomic_write(
            &self.root.join("preferences.json"),
            &crate::json_buffer::encode(preferences, MAX_PREFERENCES_BYTES)?,
        )
    }

    pub fn checkpoint(&self, document: &Document) -> Result<(), DocumentError> {
        let token = document
            .snapshot()
            .token
            .parse::<Uuid>()
            .map_err(|_| DocumentError::InvalidRecovery)?;
        let mut record = document.recovery_record();
        record.archive_base = self
            .archive_sessions
            .lock()
            .map_err(|_| DocumentError::InvalidRecovery)?
            .get(&token)
            .cloned()
            .unwrap_or_else(|| document.archive_base.clone());
        atomic_write(
            &self.checkpoint_path(document.checkpoint_key()),
            &crate::json_buffer::encode(&record, MAX_RECOVERY_RECORD_BYTES)?,
        )?;
        let library = self.canonical_library();
        let receipt = self.archive.capture_file(
            document.id().to_string().parse()?,
            document.path(),
            &document.archive_bytes()?,
            record.archive_base,
            library.as_deref(),
        )?;
        if let (Some(path), Some(library)) = (document.path(), library.as_deref())
            && path.parent() == Some(library)
            && let Ok(mut titles) = self.titles.lock()
        {
            titles.observe(path, document);
        }
        match self.capture_admission(document, &receipt)? {
            celestite_archive_core::CaptureAdmission::Confirmed => {}
            celestite_archive_core::CaptureAdmission::Conflicted => {
                return Err(celestite_archive::ArchiveError::Conflicts(vec![
                    document.id().to_string().parse()?,
                ])
                .into());
            }
            celestite_archive_core::CaptureAdmission::DifferentSource => {
                return Err(celestite_archive::ArchiveError::StaleHead.into());
            }
        }
        self.archive_sessions
            .lock()
            .map_err(|_| DocumentError::InvalidRecovery)?
            .insert(token, Some(receipt.snapshot.clone()));
        record.archive_base = Some(receipt.snapshot);
        atomic_write(
            &self.checkpoint_path(document.checkpoint_key()),
            &crate::json_buffer::encode(&record, MAX_RECOVERY_RECORD_BYTES)?,
        )?;
        Ok(())
    }

    fn capture_admission(
        &self,
        document: &Document,
        receipt: &celestite_archive::CommitReceipt,
    ) -> Result<celestite_archive_core::CaptureAdmission, DocumentError> {
        let id = document.id().to_string().parse()?;
        let bytes = document.archive_bytes()?;
        let source = celestite_archive::managed_source(
            id,
            std::str::from_utf8(&bytes).map_err(|_| DocumentError::InvalidEncoding)?,
        )?;
        let notes = self.archive.notes(&receipt.snapshot)?;
        let matches = match notes.get(&id) {
            Some(celestite_archive::Entry::Live { record }) => {
                self.archive.note(record)?.source
                    == celestite_archive::ObjectId::of(source.as_bytes())
            }
            Some(_) | None => false,
        };
        Ok(celestite_archive_core::admit_capture(
            matches,
            receipt.conflicts.contains(&id),
        ))
    }

    pub fn archive(&self) -> &celestite_archive::Vault {
        &self.archive
    }

    pub fn bind_document(&self, document: &mut Document) -> Result<(), DocumentError> {
        let current = self.archive.current()?;
        let notes = match &current {
            Some(current) => self.archive.notes(current)?,
            None => Default::default(),
        };
        if let Some(path) = document.path()
            && celestite_archive::identity(&document.snapshot().text)?.is_none()
        {
            let origin = path.to_string_lossy();
            let mut matches = std::collections::BTreeSet::new();
            for (id, entry) in &notes {
                for record in entry.alternatives().into_iter().flatten() {
                    if self.archive.note(&record)?.origin.as_deref() == Some(origin.as_ref()) {
                        matches.insert(*id);
                    }
                }
            }
            if matches.len() > 1 {
                return Err(celestite_archive::ArchiveError::Invalid(String::from(
                    "Several note identities claim this source location",
                ))
                .into());
            }
            if let Some(id) = matches.into_iter().next() {
                document.adopt_id(id);
            }
        }
        let id = document
            .id()
            .to_string()
            .parse::<celestite_archive::NoteId>()?;
        let mut base = if let Some(base) = &document.archive_base {
            self.archive.verify(base)?;
            Some(base.clone())
        } else {
            match notes.get(&id) {
                None => current,
                Some(celestite_archive::Entry::Live { record })
                    if self.archive.note(record)?.original
                        == celestite_archive::ObjectId::of(&document.archive_bytes()?) =>
                {
                    current
                }
                Some(_) => None,
            }
        };
        if document.path().is_some()
            || document.archive_base.is_some()
            || document.has_unsaved_changes()
            || !document.snapshot().text.is_empty()
        {
            let receipt = self.archive.capture_file(
                id,
                document.path(),
                &document.archive_bytes()?,
                base.clone(),
                self.canonical_library().as_deref(),
            )?;
            if self.capture_admission(document, &receipt)?
                == celestite_archive_core::CaptureAdmission::Confirmed
            {
                base = Some(receipt.snapshot);
            }
        }
        self.archive_sessions
            .lock()
            .map_err(|_| DocumentError::InvalidRecovery)?
            .insert(
                document
                    .snapshot()
                    .token
                    .parse::<Uuid>()
                    .map_err(|_| DocumentError::InvalidRecovery)?,
                base,
            );
        Ok(())
    }

    fn checkpoint_path(&self, id: Uuid) -> PathBuf {
        self.root.join("recovery").join(format!("{id}.json"))
    }

    pub fn remove_checkpoint(&self, id: Uuid) -> Result<(), DocumentError> {
        match fs::remove_file(self.checkpoint_path(id)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn recover(&self) -> Result<RecoveryReport, DocumentError> {
        let mut report = RecoveryReport::default();
        let mut paths = fs::read_dir(self.root.join("recovery"))?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        for path in paths {
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            match self.restore_record(&path) {
                Ok(document) => report.documents.push(document),
                Err(error) => report.warnings.push(format!(
                    "Could not restore {}: {error}",
                    path.file_name().map_or_else(
                        || String::from("a recovery record"),
                        |s| s.to_string_lossy().into_owned()
                    )
                )),
            }
        }
        Ok(report)
    }

    fn restore_record(&self, path: &std::path::Path) -> Result<Document, DocumentError> {
        let bytes = read_bounded(path, MAX_RECOVERY_RECORD_BYTES)?;
        let record: RecoveryRecord = serde_json::from_slice(&bytes)?;
        if self.checkpoint_path(record.key.unwrap_or(record.id)) != path {
            return Err(DocumentError::InvalidRecovery);
        }
        if let Some(base) = &record.archive_base {
            self.archive.verify(base)?;
            if !self
                .archive
                .notes(base)?
                .contains_key(&record.id.to_string().parse()?)
            {
                return Err(DocumentError::InvalidRecovery);
            }
        }
        Document::from_recovery(record)
    }
}
