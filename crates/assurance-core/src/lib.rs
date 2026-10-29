#![no_std]
#![deny(clippy::panic, clippy::indexing_slicing)]

#[derive(Clone, Copy, Debug)]
pub struct FuzzEvidence {
    pub exit: Option<i32>,
    pub expected_runs: usize,
    pub completed_runs: usize,
    pub corpus_files: usize,
    pub source_matches: bool,
}

pub const fn admit_fuzz_run(evidence: FuzzEvidence) -> bool {
    matches!(evidence.exit, Some(0))
        && evidence.expected_runs > 0
        && evidence.completed_runs == evidence.expected_runs
        && evidence.corpus_files > 0
        && evidence.source_matches
}

pub const COUNTEREXAMPLE_EXIT_CODE: i32 = 1;
pub const SUCCESS_EXIT_CODE: i32 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MutationEvidence {
    pub discovered: usize,
    pub executed: usize,
    pub caught: usize,
    pub unviable: usize,
    pub missed: usize,
    pub timeouts: usize,
    pub completed: bool,
    pub baseline_passed: bool,
    pub inventory_matches: bool,
    pub properties_valid: bool,
    pub source_matches: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationAdmission {
    Admit,
    Refuse,
}

pub const fn admit_mutation_run(
    exit_code: Option<i32>,
    evidence: MutationEvidence,
) -> MutationAdmission {
    if !matches!(exit_code, Some(SUCCESS_EXIT_CODE))
        || evidence.discovered == 0
        || evidence.caught == 0
        || evidence.executed != evidence.discovered
        || evidence.missed != 0
        || evidence.timeouts != 0
        || !evidence.completed
        || !evidence.baseline_passed
        || !evidence.inventory_matches
        || !evidence.properties_valid
        || !evidence.source_matches
    {
        return MutationAdmission::Refuse;
    }
    match evidence.caught.checked_add(evidence.unviable) {
        Some(total) if total == evidence.discovered => MutationAdmission::Admit,
        _ => MutationAdmission::Refuse,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProofInventory {
    pub proofs: usize,
    pub negative_controls: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedCounts {
    pub total: usize,
    pub executed: usize,
    pub passed: usize,
    pub failed: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunEvidence {
    pub counts: ObservedCounts,
    pub completed: bool,
    pub inventory_matches: bool,
    pub properties_valid: bool,
    pub source_matches: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProofRefusal {
    EmptyInventory,
    InventoryOverflow,
    UnexpectedExit,
    IncompleteRun,
    CountMismatch,
    InventoryMismatch,
    InvalidProperties,
    SourceChanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProofAdmission {
    Admit,
    Refuse(ProofRefusal),
}

pub const fn admit_proof_run(
    expected: ProofInventory,
    exit_code: Option<i32>,
    evidence: RunEvidence,
) -> ProofAdmission {
    if expected.proofs == 0 || expected.negative_controls == 0 {
        return ProofAdmission::Refuse(ProofRefusal::EmptyInventory);
    }
    let Some(total) = expected.proofs.checked_add(expected.negative_controls) else {
        return ProofAdmission::Refuse(ProofRefusal::InventoryOverflow);
    };
    if !matches!(exit_code, Some(COUNTEREXAMPLE_EXIT_CODE)) {
        return ProofAdmission::Refuse(ProofRefusal::UnexpectedExit);
    }
    if !evidence.completed {
        return ProofAdmission::Refuse(ProofRefusal::IncompleteRun);
    }
    if evidence.counts.total != total
        || evidence.counts.executed != total
        || evidence.counts.passed != expected.proofs
        || evidence.counts.failed != expected.negative_controls
    {
        return ProofAdmission::Refuse(ProofRefusal::CountMismatch);
    }
    if !evidence.inventory_matches {
        return ProofAdmission::Refuse(ProofRefusal::InventoryMismatch);
    }
    if !evidence.properties_valid {
        return ProofAdmission::Refuse(ProofRefusal::InvalidProperties);
    }
    if !evidence.source_matches {
        return ProofAdmission::Refuse(ProofRefusal::SourceChanged);
    }
    ProofAdmission::Admit
}
