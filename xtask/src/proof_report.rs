use std::collections::BTreeSet;

use celestite_assurance_core::{ObservedCounts, ProofInventory, RunEvidence};
use serde::Deserialize;

const EXPORT_SCHEMA: &str = "1.0";
const KANI_VERSION: &str = "0.68.0";
const PROOF_TARGET: &str = "x86_64-unknown-linux-gnu";
const VERIFICATION_CRATE: &str = "celestite_verification";
const CBMC_VERSION: &str = "6.11.0 (cbmc-6.11.0)";

#[derive(Clone, Copy)]
pub enum Expectation {
    Proof {
        covers: usize,
    },
    InductiveProof {
        covers: usize,
        progress_assertion: &'static str,
    },
    Counterexample {
        assertion: &'static str,
    },
}

#[derive(Clone, Copy)]
pub struct HarnessExpectation {
    pub name: &'static str,
    pub expectation: Expectation,
}

pub const EXPECTED_HARNESSES: [HarnessExpectation; 29] = [
    HarnessExpectation {
        name: "proofs::library_writes_only_untitled_nonempty_notes_into_an_existing_library",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::definitions_append_once_and_never_retarget_a_label",
        expectation: Expectation::Proof { covers: 3 },
    },
    HarnessExpectation {
        name: "proofs::definition_separators_never_let_a_definition_interrupt_a_paragraph",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::labels_accept_exactly_balanced_nonempty_single_line_text",
        expectation: Expectation::Proof { covers: 5 },
    },
    HarnessExpectation {
        name: "proofs::rejects_a_false_definition_contract",
        expectation: Expectation::Counterexample {
            assertion: "negative-control: a retargeted label must not be kept silently",
        },
    },
    HarnessExpectation {
        name: "proofs::inferred_https_never_overrides_local_file_intent",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::navigation_requires_the_exact_source_and_a_bounded_position",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::editor_capture_acknowledges_only_its_selected_unconflicted_source",
        expectation: Expectation::Proof { covers: 3 },
    },
    HarnessExpectation {
        name: "proofs::archive_object_id_lengths_cover_every_machine_integer",
        expectation: Expectation::Proof { covers: 3 },
    },
    HarnessExpectation {
        name: "proofs::archive_object_ids_accept_exactly_the_complete_fixed_width_representation",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::fuzz_admission_requires_completed_nonempty_current_runs",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::archive_merge_preserves_every_concurrent_change",
        expectation: Expectation::Proof { covers: 3 },
    },
    HarnessExpectation {
        name: "proofs::archive_publication_requires_synced_objects_and_an_unchanged_head",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::archive_repairs_require_matching_sources_and_valid_byte_spans",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::archive_graph_extensions_require_fresh_identity_and_complete_parents",
        expectation: Expectation::Proof { covers: 3 },
    },
    HarnessExpectation {
        name: "proofs::application_exit_waits_for_live_notes_and_pending_cleanup",
        expectation: Expectation::Proof { covers: 3 },
    },
    HarnessExpectation {
        name: "proofs::mutation_admission_rejects_missing_stale_or_interrupted_results",
        expectation: Expectation::Proof { covers: 6 },
    },
    HarnessExpectation {
        name: "proofs::rejects_a_false_mutation_admission_contract",
        expectation: Expectation::Counterexample {
            assertion: "negative-control: interrupted mutations must not be admitted",
        },
    },
    HarnessExpectation {
        name: "proofs::close_requires_the_exact_current_note_and_revision",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::encoded_lengths_match_mathematical_sizes_without_overflow",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::recovery_metadata_never_constructs_an_invalid_document_state",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::revision_transitions_preserve_bounds_and_saved_progress",
        expectation: Expectation::Proof { covers: 7 },
    },
    HarnessExpectation {
        name: "proofs::arbitrary_finite_edit_save_sequences_preserve_revision_invariants",
        expectation: Expectation::InductiveProof {
            covers: 3,
            progress_assertion: "revision trace makes progress",
        },
    },
    HarnessExpectation {
        name: "proofs::write_decision_covers_every_revision_and_disk_state",
        expectation: Expectation::Proof { covers: 3 },
    },
    HarnessExpectation {
        name: "proofs::open_target_preserves_every_nonempty_or_named_document",
        expectation: Expectation::Proof { covers: 2 },
    },
    HarnessExpectation {
        name: "proofs::edit_validation_covers_tokens_revisions_and_sizes",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::proof_admission_rejects_every_incomplete_or_unbound_run",
        expectation: Expectation::Proof { covers: 6 },
    },
    HarnessExpectation {
        name: "proofs::byte_budgets_reject_overflow_and_every_oversized_append",
        expectation: Expectation::Proof { covers: 4 },
    },
    HarnessExpectation {
        name: "proofs::rejects_a_false_write_contract",
        expectation: Expectation::Counterexample {
            assertion: "negative-control: a writable edit must not be rejected",
        },
    },
];

pub fn inventory(expected: &[HarnessExpectation]) -> ProofInventory {
    ProofInventory {
        proofs: expected
            .iter()
            .filter(|harness| {
                matches!(
                    harness.expectation,
                    Expectation::Proof { .. } | Expectation::InductiveProof { .. }
                )
            })
            .count(),
        negative_controls: expected
            .iter()
            .filter(|harness| matches!(harness.expectation, Expectation::Counterexample { .. }))
            .count(),
    }
}

#[derive(Deserialize)]
struct Metadata {
    version: String,
    kani_version: String,
    target: String,
}

#[derive(Deserialize)]
struct Tools {
    kani: String,
    cbmc: String,
}

#[derive(Deserialize)]
struct HarnessAttributes {
    kind: String,
    should_panic: bool,
}

#[derive(Deserialize)]
struct HarnessMetadata {
    pretty_name: String,
    crate_name: String,
    attributes: HarnessAttributes,
}

#[derive(Deserialize)]
struct Summary {
    total_harnesses: usize,
    executed: usize,
    status: String,
    successful: usize,
    failed: usize,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
enum CheckStatus {
    Success,
    Failure,
    Unreachable,
    Undetermined,
    Satisfied,
    Unsatisfiable,
    SolverError,
    Covered,
    Uncovered,
}

#[derive(Deserialize)]
struct Check {
    id: usize,
    function: String,
    status: CheckStatus,
    description: String,
    category: String,
}

#[derive(Deserialize)]
struct HarnessResult {
    harness_id: String,
    status: CheckStatus,
    checks: Vec<Check>,
}

impl HarnessResult {
    fn matches(&self, expected: HarnessExpectation) -> bool {
        let ids: BTreeSet<_> = self.checks.iter().map(|check| check.id).collect();
        if ids.len() != self.checks.len()
            || self.checks.is_empty()
            || self
                .checks
                .iter()
                .any(|check| check.description.is_empty() || check.category.is_empty())
        {
            return false;
        }
        let assertions: Vec<_> = self
            .checks
            .iter()
            .filter(|check| check.category == "assertion" && check.function == expected.name)
            .collect();
        let covers: Vec<_> = self
            .checks
            .iter()
            .filter(|check| check.category == "cover")
            .collect();
        match expected.expectation {
            Expectation::Proof { covers: required }
            | Expectation::InductiveProof {
                covers: required, ..
            } => {
                let induction_valid = match expected.expectation {
                    Expectation::InductiveProof {
                        progress_assertion, ..
                    } => {
                        [
                            "loop_invariant_base",
                            "loop_invariant_step",
                            "loop_assigns",
                            "loop_step_unwinding",
                        ]
                        .iter()
                        .all(|category| {
                            self.checks.iter().any(|check| {
                                check.function == expected.name
                                    && check.category == *category
                                    && check.status == CheckStatus::Success
                            })
                        }) && assertions
                            .iter()
                            .any(|check| check.description == format!("\"{progress_assertion}\""))
                    }
                    Expectation::Proof { .. } => true,
                    Expectation::Counterexample { .. } => false,
                };
                self.status == CheckStatus::Success
                    && induction_valid
                    && required > 0
                    && !assertions.is_empty()
                    && assertions
                        .iter()
                        .all(|check| check.status == CheckStatus::Success)
                    && covers.len() == required
                    && covers
                        .iter()
                        .all(|check| check.status == CheckStatus::Satisfied)
                    && self.checks.iter().all(|check| match check.status {
                        CheckStatus::Success | CheckStatus::Unreachable => {
                            check.category != "cover"
                        }
                        CheckStatus::Satisfied => check.category == "cover",
                        _ => false,
                    })
            }
            Expectation::Counterexample { assertion } => {
                let failures: Vec<_> = self
                    .checks
                    .iter()
                    .filter(|check| check.status == CheckStatus::Failure)
                    .collect();
                self.status == CheckStatus::Failure
                    && covers.is_empty()
                    && failures.len() == 1
                    && failures.first().is_some_and(|check| {
                        check.category == "assertion"
                            && check.function == expected.name
                            && check.description == format!("\"{assertion}\"")
                    })
                    && self.checks.iter().all(|check| {
                        matches!(
                            check.status,
                            CheckStatus::Success | CheckStatus::Failure | CheckStatus::Unreachable
                        )
                    })
            }
        }
    }
}

#[derive(Deserialize, Eq, PartialEq)]
struct PropertyCounts {
    total_properties: usize,
    passed: usize,
    failed: usize,
    unreachable: usize,
    undetermined: usize,
    solver_error: usize,
    satisfied: usize,
    unsatisfiable: usize,
    covered: usize,
    uncovered: usize,
}

impl PropertyCounts {
    fn from_checks(checks: &[Check]) -> Self {
        let count = |status| checks.iter().filter(|check| check.status == status).count();
        Self {
            total_properties: checks.len(),
            passed: count(CheckStatus::Success),
            failed: count(CheckStatus::Failure),
            unreachable: count(CheckStatus::Unreachable),
            undetermined: count(CheckStatus::Undetermined),
            solver_error: count(CheckStatus::SolverError),
            satisfied: count(CheckStatus::Satisfied),
            unsatisfiable: count(CheckStatus::Unsatisfiable),
            covered: count(CheckStatus::Covered),
            uncovered: count(CheckStatus::Uncovered),
        }
    }
}

#[derive(Deserialize)]
struct PropertySummary {
    harness_id: String,
    property_details: PropertyCounts,
}

#[derive(Deserialize)]
struct ErrorDetails {
    harness_id: String,
    has_errors: bool,
    error_type: Option<String>,
    failed_properties_type: Option<String>,
    exit_status: Option<String>,
}

impl ErrorDetails {
    fn matches(&self, expected: Expectation) -> bool {
        match expected {
            Expectation::Proof { .. } | Expectation::InductiveProof { .. } => {
                !self.has_errors
                    && self.error_type.is_none()
                    && self.failed_properties_type.is_none()
                    && self.exit_status.is_none()
            }
            Expectation::Counterexample { .. } => {
                self.has_errors
                    && self.error_type.as_deref() == Some("assertion_failure")
                    && self.failed_properties_type.as_deref() == Some("PanicsOnly")
                    && self.exit_status.as_deref() == Some("properties_failed")
            }
        }
    }
}

#[derive(Deserialize)]
struct VerificationResults {
    summary: Summary,
    results: Vec<HarnessResult>,
}

#[derive(Deserialize)]
pub struct ProofReport {
    metadata: Metadata,
    tools: Tools,
    harness_metadata: Vec<HarnessMetadata>,
    error_details: Vec<ErrorDetails>,
    property_details: Vec<PropertySummary>,
    verification_results: VerificationResults,
}

impl ProofReport {
    pub fn audit(&self, expected: &[HarnessExpectation], source_matches: bool) -> RunEvidence {
        let required: BTreeSet<_> = expected.iter().map(|harness| harness.name).collect();
        let matching_inventory = |names: Vec<&str>| {
            names.len() == required.len()
                && names.iter().copied().collect::<BTreeSet<_>>() == required
        };
        let inventory_matches = required.len() == expected.len()
            && matching_inventory(
                self.harness_metadata
                    .iter()
                    .map(|harness| harness.pretty_name.as_str())
                    .collect(),
            )
            && matching_inventory(
                self.verification_results
                    .results
                    .iter()
                    .map(|result| result.harness_id.as_str())
                    .collect(),
            )
            && matching_inventory(
                self.error_details
                    .iter()
                    .map(|error| error.harness_id.as_str())
                    .collect(),
            )
            && matching_inventory(
                self.property_details
                    .iter()
                    .map(|summary| summary.harness_id.as_str())
                    .collect(),
            );
        let properties_valid = self.metadata.version == EXPORT_SCHEMA
            && self.metadata.kani_version == KANI_VERSION
            && self.metadata.target == PROOF_TARGET
            && self.tools.kani == KANI_VERSION
            && self.tools.cbmc == CBMC_VERSION
            && self.harness_metadata.iter().all(|harness| {
                harness.crate_name == VERIFICATION_CRATE
                    && harness.attributes.kind == "Proof"
                    && !harness.attributes.should_panic
            })
            && expected.iter().all(|expected| {
                self.verification_results
                    .results
                    .iter()
                    .find(|result| result.harness_id == expected.name)
                    .is_some_and(|result| {
                        result.matches(*expected)
                            && self.property_details.iter().any(|summary| {
                                summary.harness_id == expected.name
                                    && summary.property_details
                                        == PropertyCounts::from_checks(&result.checks)
                            })
                    })
                    && self.error_details.iter().any(|error| {
                        error.harness_id == expected.name && error.matches(expected.expectation)
                    })
            });
        let summary = &self.verification_results.summary;
        RunEvidence {
            counts: ObservedCounts {
                total: summary.total_harnesses,
                executed: summary.executed,
                passed: summary.successful,
                failed: summary.failed,
            },
            completed: summary.status == "completed",
            inventory_matches,
            properties_valid,
            source_matches,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use celestite_assurance_core::{ProofAdmission, admit_proof_run};
    use serde_json::Value;

    use super::{EXPECTED_HARNESSES, HarnessExpectation, ProofReport, inventory};

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/kani.json");

    fn fixture_inventory() -> Vec<HarnessExpectation> {
        EXPECTED_HARNESSES.to_vec()
    }

    fn admitted(value: &Value) -> Result<bool, Box<dyn Error>> {
        let report: ProofReport = serde_json::from_value(value.clone())?;
        let expected = fixture_inventory();
        Ok(
            admit_proof_run(inventory(&expected), Some(1), report.audit(&expected, true))
                == ProofAdmission::Admit,
        )
    }

    #[test]
    fn admits_the_actual_complete_export_from_the_pinned_verifier() -> Result<(), Box<dyn Error>> {
        assert!(admitted(&serde_json::from_slice(FIXTURE)?)?);
        Ok(())
    }

    #[test]
    fn rejects_empty_partial_or_wrong_summary_counts() -> Result<(), Box<dyn Error>> {
        for (field, value) in [
            ("total_harnesses", serde_json::json!(0)),
            ("executed", serde_json::json!(3)),
            ("successful", serde_json::json!(2)),
            ("failed", serde_json::json!(0)),
            ("status", serde_json::json!("incomplete")),
        ] {
            let mut report: Value = serde_json::from_slice(FIXTURE)?;
            report["verification_results"]["summary"][field] = value;
            assert!(!admitted(&report)?, "accepted an invalid {field}");
        }
        Ok(())
    }

    #[test]
    fn rejects_missing_or_duplicated_harness_evidence() -> Result<(), Box<dyn Error>> {
        for field in ["harness_metadata", "error_details", "property_details"] {
            let mut report: Value = serde_json::from_slice(FIXTURE)?;
            let entries = report[field]
                .as_array_mut()
                .ok_or("Missing evidence array")?;
            let entry = entries.pop().ok_or("Empty evidence array")?;
            assert!(!admitted(&report)?);
            let entries = report[field]
                .as_array_mut()
                .ok_or("Missing evidence array")?;
            entries.push(entry.clone());
            entries.push(entry);
            assert!(!admitted(&report)?);
        }
        Ok(())
    }

    #[test]
    fn a_different_failed_property_is_not_the_expected_counterexample() -> Result<(), Box<dyn Error>>
    {
        let mut report: Value = serde_json::from_slice(FIXTURE)?;
        let results = report["verification_results"]["results"]
            .as_array_mut()
            .ok_or("Missing results")?;
        let negative = results
            .iter_mut()
            .find(|result| result["harness_id"] == "proofs::rejects_a_false_write_contract")
            .ok_or("Missing negative control")?;
        let checks = negative["checks"].as_array_mut().ok_or("Missing checks")?;
        let failure = checks
            .iter_mut()
            .find(|check| check["status"] == "Failure")
            .ok_or("Missing failed property")?;
        failure["status"] = serde_json::json!("Success");
        let other = checks
            .iter_mut()
            .find(|check| check["category"] == "pointer_dereference")
            .ok_or("Missing pointer check")?;
        other["status"] = serde_json::json!("Failure");
        assert!(!admitted(&report)?);
        Ok(())
    }

    #[test]
    fn rejects_a_changed_tool_version_or_expected_panic() -> Result<(), Box<dyn Error>> {
        let mut report: Value = serde_json::from_slice(FIXTURE)?;
        report["metadata"]["kani_version"] = serde_json::json!("unsupported");
        assert!(!admitted(&report)?);
        let mut report: Value = serde_json::from_slice(FIXTURE)?;
        report["harness_metadata"][0]["attributes"]["should_panic"] = serde_json::json!(true);
        assert!(!admitted(&report)?);
        Ok(())
    }

    #[test]
    fn refuses_every_unresolved_or_unreachable_owned_property() -> Result<(), Box<dyn Error>> {
        for category in ["assertion", "cover"] {
            for status in [
                "Failure",
                "Unreachable",
                "Undetermined",
                "Unsatisfiable",
                "SolverError",
                "Uncovered",
            ] {
                let mut report: Value = serde_json::from_slice(FIXTURE)?;
                let result = report["verification_results"]["results"]
                    .as_array_mut()
                    .ok_or("Missing results")?
                    .iter_mut()
                    .find(|result| result["status"] == "Success")
                    .ok_or("Missing proof")?;
                let name = result["harness_id"].clone();
                let check = result["checks"]
                    .as_array_mut()
                    .ok_or("Missing checks")?
                    .iter_mut()
                    .find(|check| check["category"] == category && check["function"] == name)
                    .ok_or("Missing owned property")?;
                check["status"] = serde_json::json!(status);
                assert!(
                    !admitted(&report)?,
                    "accepted {category} with status {status}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn refuses_missing_duplicate_or_unregistered_results() -> Result<(), Box<dyn Error>> {
        let mut report: Value = serde_json::from_slice(FIXTURE)?;
        let results = report["verification_results"]["results"]
            .as_array_mut()
            .ok_or("Missing results")?;
        let result = results.pop().ok_or("Empty results")?;
        assert!(!admitted(&report)?);
        report["verification_results"]["results"]
            .as_array_mut()
            .ok_or("Missing results")?
            .extend([result.clone(), result]);
        assert!(!admitted(&report)?);
        let mut report: Value = serde_json::from_slice(FIXTURE)?;
        report["verification_results"]["results"][0]["harness_id"] =
            serde_json::json!("proofs::unregistered");
        assert!(!admitted(&report)?);
        let parsed: ProofReport = serde_json::from_slice(FIXTURE)?;
        let expected = fixture_inventory();
        assert_ne!(
            admit_proof_run(
                inventory(&expected),
                Some(1),
                parsed.audit(&expected, false)
            ),
            ProofAdmission::Admit
        );
        for exit in [None, Some(0), Some(124), Some(125), Some(137), Some(143)] {
            assert_ne!(
                admit_proof_run(inventory(&expected), exit, parsed.audit(&expected, true)),
                ProofAdmission::Admit
            );
        }
        Ok(())
    }

    #[test]
    fn induction_requires_its_base_preservation_assigns_and_progress_properties()
    -> Result<(), Box<dyn Error>> {
        let report: ProofReport = serde_json::from_slice(FIXTURE)?;
        let expected = EXPECTED_HARNESSES
            .iter()
            .find(|harness| {
                matches!(
                    harness.expectation,
                    super::Expectation::InductiveProof { .. }
                )
            })
            .ok_or("Missing induction obligation")?;
        let original = report
            .verification_results
            .results
            .into_iter()
            .find(|result| result.harness_id == expected.name)
            .ok_or("Missing induction result")?;
        assert!(original.matches(*expected));
        for category in [
            "loop_invariant_base",
            "loop_invariant_step",
            "loop_assigns",
            "loop_step_unwinding",
        ] {
            let mut broken: super::HarnessResult = serde_json::from_value(
                serde_json::from_slice::<Value>(FIXTURE)?["verification_results"]["results"]
                    .as_array()
                    .ok_or("Missing results")?
                    .iter()
                    .find(|result| result["harness_id"] == expected.name)
                    .ok_or("Missing induction result")?
                    .clone(),
            )?;
            broken.checks.retain(|check| check.category != category);
            assert!(
                !broken.matches(*expected),
                "accepted missing induction {category}"
            );
        }
        let mut no_progress = original;
        no_progress
            .checks
            .retain(|check| check.description != "\"revision trace makes progress\"");
        assert!(!no_progress.matches(*expected));
        Ok(())
    }
}
