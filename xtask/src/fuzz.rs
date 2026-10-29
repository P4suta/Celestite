use celestite_assurance_core::{FuzzEvidence, admit_fuzz_run};
use std::{error::Error, fs, path::Path, process::Stdio};

const RUNS: usize = 4096;
const CORPUS: &str = "fuzz/corpus/archive_ids";
const ARTIFACT_DIRECTORY: &str = "fuzz/artifacts/archive_ids";

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join(super::EVIDENCE_ROOT))?;
    let evidence = tempfile::Builder::new()
        .prefix("fuzz-")
        .tempdir_in(root.join(super::EVIDENCE_ROOT))?
        .keep();
    let source = super::SourceSnapshot::capture(root, &evidence)?;
    fs::create_dir_all(source.path().join(ARTIFACT_DIRECTORY))?;
    let seeds = fs::read_dir(source.path().join(CORPUS))?.collect::<Result<Vec<_>, _>>()?;
    if seeds.is_empty()
        || seeds
            .iter()
            .any(|seed| !seed.file_type().is_ok_and(|kind| kind.is_file()))
    {
        return Err("A nonempty regular-file corpus is required".into());
    }
    let output = evidence.join("output");
    fs::create_dir(&output)?;
    fs::create_dir(output.join("corpus"))?;
    let image = super::build_image(source.path(), "verification/fuzz.Dockerfile")?;
    let status = super::bounded_command(source.path(), &image, &output)
        .args([
            "cargo",
            "+nightly-2026-10-25",
            "fuzz",
            "run",
            "archive_ids",
            "--sanitizer",
            "none",
            "/evidence/corpus",
            "/workspace/fuzz/corpus/archive_ids",
            "--",
            "-runs=4096",
            "-seed=4701",
            "-max_len=8192",
            "-artifact_prefix=/evidence/",
        ])
        .stdout(Stdio::from(fs::File::create(evidence.join("stdout.log"))?))
        .stderr(Stdio::from(fs::File::create(evidence.join("stderr.log"))?))
        .status()?;
    let log = fs::read_to_string(evidence.join("stderr.log"))?;
    let completed = log
        .lines()
        .any(|line| line.starts_with(&format!("Done {RUNS} runs in ")));
    let facts = FuzzEvidence {
        exit: status.code(),
        expected_runs: RUNS,
        completed_runs: if completed { RUNS } else { 0 },
        corpus_files: seeds.len(),
        source_matches: source.matches(root)?,
    };
    let admitted = admit_fuzz_run(facts);
    fs::write(
        evidence.join("receipt.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"sourceSha256":source.digest,"image":image,"runs":facts.completed_runs,"corpusFiles":facts.corpus_files,"admitted":admitted}),
        )?,
    )?;
    if !admitted {
        return Err(format!(
            "Fuzz run refused or inconclusive; inspect {}",
            evidence.display()
        )
        .into());
    }
    println!(
        "replayed {} corpus files and completed {RUNS} seeded fuzz executions; evidence: {}",
        seeds.len(),
        evidence.display()
    );
    Ok(())
}
