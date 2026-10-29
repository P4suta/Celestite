#![no_std]
#![deny(clippy::panic, clippy::indexing_slicing)]

pub const HASH_PREFIX: &str = "sha256:";
pub const DIGEST_LENGTH: usize = 64;
pub const OBJECT_ID_LENGTH: usize = HASH_PREFIX.len() + DIGEST_LENGTH;

pub const fn valid_object_id_length(length: usize) -> bool {
    length == OBJECT_ID_LENGTH
}

pub fn valid_object_id(bytes: &[u8]) -> bool {
    if !valid_object_id_length(bytes.len()) {
        return false;
    }
    let [b's', b'h', b'a', b'2', b'5', b'6', b':', digest @ ..] = bytes else {
        return false;
    };
    for index in 0..DIGEST_LENGTH {
        let Some(byte) = digest.get(index).copied() else {
            return false;
        };
        if !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte) {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureAdmission {
    Confirmed,
    Conflicted,
    DifferentSource,
}

pub const fn admit_capture(source_matches: bool, conflicted: bool) -> CaptureAdmission {
    if conflicted {
        CaptureAdmission::Conflicted
    } else if source_matches {
        CaptureAdmission::Confirmed
    } else {
        CaptureAdmission::DifferentSource
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeChoice {
    Shared,
    Left,
    Right,
    Conflict,
}

pub const fn merge_choice(equal: bool, left_is_base: bool, right_is_base: bool) -> MergeChoice {
    if equal {
        MergeChoice::Shared
    } else if left_is_base {
        MergeChoice::Right
    } else if right_is_base {
        MergeChoice::Left
    } else {
        MergeChoice::Conflict
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Publication {
    Preparing,
    ObjectsSynced,
    SnapshotSynced,
    Published,
}

impl Publication {
    pub const fn advance(self, success: bool) -> Self {
        if !success {
            return self;
        }
        match self {
            Self::Preparing => Self::ObjectsSynced,
            Self::ObjectsSynced => Self::SnapshotSynced,
            Self::SnapshotSynced => Self::Published,
            Self::Published => Self::Published,
        }
    }

    pub const fn may_publish(self, expected_head: bool, complete: bool) -> bool {
        matches!(self, Self::SnapshotSynced) && expected_head && complete
    }
}

pub const fn admit_extension(fresh: bool, parents_known: bool, payload_complete: bool) -> bool {
    fresh && parents_known && payload_complete
}

pub const fn admit_repair(
    same_source: bool,
    length: usize,
    start: usize,
    end: usize,
    unique_target: bool,
) -> bool {
    same_source && start <= end && end <= length && unique_target
}
