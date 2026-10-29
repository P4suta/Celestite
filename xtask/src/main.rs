use std::{
    error::Error,
    fs,
    path::Path,
    process::{Command, Stdio},
};

use celestite_assurance_core::{ProofAdmission, admit_proof_run};
use serde::Serialize;
use tempfile::NamedTempFile;

mod fuzz;
mod lean;
mod mutations;
mod proof_report;
mod source_snapshot;

use proof_report::{EXPECTED_HARNESSES, ProofReport, inventory};
use source_snapshot::SourceSnapshot;

const IMAGE_PREFIX: &str = "sha256:";
const SHA256_HEX_LENGTH: usize = 64;
const MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;
const EVIDENCE_ROOT: &str = "target/assurance";
const CONTAINER_PLATFORM: &str = "linux/amd64";
const CONTAINER_MEMORY: &str = "2048m";
const CONTAINER_CPUS: &str = "2";
const CONTAINER_PIDS: &str = "256";
const PROOF_TIMEOUT: &str = "900s";
const TARGET_TMPFS: &str = "/proof-target:rw,exec,nosuid,size=1073741824";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt<'a> {
    source_sha256: &'a str,
    source_files: usize,
    image: &'a str,
    exit_code: Option<i32>,
    proofs: usize,
    negative_controls: usize,
    verdict: String,
}

fn image_identity(text: &str) -> bool {
    text.strip_prefix(IMAGE_PREFIX).is_some_and(|digest| {
        digest.len() == SHA256_HEX_LENGTH && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn supported_logs(stdout: &str, stderr: &str) -> bool {
    const UNSOUND_DIAGNOSTICS: [&str; 3] = [
        "not enough arguments",
        "inserting non-deterministic value",
        "Kani does not support",
    ];
    [stdout, stderr].iter().all(|log| {
        !UNSOUND_DIAGNOSTICS
            .iter()
            .any(|message| log.contains(message))
    })
}

fn build_image(root: &Path, dockerfile: &str) -> Result<String, Box<dyn Error>> {
    let identity = NamedTempFile::new()?;
    let status = Command::new("docker")
        .args([
            "build",
            "--platform",
            CONTAINER_PLATFORM,
            "--file",
            dockerfile,
            "--iidfile",
        ])
        .arg(identity.path())
        .arg(".")
        .current_dir(root)
        .status()?;
    if !status.success() {
        return Err(format!("Proof image build failed: {status}").into());
    }
    let image = fs::read_to_string(identity.path())?;
    let image = image.trim().to_owned();
    if !image_identity(&image) {
        return Err("Docker did not return an immutable image identity".into());
    }
    Ok(image)
}

fn bounded_command(source: &Path, image: &str, output: &Path) -> Command {
    let mut command = Command::new("docker");
    command
        .args([
            "run",
            "--rm",
            "--platform",
            CONTAINER_PLATFORM,
            "--memory",
            CONTAINER_MEMORY,
            "--memory-swap",
            CONTAINER_MEMORY,
            "--cpus",
            CONTAINER_CPUS,
            "--pids-limit",
            CONTAINER_PIDS,
            "--network",
            "none",
            "--env",
            "CARGO_NET_OFFLINE=true",
            "--env",
            "CARGO_TARGET_DIR=/proof-target",
            "--mount",
        ])
        .arg(format!(
            "type=bind,source={},target=/workspace,readonly",
            source.display()
        ))
        .args(["--tmpfs", TARGET_TMPFS, "--mount"])
        .arg(format!(
            "type=bind,source={},target=/evidence",
            output.display()
        ))
        .args(["--workdir", "/workspace", image, "timeout", PROOF_TIMEOUT]);
    command
}

fn proofs(root: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join(EVIDENCE_ROOT))?;
    let evidence = tempfile::Builder::new()
        .prefix("proofs-")
        .tempdir_in(root.join(EVIDENCE_ROOT))?
        .keep();
    println!("proof evidence: {}", evidence.display());
    let source = SourceSnapshot::capture(root, &evidence)?;
    let output = evidence.join("output");
    fs::create_dir(&output)?;
    let image = build_image(source.path(), "verification/Dockerfile")?;
    println!("proof image: {image}; frozen source: {}", source.digest);
    let status = bounded_command(source.path(), &image, &output)
        .args([
            "cargo",
            "kani",
            "--manifest-path",
            "verification/Cargo.toml",
            "--target-dir",
            "/proof-target",
            "--all-features",
            "-Z",
            "unstable-options",
            "-Z",
            "loop-contracts",
            "--export-json",
            "/evidence/kani.json",
        ])
        .stdout(Stdio::from(fs::File::create(evidence.join("stdout.log"))?))
        .stderr(Stdio::from(fs::File::create(evidence.join("stderr.log"))?))
        .status()?;
    let report: ProofReport = serde_json::from_slice(&source_snapshot::read_bounded(
        &output.join("kani.json"),
        MAX_REPORT_BYTES,
    )?)?;
    let expected = inventory(&EXPECTED_HARNESSES);
    let mut audit = report.audit(&EXPECTED_HARNESSES, source.matches(root)?);
    let stdout = source_snapshot::read_bounded(&evidence.join("stdout.log"), MAX_REPORT_BYTES)?;
    let stderr = source_snapshot::read_bounded(&evidence.join("stderr.log"), MAX_REPORT_BYTES)?;
    audit.properties_valid &=
        supported_logs(std::str::from_utf8(&stdout)?, std::str::from_utf8(&stderr)?);
    let admission = admit_proof_run(expected, status.code(), audit);
    let receipt = Receipt {
        source_sha256: &source.digest,
        source_files: source.files,
        image: &image,
        exit_code: status.code(),
        proofs: expected.proofs,
        negative_controls: expected.negative_controls,
        verdict: format!("{admission:?}"),
    };
    fs::write(
        evidence.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    if admission != ProofAdmission::Admit {
        return Err(format!(
            "Proof run refused: {admission:?}; inspect {}",
            evidence.display()
        )
        .into());
    }
    println!(
        "verified {} proofs and {} exact negative controls",
        expected.proofs, expected.negative_controls
    );
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Missing project root")?;
    match (arguments.next().as_deref(), arguments.next()) {
        (Some("proofs"), None) => proofs(root),
        (Some("lean"), None) => lean::run(root),
        (Some("fuzz"), None) => fuzz::run(root),
        (Some("mutations-list"), None) => mutations::run(root, true),
        (Some("mutations"), None) => mutations::run(root, false),
        _ => Err("Usage: cargo run -p xtask -- proofs|lean|fuzz|mutations-list|mutations".into()),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_image_identity_requires_the_entire_digest() {
        assert!(image_identity(&format!(
            "{IMAGE_PREFIX}{}",
            "a".repeat(SHA256_HEX_LENGTH)
        )));
        for invalid in ["", "latest", "sha256:", "sha256:abc"] {
            assert!(!image_identity(invalid));
        }
        assert!(!image_identity(&format!(
            "{IMAGE_PREFIX}{}",
            "z".repeat(SHA256_HEX_LENGTH)
        )));
    }

    #[test]
    fn substituted_arguments_never_admit_an_otherwise_successful_report() {
        assert!(supported_logs(
            "VERIFICATION:- SUCCESSFUL",
            "warning: use of an unstable feature"
        ));
        for diagnostic in [
            "not enough arguments",
            "inserting non-deterministic value",
            "Kani does not support",
        ] {
            assert!(!supported_logs(diagnostic, ""));
            assert!(!supported_logs("", diagnostic));
        }
    }
}
