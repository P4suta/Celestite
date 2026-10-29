use celestite_core::{EditDecision, EditRevision, MAX_FILE_BYTES, MAX_SAFE_REVISION, plan_edit};

#[test]
fn admits_newer_and_identical_replayed_edits() {
    assert_eq!(
        plan_edit(
            EditRevision::new(1),
            EditRevision::new(2),
            true,
            true,
            MAX_FILE_BYTES
        ),
        EditDecision::Accept
    );
    assert_eq!(
        plan_edit(EditRevision::new(2), EditRevision::new(2), true, false, 0),
        EditDecision::Accept
    );
}

#[test]
fn rejects_old_tokens_even_with_newer_revisions() {
    assert_eq!(
        plan_edit(EditRevision::new(1), EditRevision::new(2), false, true, 0),
        EditDecision::Stale
    );
}

#[test]
fn rejects_older_revisions() {
    assert_eq!(
        plan_edit(EditRevision::new(2), EditRevision::new(1), true, true, 0),
        EditDecision::Stale
    );
}

#[test]
fn rejects_replacing_text_without_a_new_revision() {
    assert_eq!(
        plan_edit(EditRevision::new(2), EditRevision::new(2), true, true, 0),
        EditDecision::InvalidRevision
    );
}

#[test]
fn rejects_integers_that_cannot_cross_javascript_without_precision_loss() {
    assert_eq!(
        plan_edit(
            EditRevision::new(0),
            EditRevision::new(MAX_SAFE_REVISION + 1),
            true,
            true,
            0
        ),
        EditDecision::InvalidRevision
    );
}

#[test]
fn rejects_oversized_edits() {
    assert_eq!(
        plan_edit(
            EditRevision::new(0),
            EditRevision::new(1),
            true,
            true,
            MAX_FILE_BYTES + 1
        ),
        EditDecision::TooLarge
    );
}
