use std::{
    error::Error,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use celestite_core::checked_byte_length;
use sha2::{Digest, Sha256};

const SOURCE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
const SOURCE_ROOTS: [&str; 10] = [
    "Cargo.toml",
    "Cargo.lock",
    ".cargo",
    "crates",
    "docs",
    "fuzz",
    "src-tauri",
    "verification",
    "xtask",
    ".dockerignore",
];
const EXCLUDED_DIRECTORIES: [&str; 3] = ["target", "gen", "node_modules"];

pub struct SourceSnapshot {
    directory: PathBuf,
    pub digest: String,
    pub files: usize,
}

fn files_under(
    root: &Path,
    relative: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let metadata = fs::symlink_metadata(root.join(relative))?;
    if metadata.is_symlink() {
        return Err(format!(
            "Proof inputs cannot contain symlinks: {}",
            relative.display()
        )
        .into());
    }
    if metadata.is_file() {
        files.push(relative.to_path_buf());
    } else if metadata.is_dir() {
        for entry in fs::read_dir(root.join(relative))? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && EXCLUDED_DIRECTORIES
                    .iter()
                    .any(|name| entry.file_name() == *name)
            {
                continue;
            }
            files_under(root, &relative.join(entry.file_name()), files)?;
        }
    } else {
        return Err(format!("Proof input is not a regular file: {}", relative.display()).into());
    }
    Ok(())
}

fn collect(
    root: &Path,
    roots: &[&str],
    destination: Option<&Path>,
    limit: usize,
) -> Result<(String, usize), Box<dyn Error>> {
    let mut files = Vec::new();
    for relative in roots {
        if let Some(destination) = destination
            && fs::symlink_metadata(root.join(relative))?.is_dir()
        {
            fs::create_dir_all(destination.join(relative))?;
        }
        files_under(root, Path::new(relative), &mut files)?;
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        return Err("The proof source snapshot is empty".into());
    }
    let mut digest = Sha256::new();
    let mut total = 0;
    for relative in &files {
        let remaining = limit.checked_sub(total).ok_or("Source budget exceeded")?;
        let bytes = read_bounded(&root.join(relative), remaining)?;
        total = checked_byte_length(total, bytes.len(), limit).ok_or("Source budget exceeded")?;
        let portable = relative
            .to_str()
            .ok_or("Proof inputs must have UTF-8 paths")?
            .replace('\\', "/");
        digest.update(u64::try_from(portable.len())?.to_le_bytes());
        digest.update(portable.as_bytes());
        digest.update(u64::try_from(bytes.len())?.to_le_bytes());
        digest.update(&bytes);
        if let Some(destination) = destination {
            let path = destination.join(relative);
            fs::create_dir_all(path.parent().ok_or("Missing snapshot parent")?)?;
            fs::write(path, bytes)?;
        }
    }
    Ok((format!("{:x}", digest.finalize()), files.len()))
}

pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    let read_limit = u64::try_from(limit)?
        .checked_add(1)
        .ok_or("Read limit overflow")?;
    fs::File::open(path)?
        .take(read_limit)
        .read_to_end(&mut bytes)?;
    checked_byte_length(0, bytes.len(), limit).ok_or("Input exceeds its byte budget")?;
    Ok(bytes)
}

impl SourceSnapshot {
    pub fn capture(root: &Path, parent: &Path) -> Result<Self, Box<dyn Error>> {
        let directory = tempfile::Builder::new()
            .prefix("source-")
            .tempdir_in(parent)?;
        let (digest, files) = collect(
            root,
            &SOURCE_ROOTS,
            Some(directory.path()),
            SOURCE_BUDGET_BYTES,
        )?;
        Ok(Self {
            directory: directory.keep(),
            digest,
            files,
        })
    }

    pub fn path(&self) -> &Path {
        &self.directory
    }

    pub fn matches(&self, root: &Path) -> Result<bool, Box<dyn Error>> {
        let current = collect(root, &SOURCE_ROOTS, None, SOURCE_BUDGET_BYTES)?;
        let frozen = collect(self.path(), &SOURCE_ROOTS, None, SOURCE_BUDGET_BYTES)?;
        Ok(current == (self.digest.clone(), self.files) && frozen == current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_paths_and_bytes_and_detects_mutation() -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let copy = tempfile::tempdir()?;
        fs::write(root.path().join("input"), b"original")?;
        let first = collect(
            root.path(),
            &["input"],
            Some(copy.path()),
            SOURCE_BUDGET_BYTES,
        )?;
        assert_eq!(
            first,
            collect(copy.path(), &["input"], None, SOURCE_BUDGET_BYTES)?
        );
        fs::write(root.path().join("input"), b"modified")?;
        assert_ne!(
            first,
            collect(root.path(), &["input"], None, SOURCE_BUDGET_BYTES)?
        );
        fs::rename(copy.path().join("input"), copy.path().join("renamed"))?;
        assert_ne!(
            first,
            collect(copy.path(), &["renamed"], None, SOURCE_BUDGET_BYTES)?
        );
        Ok(())
    }

    #[test]
    fn refuses_empty_and_oversized_inputs() -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        fs::write(root.path().join("input"), b"ab")?;
        assert!(collect(root.path(), &[], None, SOURCE_BUDGET_BYTES).is_err());
        assert!(collect(root.path(), &["input"], None, 1).is_err());
        assert!(read_bounded(&root.path().join("input"), 1).is_err());
        assert_eq!(read_bounded(&root.path().join("input"), 2)?, b"ab");
        Ok(())
    }

    #[test]
    fn snapshot_binding_refuses_changed_inputs_and_ignores_generated_targets()
    -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let evidence = tempfile::tempdir()?;
        for relative in SOURCE_ROOTS {
            match relative {
                ".cargo" | "crates" | "docs" | "fuzz" | "src-tauri" | "verification" | "xtask" => {
                    fs::create_dir(root.path().join(relative))?
                }
                _ => fs::write(root.path().join(relative), b"input")?,
            }
        }
        let snapshot = SourceSnapshot::capture(root.path(), evidence.path())?;
        assert!(snapshot.matches(root.path())?);
        fs::create_dir(root.path().join("verification/target"))?;
        fs::write(
            root.path().join("verification/target/generated"),
            b"generated",
        )?;
        assert!(snapshot.matches(root.path())?);
        fs::write(root.path().join("verification/proof.rs"), b"added")?;
        assert!(!snapshot.matches(root.path())?);
        fs::remove_file(root.path().join("verification/proof.rs"))?;
        assert!(snapshot.matches(root.path())?);
        fs::write(snapshot.path().join("Cargo.toml"), b"modified")?;
        assert!(!snapshot.matches(root.path())?);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_instead_of_reading_outside_the_snapshot() -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        std::os::unix::fs::symlink("/etc/passwd", root.path().join("input"))?;
        assert!(collect(root.path(), &["input"], None, SOURCE_BUDGET_BYTES).is_err());
        Ok(())
    }
}
