use crate::model::{
    ArchiveError, COPY_BUFFER_BYTES, CommitReceipt, Entry, FORMAT_NAME, FORMAT_VERSION, Format,
    MAX_ASSET_BYTES, MAX_GRAPH_OBJECTS, MAX_RECORD_BYTES, NoteId, NoteRecord, ObjectId,
    PublicationStage, Record, SHARD_PREFIX_LENGTH, Verification,
};
use celestite_archive_core::{MergeChoice, Publication, merge_choice};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions, TryLockError},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

const FORMAT_DOCUMENT: &str = include_str!("../../../docs/storage-format.md");
const LOCAL_DEVICE: &str = "local/device";

#[derive(Debug)]
pub struct Vault {
    pub(crate) root: PathBuf,
    validated: Mutex<BTreeSet<ObjectId>>,
}

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, ArchiveError> {
    if !fs::symlink_metadata(path)?.is_file() {
        return invalid("Expected a regular file");
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(ArchiveError::TooLarge);
    }
    Ok(bytes)
}
pub(crate) fn invalid<T>(message: &str) -> Result<T, ArchiveError> {
    Err(ArchiveError::Invalid(message.to_owned()))
}
pub(crate) fn conflict_ids(notes: &BTreeMap<NoteId, Entry>) -> Vec<NoteId> {
    notes
        .iter()
        .filter_map(|(id, entry)| matches!(entry, Entry::Conflict { .. }).then_some(*id))
        .collect()
}
pub(crate) fn sync_directory(path: &Path) -> Result<(), ArchiveError> {
    File::open(path)?.sync_all()?;
    Ok(())
}
pub(crate) fn replace(path: &Path, bytes: &[u8]) -> Result<(), ArchiveError> {
    let parent = path
        .parent()
        .ok_or_else(|| ArchiveError::Invalid(String::from("No parent directory")))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| ArchiveError::Io(error.error))?;
    sync_directory(parent)
}
pub(crate) fn relative_path(path: &Path) -> Result<String, ArchiveError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| ArchiveError::Invalid(String::from("Paths must be UTF-8")))?;
                if part.contains(['\\', '\0', ':', '\r', '\n']) {
                    return invalid("Unsafe logical path");
                }
                parts.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return invalid("Path escapes its root");
                }
            }
            Component::Prefix(_) | Component::RootDir => {
                return invalid("Expected a relative path");
            }
        }
    }
    if parts.is_empty() {
        return invalid("Empty logical path");
    }
    Ok(parts.join("/"))
}

impl Vault {
    pub fn init(path: &Path) -> Result<Self, ArchiveError> {
        let existing = fs::canonicalize(
            path.ancestors()
                .find(|ancestor| ancestor.exists())
                .unwrap_or_else(|| Path::new(".")),
        )?;
        fs::create_dir_all(path)?;
        let descriptor = path.join("format.json");
        if !descriptor.exists() {
            if fs::read_dir(path)?.next().is_some() {
                return invalid("Refusing a nonempty directory");
            }
            fs::create_dir(path.join("objects"))?;
            fs::create_dir(path.join("refs"))?;
            replace(&path.join("FORMAT.md"), FORMAT_DOCUMENT.as_bytes())?;
            replace(
                &descriptor,
                &serde_json::to_vec_pretty(&Format {
                    format: FORMAT_NAME.to_owned(),
                    version: FORMAT_VERSION,
                })?,
            )?;
        }
        let vault = Self::open(path)?;
        for directory in vault.root.ancestors() {
            sync_directory(directory)?;
            if directory == existing {
                break;
            }
        }
        Ok(vault)
    }
    pub fn open(path: &Path) -> Result<Self, ArchiveError> {
        let format: Format =
            serde_json::from_slice(&read_bounded(&path.join("format.json"), MAX_RECORD_BYTES)?)?;
        if format.format != FORMAT_NAME {
            return invalid("Unknown archive identifier");
        }
        if format.version != FORMAT_VERSION {
            return Err(ArchiveError::Unsupported(format.version));
        }
        Ok(Self {
            root: fs::canonicalize(path)?,
            validated: Mutex::new(BTreeSet::new()),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn object_path(&self, id: &ObjectId) -> PathBuf {
        self.root
            .join("objects/sha256")
            .join(&id.digest()[..SHARD_PREFIX_LENGTH])
            .join(id.digest())
    }
    pub(crate) fn write_lock(&self) -> Result<File, ArchiveError> {
        let directory = self.root.join("local");
        fs::create_dir_all(&directory)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("writer.lock"))?;
        lock.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => ArchiveError::Locked,
            TryLockError::Error(error) => ArchiveError::Io(error),
        })?;
        Ok(lock)
    }
    fn device(&self) -> Result<NoteId, ArchiveError> {
        let path = self.root.join(LOCAL_DEVICE);
        match fs::read_to_string(&path) {
            Ok(value) => value.trim().parse(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let id = NoteId::new();
                replace(&path, id.to_string().as_bytes())?;
                Ok(id)
            }
            Err(error) => Err(error.into()),
        }
    }
    pub fn heads(&self) -> Result<Vec<ObjectId>, ArchiveError> {
        let mut roots = BTreeSet::new();
        for entry in fs::read_dir(self.root.join("refs"))? {
            let entry = entry?;
            let _device: NoteId = entry
                .file_name()
                .to_str()
                .ok_or_else(|| ArchiveError::Invalid(String::from("Invalid device reference")))?
                .parse()?;
            roots.insert(
                std::str::from_utf8(&read_bounded(&entry.path(), MAX_RECORD_BYTES)?)?
                    .trim()
                    .parse()?,
            );
        }
        Ok(roots.into_iter().collect())
    }
    pub fn current(&self) -> Result<Option<ObjectId>, ArchiveError> {
        match fs::read_to_string(self.root.join(LOCAL_DEVICE)) {
            Ok(value) => {
                let device: NoteId = value.trim().parse()?;
                match fs::read_to_string(self.root.join("refs").join(device.to_string())) {
                    Ok(value) => return Ok(Some(value.trim().parse()?)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let roots = self.heads()?;
        let mut maximal: BTreeSet<_> = roots.iter().cloned().collect();
        if roots.len() > 1 {
            for root in &roots {
                for ancestor in self.ancestors(root)? {
                    if ancestor != *root {
                        maximal.remove(&ancestor);
                    }
                }
            }
        }
        match maximal.into_iter().collect::<Vec<_>>().as_slice() {
            [] => Ok(None),
            [only] => Ok(Some(only.clone())),
            _ => invalid("Select or merge multiple device roots"),
        }
    }
    pub(crate) fn put_bytes(&self, bytes: &[u8]) -> Result<ObjectId, ArchiveError> {
        let id = ObjectId::of(bytes);
        let path = self.object_path(&id);
        if path.exists() {
            if self.read_bytes(&id, bytes.len())? != bytes {
                return invalid("Object collision");
            }
        } else {
            replace(&path, bytes)?;
            sync_directory(&self.root.join("objects/sha256"))?;
            sync_directory(&self.root.join("objects"))?;
        }
        Ok(id)
    }
    pub(crate) fn put_file(&self, path: &Path) -> Result<ObjectId, ArchiveError> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.len() > MAX_ASSET_BYTES {
            return Err(ArchiveError::TooLarge);
        }
        let staging = self.root.join("local");
        fs::create_dir_all(&staging)?;
        let mut temporary = NamedTempFile::new_in(&staging)?;
        let mut input = File::open(path)?.take(MAX_ASSET_BYTES + 1);
        let id = copy_hashed(&mut input, &mut temporary)?;
        temporary.as_file().sync_all()?;
        let destination = self.object_path(&id);
        if destination.exists() {
            self.verify_blob(&id)?;
        } else {
            let parent = destination
                .parent()
                .ok_or_else(|| ArchiveError::Invalid(String::from("No object directory")))?;
            fs::create_dir_all(parent)?;
            temporary
                .persist_noclobber(&destination)
                .map_err(|error| ArchiveError::Io(error.error))?;
            sync_directory(parent)?;
            sync_directory(&self.root.join("objects/sha256"))?;
            sync_directory(&self.root.join("objects"))?;
        }
        Ok(id)
    }
    pub fn read_bytes(&self, id: &ObjectId, limit: usize) -> Result<Vec<u8>, ArchiveError> {
        let bytes = read_bounded(&self.object_path(id), limit)?;
        if ObjectId::of(&bytes) != *id {
            return invalid(&format!("Object digest mismatch: {id}"));
        }
        Ok(bytes)
    }
    pub(crate) fn verify_blob(&self, id: &ObjectId) -> Result<(), ArchiveError> {
        let path = self.object_path(id);
        if !fs::symlink_metadata(&path)?.is_file() {
            return invalid("Object is not a regular file");
        }
        let actual = copy_hashed(
            &mut File::open(path)?.take(MAX_ASSET_BYTES + 1),
            &mut std::io::sink(),
        )?;
        if actual != *id {
            return invalid(&format!("Object digest mismatch: {id}"));
        }
        Ok(())
    }
    pub(crate) fn put_record(&self, record: &Record) -> Result<ObjectId, ArchiveError> {
        let bytes = serde_json::to_vec(record)?;
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(ArchiveError::TooLarge);
        }
        self.put_bytes(&bytes)
    }
    pub fn record(&self, id: &ObjectId) -> Result<Record, ArchiveError> {
        Ok(serde_json::from_slice(
            &self.read_bytes(id, MAX_RECORD_BYTES)?,
        )?)
    }
    pub fn notes(&self, snapshot: &ObjectId) -> Result<BTreeMap<NoteId, Entry>, ArchiveError> {
        let Record::Snapshot { tree, .. } = self.record(snapshot)? else {
            return invalid("Expected a snapshot");
        };
        let Record::Tree { shards } = self.record(&tree)? else {
            return invalid("Expected a snapshot tree");
        };
        let mut result = BTreeMap::new();
        for (prefix, object) in shards {
            let Record::Shard { notes } = self.record(&object)? else {
                return invalid("Expected a note shard");
            };
            for (id, entry) in notes {
                if id.shard() != prefix || result.insert(id, entry).is_some() {
                    return invalid("Invalid note shard identity");
                }
            }
        }
        Ok(result)
    }
    pub fn note(&self, id: &ObjectId) -> Result<NoteRecord, ArchiveError> {
        match self.record(id)? {
            Record::Note { note } => Ok(note),
            _ => invalid("Expected a note record"),
        }
    }
    fn write_tree(&self, notes: BTreeMap<NoteId, Entry>) -> Result<ObjectId, ArchiveError> {
        let mut groups: BTreeMap<String, BTreeMap<NoteId, Entry>> = BTreeMap::new();
        for (id, entry) in notes {
            groups.entry(id.shard()).or_default().insert(id, entry);
        }
        let mut shards = BTreeMap::new();
        for (prefix, notes) in groups {
            shards.insert(prefix, self.put_record(&Record::Shard { notes })?);
        }
        self.put_record(&Record::Tree { shards })
    }
    pub(crate) fn publish(
        &self,
        parents: Vec<ObjectId>,
        notes: BTreeMap<NoteId, Entry>,
        expected: Option<ObjectId>,
        fault: &mut impl FnMut(PublicationStage) -> Result<(), ArchiveError>,
    ) -> Result<CommitReceipt, ArchiveError> {
        for parent in &parents {
            self.verify_graph(parent, true)?;
        }
        let tree = self.write_tree(notes.clone())?;
        let mut phase = Publication::Preparing.advance(true);
        fault(PublicationStage::ObjectsSynced)?;
        let device = self.device()?;
        let captured_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| ArchiveError::Invalid(error.to_string()))?
            .as_millis()
            .to_string();
        let snapshot = self.put_record(&Record::Snapshot {
            parents,
            tree,
            device,
            captured_at,
        })?;
        let fresh = !self
            .validated
            .lock()
            .map_err(|_| ArchiveError::Invalid(String::from("Verification cache is unavailable")))?
            .contains(&snapshot);
        self.verify_graph(&snapshot, true)?;
        if !celestite_archive_core::admit_extension(fresh, true, true) {
            return invalid("Invalid graph extension");
        }
        phase = phase.advance(true);
        fault(PublicationStage::SnapshotSynced)?;
        if !phase.may_publish(self.current()? == expected, true) {
            return Err(ArchiveError::StaleHead);
        }
        let parent = self.root.join("refs");
        let mut temporary = NamedTempFile::new_in(&parent)?;
        temporary.write_all(snapshot.to_string().as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(parent.join(device.to_string()))
            .map_err(|error| ArchiveError::Io(error.error))?;
        fault(PublicationStage::ReferenceReplaced)?;
        sync_directory(&parent)?;
        fault(PublicationStage::ReferenceSynced)?;
        phase = phase.advance(true);
        if phase != Publication::Published {
            return invalid("The publication did not reach its committed state");
        }
        let conflicts = conflict_ids(&notes);
        Ok(CommitReceipt {
            snapshot,
            conflicts,
            local_committed: true,
        })
    }
    pub fn commit_notes(
        &self,
        base: Option<ObjectId>,
        changes: BTreeMap<NoteId, Entry>,
    ) -> Result<CommitReceipt, ArchiveError> {
        self.commit_with_fault(base, changes, &mut |_| Ok(()))
    }
    pub fn commit_with_fault(
        &self,
        base: Option<ObjectId>,
        changes: BTreeMap<NoteId, Entry>,
        fault: &mut impl FnMut(PublicationStage) -> Result<(), ArchiveError>,
    ) -> Result<CommitReceipt, ArchiveError> {
        let _lock = self.write_lock()?;
        let current = self.current()?;
        let ancestor = match &base {
            Some(id) => self.notes(id)?,
            None => BTreeMap::new(),
        };
        let mut notes = ancestor.clone();
        notes.extend(changes);
        let mut parents = base.iter().cloned().collect::<Vec<_>>();
        if current != base
            && let Some(current) = &current
        {
            notes = merge_maps(&ancestor, &self.notes(current)?, &notes);
            parents.push(current.clone());
        }
        parents.sort();
        parents.dedup();
        self.publish(parents, notes, current, fault)
    }
    pub fn verify(&self, root: &ObjectId) -> Result<Verification, ArchiveError> {
        if !matches!(self.record(root)?, Record::Snapshot { .. }) {
            return invalid("The archive root must be a snapshot");
        }
        self.verify_graph(root, false)
    }

    fn verify_graph(
        &self,
        root: &ObjectId,
        incremental: bool,
    ) -> Result<Verification, ArchiveError> {
        let reusable = if incremental {
            self.validated
                .lock()
                .map_err(|_| {
                    ArchiveError::Invalid(String::from("Verification cache is unavailable"))
                })?
                .clone()
        } else {
            BTreeSet::new()
        };
        let mut report = Verification::default();
        let mut pending = vec![(root.clone(), false)];
        let mut visiting = BTreeSet::new();
        let mut complete = BTreeSet::new();
        while let Some((id, leaving)) = pending.pop() {
            if leaving {
                visiting.remove(&id);
                complete.insert(id);
                continue;
            }
            if complete.contains(&id) {
                continue;
            }
            if reusable.contains(&id) {
                self.record(&id)?;
                complete.insert(id);
                continue;
            }
            if !visiting.insert(id.clone()) {
                return invalid("Cyclic object graph");
            }
            if visiting.len() + complete.len() > MAX_GRAPH_OBJECTS {
                return Err(ArchiveError::TooLarge);
            }
            let record = self.record(&id)?;
            pending.push((id, true));
            report.objects += 1;
            match record {
                Record::Snapshot {
                    parents,
                    tree,
                    captured_at,
                    ..
                } => {
                    report.snapshots += 1;
                    if captured_at.is_empty()
                        || !captured_at.bytes().all(|byte| byte.is_ascii_digit())
                    {
                        return invalid("Capture time must be decimal milliseconds");
                    }
                    if !matches!(self.record(&tree)?, Record::Tree { .. }) {
                        return invalid("Snapshot tree type mismatch");
                    }
                    pending.push((tree, false));
                    for parent in parents {
                        if !matches!(self.record(&parent)?, Record::Snapshot { .. }) {
                            return invalid("Snapshot parent type mismatch");
                        }
                        pending.push((parent, false));
                    }
                }
                Record::Tree { shards } => {
                    for (prefix, shard) in shards {
                        let Record::Shard { notes } = self.record(&shard)? else {
                            return invalid("Shard type mismatch");
                        };
                        if prefix.len() != SHARD_PREFIX_LENGTH
                            || !prefix
                                .bytes()
                                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                            || !notes.keys().all(|id| id.shard() == prefix)
                        {
                            return invalid("Shard prefix mismatch");
                        }
                        pending.push((shard, false));
                    }
                }
                Record::Shard { notes } => {
                    for (note_id, entry) in notes {
                        let mut unique = BTreeSet::new();
                        for note in entry.alternatives() {
                            if !unique.insert(note.clone()) {
                                return invalid("Duplicate conflict alternative");
                            }
                            if let Some(note) = note {
                                if self.note(&note)?.id != note_id {
                                    return invalid("Note identity mismatch");
                                }
                                pending.push((note, false));
                            }
                        }
                        if matches!(entry, Entry::Conflict { .. }) && unique.len() < 2 {
                            return invalid("Empty conflict");
                        }
                    }
                }
                Record::Note { note } => {
                    report.notes += 1;
                    if relative_path(Path::new(&note.path))? != note.path {
                        return invalid("Noncanonical note path");
                    }
                    let source = self.read_bytes(&note.source, celestite_core::MAX_FILE_BYTES)?;
                    let text = std::str::from_utf8(&source)?;
                    if crate::identity(text)? != Some(note.id) {
                        return invalid("Source note identity mismatch");
                    }
                    self.verify_blob(&note.original)?;
                    let mut previous_end = 0;
                    for link in note.links {
                        if !celestite_archive_core::admit_repair(
                            true,
                            source.len(),
                            link.start,
                            link.end,
                            true,
                        ) || link.start < previous_end
                            || !text.is_char_boundary(link.start)
                            || !text.is_char_boundary(link.end)
                        {
                            return invalid("Invalid link source span");
                        }
                        previous_end = link.end;
                        if matches!(
                            link.target,
                            crate::LinkTarget::Note { .. } | crate::LinkTarget::File { .. }
                        ) && text.get(link.start..link.end) != Some(link.destination.as_str())
                        {
                            return invalid("Link destination does not match its source bytes");
                        }
                        match link.target {
                            crate::LinkTarget::File { path, object } => {
                                relative_path(Path::new(&path))?;
                                self.verify_blob(&object)?;
                            }
                            crate::LinkTarget::Unresolved { .. }
                            | crate::LinkTarget::Ambiguous { .. } => report.unresolved_links += 1,
                            crate::LinkTarget::Note { .. } | crate::LinkTarget::External => {}
                        }
                    }
                }
            }
        }
        self.validated
            .lock()
            .map_err(|_| ArchiveError::Invalid(String::from("Verification cache is unavailable")))?
            .extend(complete);
        Ok(report)
    }
}

pub(crate) fn copy_hashed(
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<ObjectId, ArchiveError> {
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    let mut total = 0_u64;
    loop {
        let length = input.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        total += length as u64;
        if total > MAX_ASSET_BYTES {
            return Err(ArchiveError::TooLarge);
        }
        hash.update(&buffer[..length]);
        output.write_all(&buffer[..length])?;
    }
    Ok(ObjectId::from_digest(hash.finalize()))
}
pub(crate) fn merge_maps(
    base: &BTreeMap<NoteId, Entry>,
    left: &BTreeMap<NoteId, Entry>,
    right: &BTreeMap<NoteId, Entry>,
) -> BTreeMap<NoteId, Entry> {
    let keys: BTreeSet<_> = base
        .keys()
        .chain(left.keys())
        .chain(right.keys())
        .copied()
        .collect();
    keys.into_iter()
        .filter_map(|key| {
            let before = base.get(&key);
            let a = left.get(&key);
            let b = right.get(&key);
            let value = match merge_choice(a == b, a == before, b == before) {
                MergeChoice::Shared | MergeChoice::Left => a.cloned(),
                MergeChoice::Right => b.cloned(),
                MergeChoice::Conflict => Some(Entry::Conflict {
                    alternatives: a
                        .into_iter()
                        .chain(b)
                        .flat_map(Entry::alternatives)
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                }),
            };
            value.map(|value| (key, value))
        })
        .collect()
}
