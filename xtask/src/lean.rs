use celestite_archive_core::{MergeChoice, admit_extension, merge_choice};
use std::{error::Error, fmt::Write as _, fs, path::Path, process::Stdio};

const THEOREMS: [&str; 5] = [
    "Archive.extension_preserves_order",
    "Archive.ancestry_decreases",
    "Archive.history_has_no_cycles",
    "Archive.append_preserves_prior_edges",
    "Archive.conflict_preserves_both",
];
const AXIOMS: [&str; 3] = ["propext", "Classical.choice", "Quot.sound"];
const LEAN_VERSION: &str = "4.35.0";
const LEAN_THEOREM_COUNT: usize = 7;

fn vectors() -> Result<String, std::fmt::Error> {
    let mut source = String::from(
        "import Archive\n\nnamespace Archive.Refinement\n\ndef extensionVectors : List (Bool × Bool × Bool × Bool) := [\n",
    );
    let mut merges = String::new();
    for fresh in [false, true] {
        for known in [false, true] {
            for complete in [false, true] {
                writeln!(
                    source,
                    "  ({fresh}, {known}, {complete}, {}),",
                    admit_extension(fresh, known, complete)
                )?;
                let outcome = match merge_choice(fresh, known, complete) {
                    MergeChoice::Shared => "shared",
                    MergeChoice::Left => "left",
                    MergeChoice::Right => "right",
                    MergeChoice::Conflict => "conflict",
                };
                writeln!(merges, "  ({fresh}, {known}, {complete}, .{outcome}),")?;
            }
        }
    }
    source.push_str("]\n\ndef mergeVectors : List (Bool × Bool × Bool × Choice) := [\n");
    source.push_str(&merges);
    source.push_str("]\n\ntheorem extension_refines_rust : extensionVectors.all (fun (a,b,c,result) => admitExtension a b c == result) = true := by decide\ntheorem merge_refines_rust : mergeVectors.all (fun (a,b,c,result) => mergeChoice a b c == result) = true := by decide\n\n#print axioms extension_refines_rust\n#print axioms merge_refines_rust\nend Archive.Refinement\n");
    Ok(source)
}

fn axiom_reports(text: &str, expected: &[&str]) -> bool {
    let lines: Vec<_> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.len() != expected.len() {
        return false;
    }
    expected.iter().all(|name| {
        let prefix = format!("'{name}' ");
        let reports: Vec<_> = lines
            .iter()
            .filter_map(|line| line.strip_prefix(&prefix))
            .collect();
        if reports.len() != 1 {
            return false;
        }
        let report = reports[0];
        report == "does not depend on any axioms"
            || report
                .strip_prefix("depends on axioms: [")
                .and_then(|value| value.strip_suffix(']'))
                .is_some_and(|axioms| {
                    axioms
                        .split(',')
                        .all(|axiom| AXIOMS.contains(&axiom.trim()))
                })
    })
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join(super::EVIDENCE_ROOT))?;
    let evidence = tempfile::Builder::new()
        .prefix("lean-")
        .tempdir_in(root.join(super::EVIDENCE_ROOT))?
        .keep();
    let source = super::SourceSnapshot::capture(root, &evidence)?;
    let output = evidence.join("output");
    fs::create_dir(&output)?;
    fs::write(output.join("Refinement.lean"), vectors()?)?;
    let image = super::build_image(source.path(), "verification/lean.Dockerfile")?;
    let version = super::bounded_command(source.path(), &image, &output)
        .args(["lean", "--version"])
        .output()?;
    if !version.status.success() || !String::from_utf8_lossy(&version.stdout).contains(LEAN_VERSION)
    {
        return Err("Unexpected Lean release".into());
    }
    for (module, input, expected) in [
        (
            "Archive",
            "/workspace/verification/Archive.lean",
            THEOREMS.as_slice(),
        ),
        (
            "Refinement",
            "/evidence/Refinement.lean",
            &[
                "Archive.Refinement.extension_refines_rust",
                "Archive.Refinement.merge_refines_rust",
            ],
        ),
    ] {
        let log = evidence.join(format!("{module}-axioms.log"));
        let status = super::bounded_command(source.path(), &image, &output)
            .args([
                "lean",
                "-DwarningAsError=true",
                "--root",
                if module == "Archive" {
                    "/workspace/verification"
                } else {
                    "/evidence"
                },
                "-o",
            ])
            .arg(format!("/evidence/{module}.olean"))
            .arg(input)
            .stdout(Stdio::from(fs::File::create(&log)?))
            .stderr(Stdio::from(fs::File::create(
                evidence.join(format!("{module}-stderr.log")),
            )?))
            .status()?;
        if !status.success() || !axiom_reports(&fs::read_to_string(log)?, expected) {
            return Err(format!("Lean obligations refused; inspect {}", evidence.display()).into());
        }
        let replay = super::bounded_command(source.path(), &image, &output)
            .args(["leanchecker", "--fresh", module])
            .output()?;
        fs::write(
            evidence.join(format!("{module}-replay.log")),
            [&replay.stdout[..], &replay.stderr[..]].concat(),
        )?;
        if !replay.status.success() {
            return Err("Fresh kernel replay failed".into());
        }
    }
    if !source.matches(root)? {
        return Err("Lean source changed during verification".into());
    }
    fs::write(
        evidence.join("receipt.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"sourceSha256":source.digest,"image":image,"theorems":LEAN_THEOREM_COUNT,"freshKernelReplay":true,"verdict":"Admit"}),
        )?,
    )?;
    println!(
        "verified 5 graph/retention theorems and 2 complete Boolean Rust refinements; evidence: {}",
        evidence.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn axiom_reports_refuse_missing_extra_duplicate_or_untrusted_results() {
        let expected = ["Archive.test"];
        let valid = "'Archive.test' does not depend on any axioms\n";
        assert!(axiom_reports(valid, &expected));
        for invalid in [
            "",
            "'Archive.test' depends on axioms: [sorryAx]",
            "'Archive.test' depends on axioms: [Lean.ofReduceBool]",
            "'Archive.other' does not depend on any axioms",
        ] {
            assert!(!axiom_reports(invalid, &expected));
        }
        assert!(!axiom_reports(&format!("{valid}{valid}"), &expected));
    }
}
