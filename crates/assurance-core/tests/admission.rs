use celestite_assurance_core::{
    ObservedCounts, ProofAdmission, ProofInventory, RunEvidence, admit_proof_run,
};

#[test]
fn fuzz_admission_checks_actual_completion_nonempty_corpus_and_current_source() {
    use celestite_assurance_core::{FuzzEvidence, admit_fuzz_run};
    for exit in [None, Some(0), Some(1), Some(124)] {
        for expected in [0, 1, 2, usize::MAX] {
            for completed in [0, 1, 2, usize::MAX] {
                for corpus in [0, 1, 2, usize::MAX] {
                    for current in [false, true] {
                        let facts = FuzzEvidence {
                            exit,
                            expected_runs: expected,
                            completed_runs: completed,
                            corpus_files: corpus,
                            source_matches: current,
                        };
                        let success = exit == Some(0)
                            && expected != 0
                            && completed == expected
                            && corpus != 0
                            && current;
                        assert_eq!(admit_fuzz_run(facts), success);
                    }
                }
            }
        }
    }
}

#[test]
fn mutation_admission_requires_current_complete_nonempty_classified_results() {
    use celestite_assurance_core::{MutationAdmission, MutationEvidence, admit_mutation_run};
    let valid = MutationEvidence {
        discovered: 3,
        executed: 3,
        caught: 2,
        unviable: 1,
        missed: 0,
        timeouts: 0,
        completed: true,
        baseline_passed: true,
        inventory_matches: true,
        properties_valid: true,
        source_matches: true,
    };
    assert_eq!(admit_mutation_run(Some(0), valid), MutationAdmission::Admit);
    for invalid in [
        MutationEvidence {
            discovered: 0,
            ..valid
        },
        MutationEvidence {
            executed: 0,
            ..valid
        },
        MutationEvidence { caught: 0, ..valid },
        MutationEvidence {
            unviable: 0,
            ..valid
        },
        MutationEvidence {
            caught: usize::MAX,
            ..valid
        },
        MutationEvidence { missed: 1, ..valid },
        MutationEvidence {
            timeouts: 1,
            ..valid
        },
        MutationEvidence {
            completed: false,
            ..valid
        },
        MutationEvidence {
            baseline_passed: false,
            ..valid
        },
        MutationEvidence {
            inventory_matches: false,
            ..valid
        },
        MutationEvidence {
            properties_valid: false,
            ..valid
        },
        MutationEvidence {
            source_matches: false,
            ..valid
        },
    ] {
        assert_eq!(
            admit_mutation_run(Some(0), invalid),
            MutationAdmission::Refuse
        );
    }
    for exit in [
        None,
        Some(1),
        Some(2),
        Some(124),
        Some(125),
        Some(137),
        Some(143),
    ] {
        assert_eq!(admit_mutation_run(exit, valid), MutationAdmission::Refuse);
    }
}

#[test]
fn mutation_counts_cover_empty_all_unviable_overflowed_and_partial_partitions() {
    use celestite_assurance_core::{MutationAdmission, MutationEvidence, admit_mutation_run};
    const COUNTS: [usize; 6] = [0, 1, 2, 3, 4, usize::MAX];
    for discovered in COUNTS {
        for executed in COUNTS {
            for caught in COUNTS {
                for unviable in COUNTS {
                    for missed in [0, 1] {
                        for timeouts in [0, 1] {
                            let evidence = MutationEvidence {
                                discovered,
                                executed,
                                caught,
                                unviable,
                                missed,
                                timeouts,
                                completed: true,
                                baseline_passed: true,
                                inventory_matches: true,
                                properties_valid: true,
                                source_matches: true,
                            };
                            let valid = caught > 0
                                && discovered > 0
                                && executed == discovered
                                && caught as u128 + unviable as u128 == discovered as u128
                                && missed == 0
                                && timeouts == 0;
                            assert_eq!(
                                admit_mutation_run(Some(0), evidence) == MutationAdmission::Admit,
                                valid
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn every_boolean_evidence_combination_and_process_exit_is_checked() {
    let expected = ProofInventory {
        proofs: 3,
        negative_controls: 1,
    };
    for completed in [false, true] {
        for inventory_matches in [false, true] {
            for properties_valid in [false, true] {
                for source_matches in [false, true] {
                    for exit in [
                        None,
                        Some(0),
                        Some(1),
                        Some(2),
                        Some(124),
                        Some(125),
                        Some(137),
                        Some(143),
                    ] {
                        let evidence = RunEvidence {
                            counts: ObservedCounts {
                                total: 4,
                                executed: 4,
                                passed: 3,
                                failed: 1,
                            },
                            completed,
                            inventory_matches,
                            properties_valid,
                            source_matches,
                        };
                        assert_eq!(
                            admit_proof_run(expected, exit, evidence) == ProofAdmission::Admit,
                            completed
                                && inventory_matches
                                && properties_valid
                                && source_matches
                                && exit == Some(1)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn empty_overflowed_partial_or_inconsistent_inventories_are_refused() {
    const COUNTS: [usize; 6] = [0, 1, 2, 3, 4, usize::MAX];
    for proofs in COUNTS {
        for negative_controls in COUNTS {
            for total in COUNTS {
                for executed in COUNTS {
                    for passed in COUNTS {
                        for failed in COUNTS {
                            let expected = ProofInventory {
                                proofs,
                                negative_controls,
                            };
                            let evidence = RunEvidence {
                                counts: ObservedCounts {
                                    total,
                                    executed,
                                    passed,
                                    failed,
                                },
                                completed: true,
                                inventory_matches: true,
                                properties_valid: true,
                                source_matches: true,
                            };
                            let valid = proofs > 0
                                && negative_controls > 0
                                && proofs.checked_add(negative_controls) == Some(total)
                                && executed == total
                                && passed == proofs
                                && failed == negative_controls;
                            assert_eq!(
                                admit_proof_run(expected, Some(1), evidence)
                                    == ProofAdmission::Admit,
                                valid
                            );
                        }
                    }
                }
            }
        }
    }
}
