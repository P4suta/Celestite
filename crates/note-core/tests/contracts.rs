use celestite_core::{
    CloseDecision, EditTransition, MAX_FILE_BYTES, MAX_SAFE_REVISION, RECOVERY_FORMAT,
    RecoveryFacts, RevisionState, UTF8_BOM_BYTES, admit_recovery, checked_byte_length,
    encoded_length, plan_close,
};

const REVISIONS: [u64; 8] = [
    0,
    1,
    2,
    MAX_SAFE_REVISION - 1,
    MAX_SAFE_REVISION,
    MAX_SAFE_REVISION + 1,
    u64::MAX - 1,
    u64::MAX,
];
const SIZES: [usize; 9] = [
    0,
    1,
    2,
    MAX_FILE_BYTES - 1,
    MAX_FILE_BYTES,
    MAX_FILE_BYTES + 1,
    usize::MAX / 2,
    usize::MAX - 1,
    usize::MAX,
];

#[test]
fn persisted_and_cross_language_limits_match_the_public_contract() {
    assert_eq!(MAX_FILE_BYTES, 16_777_216);
    assert_eq!(MAX_SAFE_REVISION, 9_007_199_254_740_991);
    assert_eq!(UTF8_BOM_BYTES, 3);
    assert_eq!(RECOVERY_FORMAT, 1);
}

#[test]
fn exit_waits_for_live_notes_and_post_close_cleanup() {
    use celestite_core::{ExitDecision, plan_exit};
    assert_eq!(plan_exit(false, false), ExitDecision::Exit);
    assert_eq!(plan_exit(false, true), ExitDecision::WaitForCleanup);
    assert_eq!(plan_exit(true, false), ExitDecision::CloseNotes);
    assert_eq!(plan_exit(true, true), ExitDecision::CloseNotes);
}

#[test]
fn byte_and_encoding_budgets_match_widened_mathematical_sizes() {
    for bytes in SIZES {
        for additional in SIZES {
            for limit in SIZES {
                let exact = bytes as u128 + additional as u128;
                let result = checked_byte_length(bytes, additional, limit);
                assert_eq!(
                    result.map(|length| length as u128),
                    (exact <= limit as u128).then_some(exact)
                );
            }
            for bom in [false, true] {
                for crlf in [false, true] {
                    let exact = bytes as u128
                        + if crlf { additional as u128 } else { 0 }
                        + if bom { UTF8_BOM_BYTES as u128 } else { 0 };
                    let valid = additional <= bytes && exact <= MAX_FILE_BYTES as u128;
                    assert_eq!(
                        encoded_length(bytes, additional, bom, crlf).map(|length| length as u128),
                        valid.then_some(exact)
                    );
                }
            }
        }
    }
}

#[test]
fn close_permission_never_authorizes_a_different_revision_or_dirty_implicit_close() {
    for current in REVISIONS {
        for requested in REVISIONS {
            for token in [false, true] {
                for dirty in [false, true] {
                    for discard in [false, true] {
                        let allowed = plan_close(current, requested, token, dirty, discard)
                            == CloseDecision::Allow;
                        assert_eq!(
                            allowed,
                            token && current == requested && (discard || !dirty)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn revision_states_preserve_saved_progress_across_all_boundary_transitions() {
    assert_eq!(RevisionState::EMPTY.current(), 0);
    assert_eq!(RevisionState::EMPTY.saved(), 0);
    for current in REVISIONS {
        for saved in REVISIONS {
            let restored = RevisionState::restore(current, saved);
            assert_eq!(
                restored.is_some(),
                current <= MAX_SAFE_REVISION && saved <= current
            );
            let Some(state) = restored else {
                continue;
            };
            assert_eq!((state.current(), state.saved()), (current, saved));
            assert_eq!(state.is_dirty(), current != saved);
            assert_eq!(
                (state.mark_saved().current(), state.mark_saved().saved()),
                (current, current)
            );
            assert!(!state.mark_saved().is_dirty());
            for incoming in REVISIONS {
                for token in [false, true] {
                    for changed in [false, true] {
                        for bytes in SIZES {
                            let accepted = token
                                && incoming >= current
                                && incoming <= MAX_SAFE_REVISION
                                && (incoming > current || !changed)
                                && bytes <= MAX_FILE_BYTES;
                            match state.edit(incoming, token, changed, bytes) {
                                EditTransition::Accepted(next) => {
                                    assert!(accepted);
                                    assert_eq!((next.current(), next.saved()), (incoming, saved));
                                }
                                EditTransition::Stale => assert!(!token || incoming < current),
                                EditTransition::InvalidRevision => assert!(
                                    token
                                        && incoming >= current
                                        && (incoming > MAX_SAFE_REVISION
                                            || (incoming == current && changed))
                                ),
                                EditTransition::TooLarge => assert!(
                                    token
                                        && incoming >= current
                                        && incoming <= MAX_SAFE_REVISION
                                        && (incoming > current || !changed)
                                        && bytes > MAX_FILE_BYTES
                                ),
                            }
                            assert_eq!(
                                matches!(
                                    state.edit(incoming, token, changed, bytes),
                                    EditTransition::Accepted(_)
                                ),
                                accepted
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn recovery_requires_supported_format_coherent_location_and_ordered_revisions() {
    for format in [0, RECOVERY_FORMAT, RECOVERY_FORMAT + 1, u16::MAX] {
        for revision in REVISIONS {
            for saved_revision in REVISIONS {
                for text_bytes in SIZES {
                    for has_path in [false, true] {
                        for has_fingerprint in [false, true] {
                            for path_absolute in [false, true] {
                                let facts = RecoveryFacts {
                                    format,
                                    revision,
                                    saved_revision,
                                    text_bytes,
                                    has_path,
                                    has_fingerprint,
                                    path_absolute,
                                };
                                let valid = format == RECOVERY_FORMAT
                                    && revision <= MAX_SAFE_REVISION
                                    && saved_revision <= revision
                                    && text_bytes <= MAX_FILE_BYTES
                                    && has_path == has_fingerprint
                                    && (!has_path || path_absolute);
                                let result = admit_recovery(facts);
                                assert_eq!(result.is_some(), valid);
                                if let Some(state) = result {
                                    assert_eq!(
                                        (state.current(), state.saved()),
                                        (revision, saved_revision)
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
