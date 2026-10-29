use celestite_archive_core::{HASH_PREFIX, valid_object_id};
use serde::{Deserialize, Serialize};
use serde::{
    Deserializer,
    de::{Error as _, MapAccess, Visitor},
};
use sha2::{Digest, Sha256};
use std::marker::PhantomData;
use std::{collections::BTreeMap, fmt, str::FromStr};
use uuid::Uuid;

fn unique_map<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct Unique<K, V>(PhantomData<(K, V)>);
    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>> Visitor<'de> for Unique<K, V> {
        type Value = BTreeMap<K, V>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map of unique identities")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = input.next_entry()? {
                if result.insert(key, value).is_some() {
                    return Err(A::Error::custom("duplicate map identity"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Unique(PhantomData))
}

pub const FORMAT_NAME: &str = "celestite.archive";
pub const FORMAT_VERSION: u32 = 1;
pub const SHARD_PREFIX_LENGTH: usize = 2;
pub const MAX_RECORD_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_GRAPH_OBJECTS: usize = 1_000_000;
pub const MAX_ASSET_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const COPY_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error("Invalid archive: {0}")]
    Invalid(String),
    #[error("Unsupported archive format {0}; no data was changed")]
    Unsupported(u32),
    #[error("The archive changed; the previous version remains available")]
    StaleHead,
    #[error("Another writer owns this archive")]
    Locked,
    #[error("Resolve conflicting notes before checkout: {0:?}")]
    Conflicts(Vec<NoteId>),
    #[error("The input exceeds the configured byte or graph budget")]
    TooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NoteId(Uuid);
impl NoteId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
    pub fn uuid(self) -> Uuid {
        self.0
    }
    pub fn shard(self) -> String {
        self.0.simple().to_string()[..SHARD_PREFIX_LENGTH].to_owned()
    }
}
impl Default for NoteId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for NoteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
impl FromStr for NoteId {
    type Err = ArchiveError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let id = Uuid::parse_str(value)
            .map_err(|_| ArchiveError::Invalid(String::from("Invalid note ID")))?;
        if id.is_nil() || id.is_max() {
            return Err(ArchiveError::Invalid(String::from("Reserved note ID")));
        }
        Ok(Self(id))
    }
}
impl TryFrom<String> for NoteId {
    type Error = ArchiveError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl From<NoteId> for String {
    fn from(value: NoteId) -> Self {
        value.to_string()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ObjectId(String);
impl ObjectId {
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{HASH_PREFIX}{:x}", Sha256::digest(bytes)))
    }
    pub(crate) fn from_digest(digest: impl fmt::LowerHex) -> Self {
        Self(format!("{HASH_PREFIX}{digest:x}"))
    }
    pub fn digest(&self) -> &str {
        &self.0[HASH_PREFIX.len()..]
    }
}
impl fmt::Display for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
impl FromStr for ObjectId {
    type Err = ArchiveError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if !valid_object_id(value.as_bytes()) {
            return Err(ArchiveError::Invalid(String::from("Invalid object digest")));
        }
        Ok(Self(value.to_owned()))
    }
}
impl TryFrom<String> for ObjectId {
    type Error = ArchiveError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl From<ObjectId> for String {
    fn from(value: ObjectId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LinkTarget {
    Note {
        id: NoteId,
    },
    Ambiguous {
        previous: NoteId,
        current: Option<NoteId>,
    },
    File {
        path: String,
        object: ObjectId,
    },
    External,
    Unresolved {
        reason: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkBinding {
    pub start: usize,
    pub end: usize,
    pub destination: String,
    pub target: LinkTarget,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteRecord {
    pub id: NoteId,
    pub path: String,
    pub source: ObjectId,
    pub original: ObjectId,
    #[serde(default)]
    pub origin: Option<String>,
    pub links: Vec<LinkBinding>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Entry {
    Live { record: ObjectId },
    Deleted,
    Conflict { alternatives: Vec<Option<ObjectId>> },
}
impl Entry {
    pub fn alternatives(&self) -> Vec<Option<ObjectId>> {
        match self {
            Self::Live { record } => vec![Some(record.clone())],
            Self::Deleted => vec![None],
            Self::Conflict { alternatives } => alternatives.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Record {
    Note {
        note: NoteRecord,
    },
    Shard {
        #[serde(deserialize_with = "unique_map")]
        notes: BTreeMap<NoteId, Entry>,
    },
    Tree {
        #[serde(deserialize_with = "unique_map")]
        shards: BTreeMap<String, ObjectId>,
    },
    Snapshot {
        parents: Vec<ObjectId>,
        tree: ObjectId,
        device: NoteId,
        captured_at: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Format {
    pub format: String,
    pub version: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationStage {
    ObjectsSynced,
    SnapshotSynced,
    ReferenceReplaced,
    ReferenceSynced,
}
#[derive(Clone, Debug, Serialize)]
pub struct CommitReceipt {
    pub snapshot: ObjectId,
    pub conflicts: Vec<NoteId>,
    pub local_committed: bool,
}
#[derive(Debug, Default, Serialize)]
pub struct Verification {
    pub snapshots: usize,
    pub notes: usize,
    pub objects: usize,
    pub unresolved_links: usize,
}
