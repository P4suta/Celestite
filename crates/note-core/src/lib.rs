#![no_std]
#![deny(clippy::panic, clippy::indexing_slicing)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditRevision(u64);

impl EditRevision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

pub const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;
pub const UTF8_BOM_BYTES: usize = 3;

pub const fn checked_byte_length(current: usize, additional: usize, limit: usize) -> Option<usize> {
    match current.checked_add(additional) {
        Some(total) if total <= limit => Some(total),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Revision pairs require checked construction.
///
/// ```
/// use celestite_core::RevisionState;
/// assert!(RevisionState::restore(0, 1).is_none());
/// assert_eq!(RevisionState::EMPTY.current(), 0);
/// ```
///
/// ```compile_fail
/// use celestite_core::RevisionState;
/// let _ = RevisionState { current: 0, saved: 1 };
/// ```
pub struct RevisionState {
    current: u64,
    saved: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditTransition {
    Accepted(RevisionState),
    Stale,
    InvalidRevision,
    TooLarge,
}

impl RevisionState {
    pub const EMPTY: Self = Self {
        current: 0,
        saved: 0,
    };

    pub const fn restore(current: u64, saved: u64) -> Option<Self> {
        if current <= MAX_SAFE_REVISION && saved <= current {
            Some(Self { current, saved })
        } else {
            None
        }
    }

    pub const fn current(self) -> u64 {
        self.current
    }
    pub const fn saved(self) -> u64 {
        self.saved
    }
    pub const fn is_dirty(self) -> bool {
        self.current != self.saved
    }
    pub const fn mark_saved(self) -> Self {
        Self {
            current: self.current,
            saved: self.current,
        }
    }

    pub const fn edit(
        self,
        incoming: u64,
        token_matches: bool,
        text_changed: bool,
        bytes: usize,
    ) -> EditTransition {
        match plan_edit(
            EditRevision::new(self.current),
            EditRevision::new(incoming),
            token_matches,
            text_changed,
            bytes,
        ) {
            EditDecision::Accept => EditTransition::Accepted(Self {
                current: incoming,
                saved: self.saved,
            }),
            EditDecision::Stale => EditTransition::Stale,
            EditDecision::InvalidRevision => EditTransition::InvalidRevision,
            EditDecision::TooLarge => EditTransition::TooLarge,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseDecision {
    Allow,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationDecision {
    Follow,
    Stale,
    InvalidPosition,
}

pub const fn prefer_web_link(has_domain: bool, local_suffix: bool, local_entry: bool) -> bool {
    has_domain && !local_suffix && !local_entry
}

pub const fn plan_navigation(
    current: u64,
    requested: u64,
    token_matches: bool,
    source_matches: bool,
    position: usize,
    length: usize,
) -> NavigationDecision {
    if current > MAX_SAFE_REVISION || current != requested || !token_matches || !source_matches {
        NavigationDecision::Stale
    } else if position > length {
        NavigationDecision::InvalidPosition
    } else {
        NavigationDecision::Follow
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitDecision {
    CloseNotes,
    WaitForCleanup,
    Exit,
}

pub const fn plan_exit(has_windows: bool, has_pending_cleanup: bool) -> ExitDecision {
    if has_windows {
        ExitDecision::CloseNotes
    } else if has_pending_cleanup {
        ExitDecision::WaitForCleanup
    } else {
        ExitDecision::Exit
    }
}

pub const fn plan_close(
    current: u64,
    requested: u64,
    token_matches: bool,
    dirty: bool,
    discard: bool,
) -> CloseDecision {
    if token_matches && current == requested && (discard || !dirty) {
        CloseDecision::Allow
    } else {
        CloseDecision::Stale
    }
}

pub const fn encoded_length(
    text_bytes: usize,
    newlines: usize,
    bom: bool,
    crlf: bool,
) -> Option<usize> {
    if newlines > text_bytes {
        return None;
    }
    let extra_lines = if crlf { newlines } else { 0 };
    let Some(length) = text_bytes.checked_add(extra_lines) else {
        return None;
    };
    let Some(length) = length.checked_add(if bom { UTF8_BOM_BYTES } else { 0 }) else {
        return None;
    };
    if length <= MAX_FILE_BYTES {
        Some(length)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryFacts {
    pub format: u16,
    pub revision: u64,
    pub saved_revision: u64,
    pub text_bytes: usize,
    pub has_path: bool,
    pub has_fingerprint: bool,
    pub path_absolute: bool,
}

pub const RECOVERY_FORMAT: u16 = 1;

pub const fn admit_recovery(facts: RecoveryFacts) -> Option<RevisionState> {
    if facts.format != RECOVERY_FORMAT
        || facts.text_bytes > MAX_FILE_BYTES
        || facts.has_path != facts.has_fingerprint
        || (facts.has_path && !facts.path_absolute)
    {
        return None;
    }
    RevisionState::restore(facts.revision, facts.saved_revision)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditDecision {
    Accept,
    Stale,
    InvalidRevision,
    TooLarge,
}

pub const fn plan_edit(
    current: EditRevision,
    incoming: EditRevision,
    token_matches: bool,
    text_changed: bool,
    bytes: usize,
) -> EditDecision {
    if !token_matches || incoming.0 < current.0 {
        EditDecision::Stale
    } else if incoming.0 > MAX_SAFE_REVISION || (incoming.0 == current.0 && text_changed) {
        EditDecision::InvalidRevision
    } else if bytes > MAX_FILE_BYTES {
        EditDecision::TooLarge
    } else {
        EditDecision::Accept
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiskState {
    Unchanged,
    Changed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteDecision {
    Write,
    Stale,
    Conflict,
}

pub const fn plan_write(
    saved: EditRevision,
    incoming: EditRevision,
    disk: DiskState,
) -> WriteDecision {
    if incoming.0 < saved.0 {
        WriteDecision::Stale
    } else {
        match disk {
            DiskState::Unchanged => WriteDecision::Write,
            DiskState::Changed => WriteDecision::Conflict,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenTarget {
    Reuse,
    NewWindow,
}

pub const fn open_target(has_path: bool, text_is_empty: bool) -> OpenTarget {
    if !has_path && text_is_empty {
        OpenTarget::Reuse
    } else {
        OpenTarget::NewWindow
    }
}

pub const fn library_write(has_path: bool, has_library: bool, body_is_empty: bool) -> bool {
    !has_path && has_library && !body_is_empty
}

pub const MAX_LABEL_SCALARS: usize = 999;
pub const MAX_LABEL_BYTES: usize = MAX_LABEL_SCALARS * 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelVerdict {
    Valid,
    Empty,
    LineEnding,
    UnescapedBracket,
    Reserved,
    TooLong,
}

const UTF8_CONTINUATION_MASK: u8 = 0b1100_0000;
const UTF8_CONTINUATION_PATTERN: u8 = 0b1000_0000;

const fn continuation(byte: u8) -> bool {
    byte & UTF8_CONTINUATION_MASK == UTF8_CONTINUATION_PATTERN
}

/// Checks the byte shape of a CommonMark link label without its brackets.
///
/// A backslash followed by ASCII punctuation is one escape sequence.
/// Scalar counting uses UTF-8 lead bytes, so the caller must pass valid UTF-8.
pub fn label_verdict(bytes: &[u8]) -> LabelVerdict {
    if bytes.len() > MAX_LABEL_BYTES {
        return LabelVerdict::TooLong;
    }
    let mut scalars = 0_usize;
    let mut content = false;
    let mut rest = bytes;
    while let [byte, tail @ ..] = rest {
        rest = tail;
        if !continuation(*byte) {
            scalars += 1;
        }
        match byte {
            b'\n' | b'\r' => return LabelVerdict::LineEnding,
            b'[' | b']' => return LabelVerdict::UnescapedBracket,
            b'\\' => {
                if let [next, after @ ..] = rest
                    && next.is_ascii_punctuation()
                {
                    scalars += 1;
                    rest = after;
                }
                content = true;
            }
            b' ' | b'\t' => {}
            b'^' if !content => return LabelVerdict::Reserved,
            _ => content = true,
        }
    }
    if !content {
        LabelVerdict::Empty
    } else if scalars > MAX_LABEL_SCALARS {
        LabelVerdict::TooLong
    } else {
        LabelVerdict::Valid
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefinitionAction {
    Append,
    Keep,
    Refuse,
}

pub const fn plan_definition(label_defined: bool, same_destination: bool) -> DefinitionAction {
    if !label_defined {
        DefinitionAction::Append
    } else if same_destination {
        DefinitionAction::Keep
    } else {
        DefinitionAction::Refuse
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Separator {
    None,
    Newline,
    BlankLine,
}

/// Chooses the bytes between the current body and an appended reference definition.
///
/// A definition cannot interrupt a paragraph, so a blank line precedes the first definition unless the body already ends with a blank line or a definition.
pub const fn definition_separator(
    is_empty: bool,
    ends_with_newline: bool,
    last_line_blank: bool,
    last_line_is_definition: bool,
) -> Separator {
    if is_empty {
        Separator::None
    } else if !ends_with_newline {
        if last_line_blank || last_line_is_definition {
            Separator::Newline
        } else {
            Separator::BlankLine
        }
    } else if last_line_blank || last_line_is_definition {
        Separator::None
    } else {
        Separator::Newline
    }
}
