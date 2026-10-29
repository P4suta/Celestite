#![cfg_attr(kani, feature(stmt_expr_attributes, proc_macro_hygiene))]

#[cfg(kani)]
mod proofs {
    #[kani::proof]
    #[kani::unwind(1)]
    fn inferred_https_never_overrides_local_file_intent() {
        let domain: bool = kani::any();
        let suffix: bool = kani::any();
        let entry: bool = kani::any();
        let inferred = celestite_core::prefer_web_link(domain, suffix, entry);
        assert_eq!(inferred, domain && !suffix && !entry);
        kani::cover!(inferred);
        kani::cover!(!domain);
        kani::cover!(domain && suffix);
        kani::cover!(domain && !suffix && entry);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn navigation_requires_the_exact_source_and_a_bounded_position() {
        use celestite_core::{MAX_SAFE_REVISION, NavigationDecision, plan_navigation};
        let current: u64 = kani::any();
        let requested: u64 = kani::any();
        let token_matches: bool = kani::any();
        let source_matches: bool = kani::any();
        let position: usize = kani::any();
        let length: usize = kani::any();
        let fresh =
            current <= MAX_SAFE_REVISION && current == requested && token_matches && source_matches;
        let result = plan_navigation(
            current,
            requested,
            token_matches,
            source_matches,
            position,
            length,
        );
        assert_eq!(
            result == NavigationDecision::Follow,
            fresh && position <= length
        );
        assert_eq!(result == NavigationDecision::Stale, !fresh);
        assert_eq!(
            result == NavigationDecision::InvalidPosition,
            fresh && position > length
        );
        kani::cover!(result == NavigationDecision::Follow);
        kani::cover!(result == NavigationDecision::Stale);
        kani::cover!(result == NavigationDecision::InvalidPosition);
        kani::cover!(fresh && position == length);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn editor_capture_acknowledges_only_its_selected_unconflicted_source() {
        use celestite_archive_core::{CaptureAdmission, admit_capture};
        let matches: bool = kani::any();
        let conflict: bool = kani::any();
        let result = admit_capture(matches, conflict);
        assert_eq!(result == CaptureAdmission::Confirmed, matches && !conflict);
        assert_eq!(result == CaptureAdmission::Conflicted, conflict);
        assert_eq!(
            result == CaptureAdmission::DifferentSource,
            !matches && !conflict
        );
        kani::cover!(result == CaptureAdmission::Confirmed);
        kani::cover!(result == CaptureAdmission::Conflicted);
        kani::cover!(result == CaptureAdmission::DifferentSource);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn archive_object_id_lengths_cover_every_machine_integer() {
        let length: usize = kani::any();
        assert_eq!(
            celestite_archive_core::valid_object_id_length(length),
            length == 71
        );
        kani::cover!(length == 71);
        kani::cover!(length < 71);
        kani::cover!(length > 71);
    }

    #[kani::proof]
    #[kani::unwind(75)]
    fn archive_object_ids_accept_exactly_the_complete_fixed_width_representation() {
        const BUFFER_LENGTH: usize = celestite_archive_core::OBJECT_ID_LENGTH;
        let bytes: [u8; BUFFER_LENGTH] = kani::any();
        let admitted = celestite_archive_core::valid_object_id(&bytes);
        let prefix = bytes[0] == b's'
            && bytes[1] == b'h'
            && bytes[2] == b'a'
            && bytes[3] == b'2'
            && bytes[4] == b'5'
            && bytes[5] == b'6'
            && bytes[6] == b':';
        let mut expected = prefix;
        let mut index = 7;
        while index < BUFFER_LENGTH {
            let byte = bytes[index];
            expected &= (byte >= b'0' && byte <= b'9') || (byte >= b'a' && byte <= b'f');
            index += 1;
        }
        assert_eq!(admitted, expected);
        kani::cover!(admitted);
        kani::cover!(!prefix);
        kani::cover!(prefix && !admitted);
        kani::cover!(admitted && bytes[BUFFER_LENGTH - 1] == b'f');
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn fuzz_admission_requires_completed_nonempty_current_runs() {
        use celestite_assurance_core::{FuzzEvidence, admit_fuzz_run};
        let facts = FuzzEvidence {
            exit: if kani::any() { Some(kani::any()) } else { None },
            expected_runs: kani::any(),
            completed_runs: kani::any(),
            corpus_files: kani::any(),
            source_matches: kani::any(),
        };
        let valid = facts.exit == Some(0)
            && facts.expected_runs > 0
            && facts.completed_runs == facts.expected_runs
            && facts.corpus_files > 0
            && facts.source_matches;
        assert_eq!(admit_fuzz_run(facts), valid);
        kani::cover!(valid);
        kani::cover!(facts.corpus_files == 0);
        kani::cover!(facts.completed_runs != facts.expected_runs);
        kani::cover!(!facts.source_matches);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn archive_merge_preserves_every_concurrent_change() {
        use celestite_archive_core::{MergeChoice, merge_choice};
        let same: bool = kani::any();
        let left: bool = kani::any();
        let right: bool = kani::any();
        let choice = merge_choice(same, left, right);
        assert_eq!(choice == MergeChoice::Conflict, !same && !left && !right);
        assert!(!same || choice == MergeChoice::Shared);
        assert!(same || !left || choice == MergeChoice::Right);
        assert!(same || left || !right || choice == MergeChoice::Left);
        kani::cover!(choice == MergeChoice::Conflict);
        kani::cover!(choice == MergeChoice::Shared);
        kani::cover!(choice == MergeChoice::Left || choice == MergeChoice::Right);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn archive_publication_requires_synced_objects_and_an_unchanged_head() {
        use celestite_archive_core::Publication;
        let stage = match kani::any::<u8>() % 4 {
            0 => Publication::Preparing,
            1 => Publication::ObjectsSynced,
            2 => Publication::SnapshotSynced,
            _ => Publication::Published,
        };
        let head: bool = kani::any();
        let complete: bool = kani::any();
        assert_eq!(
            stage.may_publish(head, complete),
            stage == Publication::SnapshotSynced && head && complete
        );
        assert_eq!(stage.advance(false), stage);
        assert_eq!(
            Publication::Preparing.advance(true).advance(true),
            Publication::SnapshotSynced
        );
        assert_eq!(
            Publication::Preparing
                .advance(true)
                .advance(true)
                .advance(true),
            Publication::Published
        );
        kani::cover!(stage == Publication::Preparing);
        kani::cover!(stage == Publication::ObjectsSynced);
        kani::cover!(stage == Publication::SnapshotSynced);
        kani::cover!(stage == Publication::Published);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn archive_repairs_require_matching_sources_and_valid_byte_spans() {
        let same: bool = kani::any();
        let unique: bool = kani::any();
        let length: usize = kani::any();
        let start: usize = kani::any();
        let end: usize = kani::any();
        let admitted = celestite_archive_core::admit_repair(same, length, start, end, unique);
        assert_eq!(admitted, same && unique && start <= end && end <= length);
        if admitted {
            assert!(end - start <= length);
        }
        kani::cover!(admitted);
        kani::cover!(!same);
        kani::cover!(start > end || end > length);
        kani::cover!(!unique);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn archive_graph_extensions_require_fresh_identity_and_complete_parents() {
        let fresh: bool = kani::any();
        let known: bool = kani::any();
        let complete: bool = kani::any();
        let admitted = celestite_archive_core::admit_extension(fresh, known, complete);
        assert_eq!(admitted, fresh && known && complete);
        kani::cover!(admitted);
        kani::cover!(!fresh);
        kani::cover!(!known || !complete);
    }

    use celestite_assurance_core::{
        MutationAdmission, MutationEvidence, ObservedCounts, ProofAdmission, ProofInventory,
        RunEvidence, admit_mutation_run, admit_proof_run,
    };

    #[kani::proof]
    #[kani::unwind(1)]
    fn mutation_admission_rejects_missing_stale_or_interrupted_results() {
        let evidence = MutationEvidence {
            discovered: kani::any(),
            executed: kani::any(),
            caught: kani::any(),
            unviable: kani::any(),
            missed: kani::any(),
            timeouts: kani::any(),
            completed: kani::any(),
            baseline_passed: kani::any(),
            inventory_matches: kani::any(),
            properties_valid: kani::any(),
            source_matches: kani::any(),
        };
        let exit = if kani::any() { Some(kani::any()) } else { None };
        let sum = evidence.caught as u128 + evidence.unviable as u128;
        let valid = exit == Some(0)
            && evidence.discovered > 0
            && evidence.caught > 0
            && evidence.executed == evidence.discovered
            && sum == evidence.discovered as u128
            && evidence.missed == 0
            && evidence.timeouts == 0
            && evidence.completed
            && evidence.baseline_passed
            && evidence.inventory_matches
            && evidence.properties_valid
            && evidence.source_matches;
        let admitted = admit_mutation_run(exit, evidence) == MutationAdmission::Admit;
        assert_eq!(admitted, valid);
        kani::cover!(admitted);
        kani::cover!(exit.is_none());
        kani::cover!(evidence.caught == 0);
        kani::cover!(sum > usize::MAX as u128);
        kani::cover!(evidence.timeouts > 0);
        kani::cover!(!evidence.source_matches);
    }

    #[cfg(feature = "negative-control")]
    #[kani::proof]
    #[kani::unwind(1)]
    fn rejects_a_false_mutation_admission_contract() {
        let interrupted = MutationEvidence {
            discovered: 1,
            executed: 1,
            caught: 1,
            unviable: 0,
            missed: 0,
            timeouts: 1,
            completed: true,
            baseline_passed: true,
            inventory_matches: true,
            properties_valid: true,
            source_matches: true,
        };
        assert_eq!(
            admit_mutation_run(Some(0), interrupted),
            MutationAdmission::Admit,
            "negative-control: interrupted mutations must not be admitted"
        );
    }
    use celestite_core::{
        CloseDecision, DiskState, EditDecision, EditRevision, EditTransition, ExitDecision,
        MAX_FILE_BYTES, MAX_SAFE_REVISION, OpenTarget, RECOVERY_FORMAT, RecoveryFacts,
        RevisionState, UTF8_BOM_BYTES, WriteDecision, admit_recovery, checked_byte_length,
        encoded_length, open_target, plan_close, plan_edit, plan_exit, plan_write,
    };

    #[kani::proof]
    #[kani::unwind(1)]
    fn application_exit_waits_for_live_notes_and_pending_cleanup() {
        let windows: bool = kani::any();
        let cleanup: bool = kani::any();
        let decision = plan_exit(windows, cleanup);
        assert_eq!(decision == ExitDecision::Exit, !windows && !cleanup);
        assert_eq!(
            decision == ExitDecision::WaitForCleanup,
            !windows && cleanup
        );
        assert_eq!(decision == ExitDecision::CloseNotes, windows);
        kani::cover!(decision == ExitDecision::Exit);
        kani::cover!(decision == ExitDecision::WaitForCleanup);
        kani::cover!(decision == ExitDecision::CloseNotes);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn close_requires_the_exact_current_note_and_revision() {
        let current: u64 = kani::any();
        let requested: u64 = kani::any();
        let token_matches: bool = kani::any();
        let dirty: bool = kani::any();
        let discard: bool = kani::any();
        let allowed =
            plan_close(current, requested, token_matches, dirty, discard) == CloseDecision::Allow;
        assert_eq!(
            allowed,
            token_matches && current == requested && (discard || !dirty)
        );
        kani::cover!(allowed && dirty && discard);
        kani::cover!(allowed && !dirty && !discard);
        kani::cover!(!allowed && discard && current != requested);
        kani::cover!(!allowed && !token_matches);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn encoded_lengths_match_mathematical_sizes_without_overflow() {
        let bytes: usize = kani::any();
        let newlines: usize = kani::any();
        let bom: bool = kani::any();
        let crlf: bool = kani::any();
        let exact = bytes as u128
            + if crlf { newlines as u128 } else { 0 }
            + if bom { UTF8_BOM_BYTES as u128 } else { 0 };
        let result = encoded_length(bytes, newlines, bom, crlf);
        assert_eq!(
            result.is_some(),
            newlines <= bytes && exact <= MAX_FILE_BYTES as u128
        );
        if let Some(length) = result {
            assert_eq!(length as u128, exact);
        }
        kani::cover!(result == Some(0));
        kani::cover!(result == Some(MAX_FILE_BYTES));
        kani::cover!(result.is_none() && exact > usize::MAX as u128);
        kani::cover!(result.is_none() && newlines > bytes);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn recovery_metadata_never_constructs_an_invalid_document_state() {
        let facts = RecoveryFacts {
            format: kani::any(),
            revision: kani::any(),
            saved_revision: kani::any(),
            text_bytes: kani::any(),
            has_path: kani::any(),
            has_fingerprint: kani::any(),
            path_absolute: kani::any(),
        };
        let restored = admit_recovery(facts);
        let valid = facts.format == RECOVERY_FORMAT
            && facts.revision <= MAX_SAFE_REVISION
            && facts.saved_revision <= facts.revision
            && facts.text_bytes <= MAX_FILE_BYTES
            && facts.has_path == facts.has_fingerprint
            && (!facts.has_path || facts.path_absolute);
        assert_eq!(restored.is_some(), valid);
        if let Some(state) = restored {
            assert_eq!(state.current(), facts.revision);
            assert_eq!(state.saved(), facts.saved_revision);
        }
        kani::cover!(valid && facts.has_path);
        kani::cover!(valid && !facts.has_path);
        kani::cover!(!valid && facts.saved_revision > facts.revision);
        kani::cover!(!valid && facts.has_path != facts.has_fingerprint);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn revision_transitions_preserve_bounds_and_saved_progress() {
        let current: u64 = kani::any();
        let saved: u64 = kani::any();
        let restored = RevisionState::restore(current, saved);
        assert_eq!(
            restored.is_some(),
            saved <= current && current <= MAX_SAFE_REVISION
        );
        kani::cover!(restored.is_none());
        let Some(state) = restored else {
            return;
        };
        assert_eq!(state.is_dirty(), current != saved);
        let clean = state.mark_saved();
        assert_eq!(clean.current(), current);
        assert_eq!(clean.saved(), current);
        assert!(!clean.is_dirty());
        kani::cover!(state.is_dirty());
        kani::cover!(!state.is_dirty());
        let incoming: u64 = kani::any();
        let token: bool = kani::any();
        let changed: bool = kani::any();
        let bytes: usize = kani::any();
        let transition = state.edit(incoming, token, changed, bytes);
        let accepted = token
            && incoming >= current
            && incoming <= MAX_SAFE_REVISION
            && (incoming > current || !changed)
            && bytes <= MAX_FILE_BYTES;
        assert_eq!(matches!(transition, EditTransition::Accepted(_)), accepted);
        if let EditTransition::Accepted(next) = transition {
            assert_eq!(next.current(), incoming);
            assert_eq!(next.saved(), saved);
            assert!(next.saved() <= next.current() && next.current() <= MAX_SAFE_REVISION);
        }
        kani::cover!(matches!(transition, EditTransition::Accepted(_)));
        kani::cover!(transition == EditTransition::Stale);
        kani::cover!(transition == EditTransition::InvalidRevision);
        kani::cover!(transition == EditTransition::TooLarge);
    }

    fn edit_or_save(current: u64, saved: u64) -> (u64, u64) {
        let Some(state) = RevisionState::restore(current, saved) else {
            return (current, saved);
        };
        let next = if kani::any() {
            state.mark_saved()
        } else if let EditTransition::Accepted(next) =
            state.edit(kani::any(), kani::any(), kani::any(), kani::any())
        {
            next
        } else {
            state
        };
        (next.current(), next.saved())
    }

    #[kani::proof]
    #[kani::unwind(2)]
    fn arbitrary_finite_edit_save_sequences_preserve_revision_invariants() {
        let initial_current: u64 = kani::any();
        let initial_saved: u64 = kani::any();
        let Some(initial) = RevisionState::restore(initial_current, initial_saved) else {
            return;
        };
        let mut current = initial.current();
        let mut saved = initial.saved();
        let steps: u64 = kani::any();
        let mut remaining = steps;
        #[kani::loop_invariant(saved <= current && current <= MAX_SAFE_REVISION && current >= initial_current && saved >= initial_saved)]
        while remaining > 0 {
            let previous_remaining = remaining;
            (current, saved) = edit_or_save(current, saved);
            remaining -= 1;
            assert!(
                remaining < previous_remaining,
                "revision trace makes progress"
            );
        }
        assert_eq!(remaining, 0);
        assert!(saved <= current && current <= MAX_SAFE_REVISION);
        assert!(current >= initial_current && saved >= initial_saved);
        kani::cover!(steps == 0);
        kani::cover!(steps > 0 && current != saved);
        kani::cover!(steps > 0 && current == saved);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn byte_budgets_reject_overflow_and_every_oversized_append() {
        let current: usize = kani::any();
        let additional: usize = kani::any();
        let limit: usize = kani::any();
        let mathematical = current as u128 + additional as u128;
        let admitted = checked_byte_length(current, additional, limit);
        assert_eq!(admitted.is_some(), mathematical <= limit as u128);
        if let Some(total) = admitted {
            assert_eq!(total as u128, mathematical);
        }
        kani::cover!(admitted == Some(0));
        kani::cover!(admitted.is_some() && mathematical > 0);
        kani::cover!(mathematical > usize::MAX as u128);
        kani::cover!(mathematical <= usize::MAX as u128 && mathematical > limit as u128);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn proof_admission_rejects_every_incomplete_or_unbound_run() {
        let expected = ProofInventory {
            proofs: kani::any(),
            negative_controls: kani::any(),
        };
        let exit = if kani::any() { Some(kani::any()) } else { None };
        let evidence = RunEvidence {
            counts: ObservedCounts {
                total: kani::any(),
                executed: kani::any(),
                passed: kani::any(),
                failed: kani::any(),
            },
            completed: kani::any(),
            inventory_matches: kani::any(),
            properties_valid: kani::any(),
            source_matches: kani::any(),
        };
        let counts_match = match expected.proofs.checked_add(expected.negative_controls) {
            Some(total) => {
                evidence.counts.total == total
                    && evidence.counts.executed == total
                    && evidence.counts.passed == expected.proofs
                    && evidence.counts.failed == expected.negative_controls
            }
            None => false,
        };
        let contract = expected.proofs > 0
            && expected.negative_controls > 0
            && exit == Some(celestite_assurance_core::COUNTEREXAMPLE_EXIT_CODE)
            && evidence.completed
            && evidence.inventory_matches
            && evidence.properties_valid
            && evidence.source_matches
            && counts_match;
        let admitted = admit_proof_run(expected, exit, evidence) == ProofAdmission::Admit;
        assert_eq!(admitted, contract);
        kani::cover!(admitted);
        kani::cover!(exit.is_none());
        kani::cover!(expected.proofs == 0);
        kani::cover!(
            expected
                .proofs
                .checked_add(expected.negative_controls)
                .is_none()
        );
        kani::cover!(!evidence.source_matches);
        kani::cover!(!evidence.completed);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn write_decision_covers_every_revision_and_disk_state() {
        let saved: u64 = kani::any();
        let incoming: u64 = kani::any();
        let changed: bool = kani::any();
        let disk = if changed {
            DiskState::Changed
        } else {
            DiskState::Unchanged
        };
        let decision = plan_write(EditRevision::new(saved), EditRevision::new(incoming), disk);
        assert_eq!(
            decision == WriteDecision::Write,
            incoming >= saved && !changed
        );
        assert_eq!(decision == WriteDecision::Stale, incoming < saved);
        assert_eq!(
            decision == WriteDecision::Conflict,
            incoming >= saved && changed
        );
        kani::cover!(decision == WriteDecision::Write);
        kani::cover!(decision == WriteDecision::Stale);
        kani::cover!(decision == WriteDecision::Conflict);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn open_target_preserves_every_nonempty_or_named_document() {
        let has_path: bool = kani::any();
        let empty: bool = kani::any();
        let target = open_target(has_path, empty);
        assert_eq!(target == OpenTarget::Reuse, !has_path && empty);
        kani::cover!(target == OpenTarget::Reuse);
        kani::cover!(target == OpenTarget::NewWindow);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn edit_validation_covers_tokens_revisions_and_sizes() {
        let current: u64 = kani::any();
        let incoming: u64 = kani::any();
        let token_matches: bool = kani::any();
        let text_changed: bool = kani::any();
        let bytes: usize = kani::any();
        let result = plan_edit(
            EditRevision::new(current),
            EditRevision::new(incoming),
            token_matches,
            text_changed,
            bytes,
        );
        let stale = !token_matches || incoming < current;
        let invalid = incoming > MAX_SAFE_REVISION || (incoming == current && text_changed);
        assert_eq!(result == EditDecision::Stale, stale);
        assert_eq!(result == EditDecision::InvalidRevision, !stale && invalid);
        assert_eq!(
            result == EditDecision::TooLarge,
            !stale && !invalid && bytes > MAX_FILE_BYTES
        );
        assert_eq!(
            result == EditDecision::Accept,
            !stale && !invalid && bytes <= MAX_FILE_BYTES
        );
        kani::cover!(result == EditDecision::Accept);
        kani::cover!(result == EditDecision::Stale);
        kani::cover!(result == EditDecision::InvalidRevision);
        kani::cover!(result == EditDecision::TooLarge);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn library_writes_only_untitled_nonempty_notes_into_an_existing_library() {
        let has_path: bool = kani::any();
        let has_library: bool = kani::any();
        let body_is_empty: bool = kani::any();
        let written = celestite_core::library_write(has_path, has_library, body_is_empty);
        assert_eq!(written, !has_path && has_library && !body_is_empty);
        kani::cover!(written);
        kani::cover!(has_path);
        kani::cover!(!has_path && !has_library);
        kani::cover!(!has_path && has_library && body_is_empty);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn definitions_append_once_and_never_retarget_a_label() {
        use celestite_core::{DefinitionAction, plan_definition};
        let defined: bool = kani::any();
        let same: bool = kani::any();
        let action = plan_definition(defined, same);
        assert_eq!(action == DefinitionAction::Append, !defined);
        assert_eq!(action == DefinitionAction::Keep, defined && same);
        assert_eq!(action == DefinitionAction::Refuse, defined && !same);
        kani::cover!(action == DefinitionAction::Append);
        kani::cover!(action == DefinitionAction::Keep);
        kani::cover!(action == DefinitionAction::Refuse);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn definition_separators_never_let_a_definition_interrupt_a_paragraph() {
        use celestite_core::{Separator, definition_separator};
        let is_empty: bool = kani::any();
        let ends_with_newline: bool = kani::any();
        let last_line_blank: bool = kani::any();
        let last_line_is_definition: bool = kani::any();
        let separator = definition_separator(
            is_empty,
            ends_with_newline,
            last_line_blank,
            last_line_is_definition,
        );
        let paragraph_open = !is_empty && !last_line_blank && !last_line_is_definition;
        assert_eq!(
            separator == Separator::BlankLine,
            paragraph_open && !ends_with_newline
        );
        assert_eq!(
            separator == Separator::Newline,
            (paragraph_open && ends_with_newline)
                || (!is_empty
                    && !ends_with_newline
                    && (last_line_blank || last_line_is_definition))
        );
        assert_eq!(
            separator == Separator::None,
            is_empty || (ends_with_newline && (last_line_blank || last_line_is_definition))
        );
        kani::cover!(separator == Separator::BlankLine);
        kani::cover!(separator == Separator::Newline);
        kani::cover!(separator == Separator::None && !is_empty);
        kani::cover!(is_empty);
    }

    #[kani::proof]
    #[kani::unwind(6)]
    fn labels_accept_exactly_balanced_nonempty_single_line_text() {
        use celestite_core::{LabelVerdict, label_verdict};
        const LENGTH: usize = 4;
        let bytes: [u8; LENGTH] = kani::any();
        let verdict = label_verdict(&bytes);
        let mut line_ending = false;
        let mut bracket = false;
        let mut content = false;
        let mut reserved = false;
        let mut index = 0;
        while index < LENGTH && !line_ending && !bracket && !reserved {
            let byte = bytes[index];
            if byte == b'\n' || byte == b'\r' {
                line_ending = true;
            } else if byte == b'[' || byte == b']' {
                bracket = true;
            } else if byte == b'\\' {
                if index + 1 < LENGTH && bytes[index + 1].is_ascii_punctuation() {
                    index += 1;
                }
                content = true;
            } else if byte == b' ' || byte == b'\t' {
            } else if byte == b'^' && !content {
                reserved = true;
            } else {
                content = true;
            }
            index += 1;
        }
        assert_eq!(verdict == LabelVerdict::LineEnding, line_ending);
        assert_eq!(verdict == LabelVerdict::UnescapedBracket, bracket);
        assert_eq!(verdict == LabelVerdict::Reserved, reserved);
        assert_eq!(
            verdict == LabelVerdict::Empty,
            !line_ending && !bracket && !reserved && !content
        );
        assert_eq!(
            verdict == LabelVerdict::Valid,
            !line_ending && !bracket && !reserved && content
        );
        assert_ne!(verdict, LabelVerdict::TooLong);
        kani::cover!(verdict == LabelVerdict::Valid);
        kani::cover!(verdict == LabelVerdict::Empty);
        kani::cover!(verdict == LabelVerdict::UnescapedBracket && bytes[0] == b'\\');
        kani::cover!(verdict == LabelVerdict::Reserved);
        kani::cover!(verdict == LabelVerdict::Valid && bytes[0] == b'\\' && bytes[1] == b']');
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn media_are_written_once_kept_when_identical_and_never_overwritten() {
        use celestite_core::{AssetStore, plan_asset_store};
        let exists: bool = kani::any();
        let same_content: bool = kani::any();
        let action = plan_asset_store(exists, same_content);
        assert_eq!(action == AssetStore::Write, !exists);
        assert_eq!(action == AssetStore::Keep, exists && same_content);
        assert_eq!(action == AssetStore::Refuse, exists && !same_content);
        kani::cover!(action == AssetStore::Write);
        kani::cover!(action == AssetStore::Keep);
        kani::cover!(action == AssetStore::Refuse);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn media_links_require_a_library_and_a_note_without_a_path_or_inside_it() {
        use celestite_core::media_link_allowed;
        let has_library: bool = kani::any();
        let has_path: bool = kani::any();
        let in_library: bool = kani::any();
        let allowed = media_link_allowed(has_library, has_path, in_library);
        assert_eq!(allowed, has_library && (!has_path || in_library));
        kani::cover!(allowed && !has_path);
        kani::cover!(allowed && has_path);
        kani::cover!(!allowed && has_library);
        kani::cover!(!allowed && !has_library);
    }

    #[cfg(feature = "negative-control")]
    #[kani::proof]
    #[kani::unwind(1)]
    fn rejects_a_false_definition_contract() {
        use celestite_core::{DefinitionAction, plan_definition};
        assert_eq!(
            plan_definition(true, false),
            DefinitionAction::Keep,
            "negative-control: a retargeted label must not be kept silently"
        );
    }

    #[cfg(feature = "negative-control")]
    #[kani::proof]
    #[kani::unwind(1)]
    fn rejects_a_false_write_contract() {
        assert_ne!(
            plan_write(
                EditRevision::new(0),
                EditRevision::new(0),
                DiskState::Unchanged
            ),
            WriteDecision::Write,
            "negative-control: a writable edit must not be rejected"
        );
    }
}
