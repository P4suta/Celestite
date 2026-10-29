use celestite_archive_core::{
    MergeChoice, Publication, admit_extension, admit_repair, merge_choice,
};

#[test]
fn capture_receipts_never_acknowledge_conflicts_or_another_selected_source() {
    use celestite_archive_core::{CaptureAdmission, admit_capture};
    assert_eq!(admit_capture(true, false), CaptureAdmission::Confirmed);
    assert_eq!(
        admit_capture(false, false),
        CaptureAdmission::DifferentSource
    );
    assert_eq!(admit_capture(true, true), CaptureAdmission::Conflicted);
    assert_eq!(admit_capture(false, true), CaptureAdmission::Conflicted);
}

#[test]
fn object_identifier_grammar_enforces_exact_width_prefix_and_lowercase_hex() {
    use celestite_archive_core::valid_object_id;
    let valid = format!("sha256:{}", "a".repeat(64));
    assert!(valid_object_id(valid.as_bytes()));
    for value in ["", "sha256:", "SHA256:", "../objects"] {
        assert!(!valid_object_id(value.as_bytes()));
    }
    for length in 0..=72 {
        assert_eq!(
            valid_object_id(format!("sha256:{}", "0".repeat(length)).as_bytes()),
            length == 64
        );
    }
    for index in 0..valid.len() {
        for byte in u8::MIN..=u8::MAX {
            let mut bytes = valid.as_bytes().to_vec();
            bytes[index] = byte;
            let expected = if index < "sha256:".len() {
                byte == valid.as_bytes()[index]
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            };
            assert_eq!(
                valid_object_id(&bytes),
                expected,
                "position {index}, byte {byte}"
            );
        }
    }
}

#[test]
fn merge_and_publication_cover_every_boolean_combination() {
    for same in [false, true] {
        for left_base in [false, true] {
            for right_base in [false, true] {
                let result = merge_choice(same, left_base, right_base);
                assert_eq!(
                    result == MergeChoice::Conflict,
                    !same && !left_base && !right_base
                );
                assert_eq!(result == MergeChoice::Shared, same);
                if !same && left_base {
                    assert_eq!(result, MergeChoice::Right);
                }
                if !same && !left_base && right_base {
                    assert_eq!(result, MergeChoice::Left);
                }
            }
        }
    }
    for stage in [
        Publication::Preparing,
        Publication::ObjectsSynced,
        Publication::SnapshotSynced,
        Publication::Published,
    ] {
        assert_eq!(stage.advance(false), stage);
        for head in [false, true] {
            for complete in [false, true] {
                assert_eq!(
                    stage.may_publish(head, complete),
                    stage == Publication::SnapshotSynced && head && complete
                );
            }
        }
    }
    for fresh in [false, true] {
        for known in [false, true] {
            for complete in [false, true] {
                assert_eq!(
                    admit_extension(fresh, known, complete),
                    fresh && known && complete
                );
            }
        }
    }
}

#[test]
fn successful_publication_advances_only_through_the_declared_write_phases() {
    let mut state = Publication::Preparing;
    for expected in [
        Publication::ObjectsSynced,
        Publication::SnapshotSynced,
        Publication::Published,
        Publication::Published,
    ] {
        state = state.advance(true);
        assert_eq!(state, expected);
    }
}

#[test]
fn repair_admission_checks_source_identity_and_all_span_boundaries() {
    for length in [0, 1, 2, usize::MAX] {
        for start in [0, 1, 2, usize::MAX] {
            for end in [0, 1, 2, usize::MAX] {
                for same in [false, true] {
                    for unique in [false, true] {
                        assert_eq!(
                            admit_repair(same, length, start, end, unique),
                            same && unique && start <= end && end <= length
                        );
                    }
                }
            }
        }
    }
}
