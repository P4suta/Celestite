use std::{collections::BTreeSet, error::Error, fs, path::Path, process::Stdio};

use celestite_assurance_core::{MutationAdmission, MutationEvidence, admit_mutation_run};
use serde::{Deserialize, Serialize};

use crate::{
    EVIDENCE_ROOT, MAX_REPORT_BYTES, bounded_command, build_image,
    source_snapshot::{SourceSnapshot, read_bounded},
};

const MUTANTS_VERSION: &str = "27.1.0";
const MAX_MUTANTS: usize = 384;
const RUST_FAILURE_EXIT_CODE: i32 = 101;
const SELECTION: [&str; 18] = [
    "--package",
    "celestite-archive-core",
    "--test-package",
    "celestite-archive-core",
    "--file",
    "crates/archive-core/src/lib.rs",
    "--package",
    "celestite-core",
    "--package",
    "celestite-assurance-core",
    "--test-package",
    "celestite-core",
    "--test-package",
    "celestite-assurance-core",
    "--file",
    "crates/note-core/src/lib.rs",
    "--file",
    "crates/assurance-core/src/lib.rs",
];

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
struct Mutant {
    name: String,
    package: String,
    file: String,
}

#[derive(Deserialize)]
enum Scenario {
    Baseline,
    Mutant(Mutant),
}

#[derive(Deserialize, Eq, PartialEq)]
enum Summary {
    Success,
    CaughtMutant,
    MissedMutant,
    Unviable,
    Failure,
    Timeout,
}

#[derive(Deserialize, Eq, PartialEq)]
enum Phase {
    Build,
    Test,
    Check,
}

#[derive(Deserialize, Eq, PartialEq)]
enum Exit {
    Success,
    Failure(i32),
    Timeout,
    Signalled(i32),
    Other,
}

#[derive(Deserialize)]
struct PhaseResult {
    phase: Phase,
    process_status: Exit,
}

#[derive(Deserialize)]
struct Outcome {
    scenario: Scenario,
    summary: Summary,
    phase_results: Vec<PhaseResult>,
}

#[derive(Deserialize)]
struct Outcomes {
    outcomes: Vec<Outcome>,
    total_mutants: usize,
    missed: usize,
    caught: usize,
    timeout: usize,
    unviable: usize,
    success: usize,
    end_time: Option<String>,
    cargo_mutants_version: String,
}

impl Outcomes {
    fn audit(&self, expected: &[Mutant], source_matches: bool) -> MutationEvidence {
        let required: BTreeSet<_> = expected.iter().collect();
        let mut observed = BTreeSet::new();
        let mut baseline = 0;
        let mut caught = 0;
        let mut unviable = 0;
        let mut properties_valid = true;
        let mut baseline_passed = true;
        for outcome in &self.outcomes {
            let phases: Vec<_> = outcome
                .phase_results
                .iter()
                .map(|result| (&result.phase, &result.process_status))
                .collect();
            match &outcome.scenario {
                Scenario::Baseline => {
                    baseline += 1;
                    if outcome.summary != Summary::Success
                        || phases
                            != [
                                (&Phase::Build, &Exit::Success),
                                (&Phase::Test, &Exit::Success),
                            ]
                    {
                        baseline_passed = false;
                    }
                }
                Scenario::Mutant(mutant) => {
                    if !observed.insert(mutant) {
                        properties_valid = false;
                    }
                    match outcome.summary {
                        Summary::CaughtMutant
                            if phases
                                == [
                                    (&Phase::Build, &Exit::Success),
                                    (&Phase::Test, &Exit::Failure(RUST_FAILURE_EXIT_CODE)),
                                ] =>
                        {
                            caught += 1
                        }
                        Summary::Unviable
                            if phases
                                == [(&Phase::Build, &Exit::Failure(RUST_FAILURE_EXIT_CODE))] =>
                        {
                            unviable += 1
                        }
                        _ => properties_valid = false,
                    }
                }
            }
        }
        MutationEvidence {
            discovered: expected.len(),
            executed: self.total_mutants,
            caught: self.caught,
            unviable: self.unviable,
            missed: self.missed,
            timeouts: self.timeout,
            completed: self.end_time.is_some(),
            baseline_passed: baseline_passed && baseline == 1,
            inventory_matches: required.len() == expected.len() && observed == required,
            properties_valid: properties_valid
                && self.success == 0
                && self.caught == caught
                && self.unviable == unviable
                && self.cargo_mutants_version == MUTANTS_VERSION,
            source_matches,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt<'a> {
    source_sha256: &'a str,
    image: &'a str,
    discovered: usize,
    caught: usize,
    unviable: usize,
    admitted: bool,
}

pub fn run(root: &Path, list_only: bool) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join(EVIDENCE_ROOT))?;
    let evidence = tempfile::Builder::new()
        .prefix("mutations-")
        .tempdir_in(root.join(EVIDENCE_ROOT))?
        .keep();
    println!("mutation evidence: {}", evidence.display());
    let source = SourceSnapshot::capture(root, &evidence)?;
    let image = build_image(source.path(), "verification/mutation.Dockerfile")?;
    let output = evidence.join("output");
    fs::create_dir(&output)?;
    let status = bounded_command(source.path(), &image, &output)
        .args(["cargo", "mutants"])
        .args(SELECTION)
        .args(["--list", "--json", "--no-shuffle"])
        .stdout(Stdio::from(fs::File::create(
            evidence.join("discovered.json"),
        )?))
        .stderr(Stdio::from(fs::File::create(
            evidence.join("discovery.log"),
        )?))
        .status()?;
    if !status.success() {
        return Err(format!(
            "Mutation discovery failed: {status}; inspect {}",
            evidence.display()
        )
        .into());
    }
    let discovered: Vec<Mutant> = serde_json::from_slice(&read_bounded(
        &evidence.join("discovered.json"),
        MAX_REPORT_BYTES,
    )?)?;
    if discovered.is_empty() || discovered.len() > MAX_MUTANTS {
        return Err(format!(
            "Mutation inventory is empty or exceeds the bounded scope ({MAX_MUTANTS}): {}",
            discovered.len()
        )
        .into());
    }
    println!(
        "discovered {} mutants across three pure cores",
        discovered.len()
    );
    if list_only {
        return Ok(());
    }
    let status = bounded_command(source.path(), &image, &output)
        .args(["cargo", "mutants"])
        .args(SELECTION)
        .args([
            "--jobs",
            "1",
            "--no-shuffle",
            "--timeout",
            "30",
            "--build-timeout",
            "60",
            "--output",
            "/evidence",
        ])
        .stdout(Stdio::from(fs::File::create(evidence.join("stdout.log"))?))
        .stderr(Stdio::from(fs::File::create(evidence.join("stderr.log"))?))
        .status()?;
    let outcomes: Outcomes = serde_json::from_slice(&read_bounded(
        &output.join("mutants.out/outcomes.json"),
        MAX_REPORT_BYTES,
    )?)?;
    let admitted = admit_mutation_run(
        status.code(),
        outcomes.audit(&discovered, source.matches(root)?),
    ) == MutationAdmission::Admit;
    fs::write(
        evidence.join("receipt.json"),
        serde_json::to_vec_pretty(&Receipt {
            source_sha256: &source.digest,
            image: &image,
            discovered: discovered.len(),
            caught: outcomes.caught,
            unviable: outcomes.unviable,
            admitted,
        })?,
    )?;
    if !admitted {
        return Err(format!(
            "Mutation run refused or inconclusive; inspect {}",
            evidence.display()
        )
        .into());
    }
    println!(
        "mutation check: {} caught; {} rejected by the compiler; none missed or timed out",
        outcomes.caught, outcomes.unviable
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const MISSED_LIMITS: &[u8] =
        include_bytes!("../tests/fixtures/mutations-with-missed-limits.json");

    fn classified_partition() -> Result<(Value, Vec<Mutant>), Box<dyn Error>> {
        let mut value: Value = serde_json::from_slice(MISSED_LIMITS)?;
        value["outcomes"]
            .as_array_mut()
            .ok_or("Missing outcomes")?
            .retain(|outcome| outcome["summary"] != "MissedMutant");
        let parsed: Outcomes = serde_json::from_value(value.clone())?;
        let expected: Vec<_> = parsed
            .outcomes
            .into_iter()
            .filter_map(|outcome| match outcome.scenario {
                Scenario::Mutant(mutant) => Some(mutant),
                Scenario::Baseline => None,
            })
            .collect();
        value["total_mutants"] = json!(expected.len());
        value["missed"] = json!(0);
        Ok((value, expected))
    }

    fn admits(value: &Value, expected: &[Mutant]) -> Result<bool, Box<dyn Error>> {
        let parsed: Outcomes = serde_json::from_value(value.clone())?;
        Ok(admit_mutation_run(Some(0), parsed.audit(expected, true)) == MutationAdmission::Admit)
    }

    #[test]
    fn admits_the_classified_partition_but_refuses_the_actual_missed_mutations()
    -> Result<(), Box<dyn Error>> {
        let (classified, expected) = classified_partition()?;
        assert!(admits(&classified, &expected)?);
        let actual: Value = serde_json::from_slice(MISSED_LIMITS)?;
        assert!(!admits(&actual, &expected)?);
        Ok(())
    }

    #[test]
    fn rejects_incomplete_inconsistent_stale_or_wrong_tool_results() -> Result<(), Box<dyn Error>> {
        let (valid, expected) = classified_partition()?;
        for (field, invalid) in [
            ("total_mutants", json!(0)),
            ("caught", json!(0)),
            ("unviable", json!(0)),
            ("missed", json!(1)),
            ("timeout", json!(1)),
            ("success", json!(1)),
            ("end_time", Value::Null),
            ("cargo_mutants_version", json!("unsupported")),
        ] {
            let mut broken = valid.clone();
            broken[field] = invalid;
            assert!(!admits(&broken, &expected)?, "accepted invalid {field}");
        }
        let parsed: Outcomes = serde_json::from_value(valid)?;
        assert_eq!(
            admit_mutation_run(Some(0), parsed.audit(&expected, false)),
            MutationAdmission::Refuse
        );
        for exit in [
            None,
            Some(2),
            Some(3),
            Some(4),
            Some(124),
            Some(125),
            Some(137),
            Some(143),
        ] {
            assert_eq!(
                admit_mutation_run(exit, parsed.audit(&expected, true)),
                MutationAdmission::Refuse
            );
        }
        Ok(())
    }

    #[test]
    fn rejects_wrong_failure_phases_missing_scenarios_and_duplicates() -> Result<(), Box<dyn Error>>
    {
        let (valid, expected) = classified_partition()?;
        for summary in ["CaughtMutant", "Unviable", "Success"] {
            let mut broken = valid.clone();
            let outcome = broken["outcomes"]
                .as_array_mut()
                .ok_or("Missing outcomes")?
                .iter_mut()
                .find(|outcome| outcome["summary"] == summary)
                .ok_or("Missing classified outcome")?;
            outcome["phase_results"][0]["process_status"] = json!("Timeout");
            assert!(!admits(&broken, &expected)?);
        }
        let mut broken = valid.clone();
        let removed = broken["outcomes"]
            .as_array_mut()
            .ok_or("Missing outcomes")?
            .pop()
            .ok_or("Empty outcomes")?;
        assert!(!admits(&broken, &expected)?);
        broken["outcomes"]
            .as_array_mut()
            .ok_or("Missing outcomes")?
            .extend([removed.clone(), removed]);
        assert!(!admits(&broken, &expected)?);
        Ok(())
    }
}
