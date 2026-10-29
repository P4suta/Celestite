use std::{
    error::Error,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

use celestite_documents::{Document, DocumentError, NoteStore};

const CHILD_ROOT_VARIABLE: &str = "CELESTITE_CHECKPOINT_TEST_ROOT";
const CHECKPOINT_READY: &str = "CELESTITE_CHECKPOINT_DURABLE";
const CHILD_DEADLINE: Duration = Duration::from_secs(10);
const DRAFT: &str = "# Recover after process termination\n日本語 🦀\n";

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            if let Err(error) = self.0.kill() {
                eprintln!("Could not stop checkpoint worker: {error}");
            }
            if let Err(error) = self.0.wait() {
                eprintln!("Could not reap checkpoint worker: {error}");
            }
        }
    }
}

#[test]
fn checkpoint_worker() -> Result<(), Box<dyn Error>> {
    let Ok(root) = std::env::var(CHILD_ROOT_VARIABLE) else {
        return Ok(());
    };
    let store = NoteStore::new(root.into())?;
    let mut document = Document::new();
    document.edit(&document.snapshot().token, 1, DRAFT.into())?;
    store.checkpoint(&document)?;
    println!("{CHECKPOINT_READY}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Err("The checkpoint worker must be terminated by its parent".into())
}

#[test]
fn killed_process_releases_ownership_and_restores_its_acknowledged_checkpoint()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let mut worker = Worker(
        Command::new(std::env::current_exe()?)
            .args(["--exact", "checkpoint_worker", "--nocapture"])
            .env(CHILD_ROOT_VARIABLE, directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let stdout = worker.0.stdout.take().ok_or("Missing worker stdout")?;
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let result = BufReader::new(stdout)
            .lines()
            .find_map(|line| match line {
                Ok(line) if line.contains(CHECKPOINT_READY) => Some(Ok(())),
                Ok(_) => None,
                Err(error) => Some(Err(error.to_string())),
            })
            .unwrap_or_else(|| Err("Worker exited before acknowledging its checkpoint".into()));
        let _ = sender.send(result);
    });
    let ready = receiver.recv_timeout(CHILD_DEADLINE);
    if !matches!(ready, Ok(Ok(()))) {
        worker.0.kill()?;
        worker.0.wait()?;
        reader.join().map_err(|_| "Worker output reader panicked")?;
        return Err(format!("Checkpoint handshake failed or is inconclusive: {ready:?}").into());
    }
    reader.join().map_err(|_| "Worker output reader panicked")?;
    assert!(matches!(
        NoteStore::new(directory.path().to_path_buf()),
        Err(DocumentError::AlreadyRunning)
    ));
    worker.0.kill()?;
    assert!(!worker.0.wait()?.success());
    let store = NoteStore::new(directory.path().to_path_buf())?;
    let recovered = store.recover()?;
    assert!(recovered.warnings.is_empty());
    assert_eq!(recovered.documents.len(), 1);
    let note = recovered
        .documents
        .first()
        .ok_or("Missing recovered note")?;
    assert_eq!(note.snapshot().text, DRAFT);
    assert_eq!(note.snapshot().revision, 1);
    assert!(note.has_unsaved_changes());
    Ok(())
}
