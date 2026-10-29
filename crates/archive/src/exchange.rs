use crate::model::{MAX_GRAPH_OBJECTS, MAX_RECORD_BYTES};
use crate::storage::{copy_hashed, invalid, read_bounded, relative_path, replace, sync_directory};
use crate::{ArchiveError, Entry, LinkTarget, NoteId, ObjectId, Record, Vault};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    path::Path,
};
use tempfile::NamedTempFile;

const BAG_DECLARATION: &str = "BagIt-Version: 1.0\nTag-File-Character-Encoding: UTF-8\n";
const BAG_FILE: &str = "bagit.txt";
const PAYLOAD_MANIFEST: &str = "manifest-sha256.txt";
const TAG_MANIFEST: &str = "tagmanifest-sha256.txt";
const MAX_EXTENSION_LENGTH: usize = 16;

fn fresh_destination(destination: &Path, source: &Path) -> Result<(), ArchiveError> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .ok_or_else(|| ArchiveError::Invalid(String::from("Invalid output directory")))?;
    let resolved = fs::canonicalize(parent)?.join(name);
    let source = fs::canonicalize(source)?;
    if resolved.starts_with(&source) || source.starts_with(&resolved) {
        return invalid("Input and output directories must not overlap");
    }
    fs::create_dir(destination)?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct CheckoutReport {
    pub snapshot: ObjectId,
    pub notes: usize,
    pub media: usize,
    pub unresolved: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub id: NoteId,
    pub revision: ObjectId,
    pub path: String,
}

fn files(root: &Path) -> Result<BTreeSet<String>, ArchiveError> {
    let mut pending = vec![root.to_path_buf()];
    let mut paths = BTreeSet::new();
    while let Some(directory) = pending.pop() {
        for item in fs::read_dir(directory)? {
            let item = item?;
            let kind = item.file_type()?;
            if kind.is_dir() {
                pending.push(item.path());
            } else if kind.is_file() {
                paths.insert(relative_path(
                    item.path()
                        .strip_prefix(root)
                        .map_err(|error| ArchiveError::Invalid(error.to_string()))?,
                )?);
            } else {
                return invalid("Interchange contains a nonregular file");
            }
            if paths.len() + pending.len() > MAX_GRAPH_OBJECTS {
                return Err(ArchiveError::TooLarge);
            }
        }
    }
    Ok(paths)
}
fn copy_verified(input: &Path, output: &Path, expected: &ObjectId) -> Result<(), ArchiveError> {
    if !fs::symlink_metadata(input)?.is_file() {
        return invalid("Expected a regular payload file");
    }
    let parent = output
        .parent()
        .ok_or_else(|| ArchiveError::Invalid(String::from("No output directory")))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    let actual = copy_hashed(&mut File::open(input)?, &mut temporary)?;
    if &actual != expected {
        return invalid("Payload checksum mismatch");
    }
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(output)
        .map_err(|error| ArchiveError::Io(error.error))?;
    sync_directory(parent)
}
fn hash_file(path: &Path) -> Result<ObjectId, ArchiveError> {
    if !fs::symlink_metadata(path)?.is_file() {
        return invalid("Expected a regular manifest member");
    }
    copy_hashed(&mut File::open(path)?, &mut std::io::sink())
}
fn manifest(entries: &BTreeMap<String, ObjectId>) -> String {
    entries
        .iter()
        .map(|(path, id)| format!("{}  {path}\n", id.digest()))
        .collect()
}
fn read_manifest(path: &Path) -> Result<BTreeMap<String, ObjectId>, ArchiveError> {
    let bytes = read_bounded(path, MAX_RECORD_BYTES)?;
    let text = std::str::from_utf8(&bytes)?;
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let (digest, path) = line
            .split_once(char::is_whitespace)
            .ok_or_else(|| ArchiveError::Invalid(String::from("Invalid checksum manifest line")))?;
        let path = path.trim_start();
        if relative_path(Path::new(path))? != path
            || result
                .insert(path.to_owned(), format!("sha256:{digest}").parse()?)
                .is_some()
        {
            return invalid("Duplicate or noncanonical manifest path");
        }
    }
    Ok(result)
}

impl Vault {
    pub fn resolve(
        &self,
        id: NoteId,
        selected: Option<ObjectId>,
    ) -> Result<crate::CommitReceipt, ArchiveError> {
        let current = self
            .current()?
            .ok_or_else(|| ArchiveError::Invalid(String::from("The archive is empty")))?;
        let notes = self.notes(&current)?;
        let Some(Entry::Conflict { alternatives }) = notes.get(&id) else {
            return invalid("The note has no unresolved conflict");
        };
        if !alternatives.contains(&selected) {
            return invalid("The selection is not a conflict alternative");
        }
        let entry = match selected {
            Some(record) => Entry::Live { record },
            None => Entry::Deleted,
        };
        self.commit_notes(Some(current), BTreeMap::from([(id, entry)]))
    }
    pub(crate) fn ancestors(&self, root: &ObjectId) -> Result<BTreeSet<ObjectId>, ArchiveError> {
        let mut found = BTreeSet::new();
        let mut pending = vec![root.clone()];
        while let Some(id) = pending.pop() {
            if !found.insert(id.clone()) {
                continue;
            }
            if found.len() > MAX_GRAPH_OBJECTS {
                return Err(ArchiveError::TooLarge);
            }
            let Record::Snapshot { parents, .. } = self.record(&id)? else {
                return invalid("Expected an ancestor snapshot");
            };
            pending.extend(parents);
        }
        Ok(found)
    }

    pub fn merge_snapshot(
        &self,
        incoming: &ObjectId,
    ) -> Result<crate::CommitReceipt, ArchiveError> {
        let _lock = self.write_lock()?;
        self.verify(incoming)?;
        let current = self.current()?;
        let theirs = self.notes(incoming)?;
        let (parents, merged) = match &current {
            None => (vec![incoming.clone()], theirs),
            Some(current) => {
                let left_ancestors = self.ancestors(current)?;
                if left_ancestors.contains(incoming) {
                    return Ok(crate::CommitReceipt {
                        snapshot: current.clone(),
                        conflicts: crate::storage::conflict_ids(&self.notes(current)?),
                        local_committed: true,
                    });
                }
                let right_ancestors = self.ancestors(incoming)?;
                let common: BTreeSet<_> = left_ancestors
                    .intersection(&right_ancestors)
                    .cloned()
                    .collect();
                let mut maximal = common.clone();
                for candidate in &common {
                    for older in self.ancestors(candidate)? {
                        if older != *candidate {
                            maximal.remove(&older);
                        }
                    }
                }
                let base = if maximal.len() == 1 {
                    self.notes(maximal.iter().next().ok_or_else(|| {
                        ArchiveError::Invalid(String::from("Missing merge base"))
                    })?)?
                } else {
                    BTreeMap::new()
                };
                let merged = crate::storage::merge_maps(&base, &self.notes(current)?, &theirs);
                (vec![current.clone(), incoming.clone()], merged)
            }
        };
        self.publish(parents, merged, current, &mut |_| Ok(()))
    }

    pub fn ingest_bag(&self, bag: &Path) -> Result<Vec<crate::CommitReceipt>, ArchiveError> {
        Self::verify_bag(bag)?;
        let incoming = Self::open(&bag.join("data"))?;
        let roots = incoming.heads()?;
        for object in incoming.reachable(&roots)? {
            let destination = self.object_path(&object);
            if destination.exists() {
                self.verify_blob(&object)?;
            } else {
                copy_verified(&incoming.object_path(&object), &destination, &object)?;
            }
        }
        roots.iter().map(|root| self.merge_snapshot(root)).collect()
    }
    pub(crate) fn reachable(&self, roots: &[ObjectId]) -> Result<BTreeSet<ObjectId>, ArchiveError> {
        let mut result = BTreeSet::new();
        let mut pending = roots.to_vec();
        for root in roots {
            self.verify(root)?;
        }
        while let Some(id) = pending.pop() {
            if !result.insert(id.clone()) {
                continue;
            }
            if result.len() > MAX_GRAPH_OBJECTS {
                return Err(ArchiveError::TooLarge);
            }
            match self.record(&id)? {
                Record::Snapshot { parents, tree, .. } => {
                    pending.extend(parents);
                    pending.push(tree);
                }
                Record::Tree { shards } => pending.extend(shards.into_values()),
                Record::Shard { notes } => {
                    for entry in notes.into_values() {
                        pending.extend(entry.alternatives().into_iter().flatten());
                    }
                }
                Record::Note { note } => {
                    result.insert(note.source);
                    result.insert(note.original);
                    for link in note.links {
                        if let LinkTarget::File { object, .. } = link.target {
                            result.insert(object);
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    pub fn export_bag(&self, destination: &Path) -> Result<(), ArchiveError> {
        let _lock = self.write_lock()?;
        let roots = self.heads()?;
        let objects = self.reachable(&roots)?;
        fresh_destination(destination, &self.root)?;
        let data = destination.join("data");
        fs::create_dir(&data)?;
        let mut payload = BTreeMap::new();
        for name in ["format.json", "FORMAT.md"] {
            let input = self.root.join(name);
            let id = hash_file(&input)?;
            copy_verified(&input, &data.join(name), &id)?;
            payload.insert(format!("data/{name}"), id);
        }
        for object in objects {
            let input = self.object_path(&object);
            let path = relative_path(
                input
                    .strip_prefix(&self.root)
                    .map_err(|error| ArchiveError::Invalid(error.to_string()))?,
            )?;
            copy_verified(&input, &data.join(&path), &object)?;
            payload.insert(format!("data/{path}"), object);
        }
        fs::create_dir_all(data.join("refs"))?;
        for entry in fs::read_dir(self.root.join("refs"))? {
            let entry = entry?;
            let device: NoteId = entry
                .file_name()
                .to_str()
                .ok_or_else(|| ArchiveError::Invalid(String::from("Invalid device name")))?
                .parse()?;
            let bytes = read_bounded(&entry.path(), MAX_RECORD_BYTES)?;
            let root: ObjectId = std::str::from_utf8(&bytes)?.trim().parse()?;
            if !roots.contains(&root) {
                return invalid("Reference changed during export");
            }
            let path = format!("refs/{device}");
            replace(&data.join(&path), &bytes)?;
            payload.insert(format!("data/{path}"), ObjectId::of(&bytes));
        }
        replace(&destination.join(BAG_FILE), BAG_DECLARATION.as_bytes())?;
        replace(
            &destination.join(PAYLOAD_MANIFEST),
            manifest(&payload).as_bytes(),
        )?;
        let tags = BTreeMap::from([
            (BAG_FILE.to_owned(), hash_file(&destination.join(BAG_FILE))?),
            (
                PAYLOAD_MANIFEST.to_owned(),
                hash_file(&destination.join(PAYLOAD_MANIFEST))?,
            ),
        ]);
        replace(&destination.join(TAG_MANIFEST), manifest(&tags).as_bytes())?;
        Self::verify_bag(destination)?;
        Ok(())
    }

    pub fn verify_bag(bag: &Path) -> Result<(), ArchiveError> {
        let declaration = read_bounded(&bag.join(BAG_FILE), MAX_RECORD_BYTES)?;
        if std::str::from_utf8(&declaration)?.replace("\r\n", "\n") != BAG_DECLARATION {
            return invalid("Unsupported BagIt declaration");
        }
        let tags = read_manifest(&bag.join(TAG_MANIFEST))?;
        let expected_tags = BTreeSet::from([BAG_FILE.to_owned(), PAYLOAD_MANIFEST.to_owned()]);
        if tags.keys().cloned().collect::<BTreeSet<_>>() != expected_tags {
            return invalid("Incomplete archive tag manifest");
        }
        for (path, digest) in tags {
            if hash_file(&bag.join(path))? != digest {
                return invalid("Tag checksum mismatch");
            }
        }
        let payload = read_manifest(&bag.join(PAYLOAD_MANIFEST))?;
        let actual: BTreeSet<_> = files(&bag.join("data"))?
            .into_iter()
            .map(|path| format!("data/{path}"))
            .collect();
        if payload.keys().cloned().collect::<BTreeSet<_>>() != actual {
            return invalid("Payload manifest inventory mismatch");
        }
        for (path, digest) in payload {
            if !path.starts_with("data/") || hash_file(&bag.join(path))? != digest {
                return invalid("Payload checksum mismatch");
            }
        }
        let archive = Self::open(&bag.join("data"))?;
        for root in archive.heads()? {
            archive.verify(&root)?;
        }
        Ok(())
    }

    pub fn restore_bag(bag: &Path, destination: &Path) -> Result<Self, ArchiveError> {
        Self::verify_bag(bag)?;
        fresh_destination(destination, bag)?;
        let data = bag.join("data");
        for path in files(&data)? {
            let source = data.join(&path);
            let digest = hash_file(&source)?;
            copy_verified(&source, &destination.join(&path), &digest)?;
        }
        fs::create_dir_all(destination.join("objects"))?;
        fs::create_dir_all(destination.join("refs"))?;
        let restored = Self::open(destination)?;
        for root in restored.heads()? {
            restored.verify(&root)?;
        }
        Ok(restored)
    }

    pub fn checkout(
        &self,
        snapshot: &ObjectId,
        destination: &Path,
    ) -> Result<CheckoutReport, ArchiveError> {
        self.checkout_selected(snapshot, destination, &BTreeMap::new())
    }
    pub fn checkout_selected(
        &self,
        snapshot: &ObjectId,
        destination: &Path,
        selections: &BTreeMap<NoteId, Option<ObjectId>>,
    ) -> Result<CheckoutReport, ArchiveError> {
        self.verify(snapshot)?;
        let notes = self.notes(snapshot)?;
        let mut selected = BTreeMap::new();
        let mut conflicts = Vec::new();
        for (id, entry) in notes {
            let record = match entry {
                Entry::Live { record } => Some(record),
                Entry::Deleted => None,
                Entry::Conflict { alternatives } => match selections.get(&id) {
                    Some(selected) if alternatives.contains(selected) => selected.clone(),
                    Some(_) => {
                        return invalid("The selected version is not a conflict alternative");
                    }
                    None => {
                        conflicts.push(id);
                        None
                    }
                },
            };
            if let Some(record) = record {
                selected.insert(id, (record.clone(), self.note(&record)?));
            }
        }
        if !conflicts.is_empty() {
            return Err(ArchiveError::Conflicts(conflicts));
        }
        fresh_destination(destination, &self.root)?;
        let mut report = CheckoutReport {
            snapshot: snapshot.clone(),
            notes: 0,
            media: 0,
            unresolved: Vec::new(),
        };
        let mut media = BTreeSet::new();
        for (id, (_, note)) in &selected {
            let source = self.read_bytes(&note.source, celestite_core::MAX_FILE_BYTES)?;
            let mut source = std::str::from_utf8(&source)?.to_owned();
            let mut patches = Vec::new();
            for link in &note.links {
                let suffix = link
                    .destination
                    .find(['#', '?'])
                    .map_or("", |offset| &link.destination[offset..]);
                let target = match &link.target {
                    LinkTarget::Note { id: target } if selected.contains_key(target) => {
                        Some(format!("{target}.md{suffix}"))
                    }
                    LinkTarget::File { path, object } => {
                        let extension = Path::new(path)
                            .extension()
                            .and_then(|value| value.to_str())
                            .filter(|value| {
                                value.len() <= MAX_EXTENSION_LENGTH
                                    && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
                            })
                            .unwrap_or("bin");
                        let filename = format!("assets/{}.{}", object.digest(), extension);
                        if media.insert(filename.clone()) {
                            copy_verified(
                                &self.object_path(object),
                                &destination.join(&filename),
                                object,
                            )?;
                        }
                        Some(format!("{filename}{suffix}"))
                    }
                    LinkTarget::External => None,
                    LinkTarget::Note { .. }
                    | LinkTarget::Unresolved { .. }
                    | LinkTarget::Ambiguous { .. } => {
                        report
                            .unresolved
                            .push(format!("{id}: {}", link.destination));
                        None
                    }
                };
                if let Some(target) = target {
                    if !celestite_archive_core::admit_repair(
                        ObjectId::of(source.as_bytes()) == note.source,
                        source.len(),
                        link.start,
                        link.end,
                        true,
                    ) || source.get(link.start..link.end) != Some(link.destination.as_str())
                    {
                        return invalid("Stale or ambiguous source-bound link patch");
                    }
                    patches.push((link.start, link.end, target));
                }
            }
            patches.sort_by_key(|(start, _, _)| *start);
            for pair in patches.windows(2) {
                if pair[0].1 > pair[1].0 {
                    return invalid("Overlapping link patches");
                }
            }
            for (start, end, target) in patches.into_iter().rev() {
                source.replace_range(start..end, &target);
            }
            replace(&destination.join(format!("{id}.md")), source.as_bytes())?;
            report.notes += 1;
        }
        report.media = media.len();
        replace(
            &destination.join("checkout.json"),
            &serde_json::to_vec_pretty(&report)?,
        )?;
        Ok(report)
    }

    pub fn rebuild_index(&self, snapshot: &ObjectId) -> Result<(), ArchiveError> {
        let _lock = self.write_lock()?;
        self.build_index(snapshot)
    }

    fn build_index(&self, snapshot: &ObjectId) -> Result<(), ArchiveError> {
        self.verify(snapshot)?;
        let directory = self.root.join("local");
        fs::create_dir_all(&directory)?;
        let temporary = NamedTempFile::new_in(&directory)?;
        let mut connection = rusqlite::Connection::open(temporary.path())?;
        connection.execute_batch("CREATE TABLE metadata (snapshot TEXT NOT NULL); CREATE TABLE notes (id TEXT NOT NULL, revision TEXT NOT NULL, path TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY (id, revision));")?;
        let transaction = connection.transaction()?;
        transaction.execute("INSERT INTO metadata VALUES (?1)", [snapshot.to_string()])?;
        for (id, entry) in self.notes(snapshot)? {
            for record in entry.alternatives().into_iter().flatten() {
                let note = self.note(&record)?;
                let body = self.read_bytes(&note.source, celestite_core::MAX_FILE_BYTES)?;
                transaction.execute(
                    "INSERT INTO notes VALUES (?1, ?2, ?3, ?4)",
                    (
                        id.to_string(),
                        record.to_string(),
                        note.path,
                        std::str::from_utf8(&body)?,
                    ),
                )?;
            }
        }
        transaction.commit()?;
        drop(connection);
        temporary
            .persist(directory.join("index.sqlite"))
            .map_err(|error| ArchiveError::Io(error.error))?;
        Ok(())
    }
    pub fn search(&self, snapshot: &ObjectId, query: &str) -> Result<Vec<SearchHit>, ArchiveError> {
        let _lock = self.write_lock()?;
        let path = self.root.join("local/index.sqlite");
        let current = if path.exists() {
            rusqlite::Connection::open(&path)
                .ok()
                .and_then(|connection| {
                    connection
                        .query_row("SELECT snapshot FROM metadata", [], |row| {
                            row.get::<_, String>(0)
                        })
                        .ok()
                })
        } else {
            None
        };
        if current.as_deref() != Some(snapshot.to_string().as_str()) {
            self.build_index(snapshot)?;
        }
        let connection = rusqlite::Connection::open(path)?;
        let pattern = format!(
            "%{}%",
            query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let mut statement = connection.prepare("SELECT id, revision, path FROM notes WHERE body LIKE ?1 ESCAPE '\\' ORDER BY id, revision")?;
        let rows = statement.query_map([pattern], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, revision, path) = row?;
            Ok(SearchHit {
                id: id.parse()?,
                revision: revision.parse()?,
                path,
            })
        })
        .collect()
    }
}
