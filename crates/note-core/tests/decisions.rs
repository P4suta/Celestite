use celestite_core::{DiskState, EditRevision, OpenTarget, WriteDecision, open_target, plan_write};

#[test]
fn current_revision_can_write_unchanged_file() {
    assert_eq!(
        plan_write(
            EditRevision::new(2),
            EditRevision::new(3),
            DiskState::Unchanged
        ),
        WriteDecision::Write
    );
}

#[test]
fn stale_revision_never_overwrites_newer_save() {
    assert_eq!(
        plan_write(
            EditRevision::new(9),
            EditRevision::new(8),
            DiskState::Unchanged
        ),
        WriteDecision::Stale
    );
}

#[test]
fn changed_file_requires_an_explicit_recovery_choice() {
    assert_eq!(
        plan_write(
            EditRevision::new(2),
            EditRevision::new(3),
            DiskState::Changed
        ),
        WriteDecision::Conflict
    );
}

#[test]
fn equal_revision_remains_retryable_after_io_failure() {
    assert_eq!(
        plan_write(
            EditRevision::new(4),
            EditRevision::new(4),
            DiskState::Unchanged
        ),
        WriteDecision::Write
    );
}

#[test]
fn only_an_empty_untitled_document_is_reused() {
    assert_eq!(open_target(false, true), OpenTarget::Reuse);
    assert_eq!(open_target(false, false), OpenTarget::NewWindow);
    assert_eq!(open_target(true, true), OpenTarget::NewWindow);
    assert_eq!(open_target(true, false), OpenTarget::NewWindow);
}
