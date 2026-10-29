use std::{error::Error, fs, sync::Barrier};

use celestite_documents::Document;

#[test]
fn concurrent_readers_only_observe_complete_replacements() -> Result<(), Box<dyn Error>> {
    const ROUNDS: u64 = 64;
    const BLOCK_BYTES: usize = 64 * 1024;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("atomic.md");
    let first = "a".repeat(BLOCK_BYTES);
    let second = "b".repeat(BLOCK_BYTES);
    fs::write(&path, &first)?;
    let mut document = Document::open(&path)?;
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        let writer = scope.spawn(|| -> Result<(), String> {
            let mut outcome = Ok(());
            for revision in 1..=ROUNDS {
                let next = if revision % 2 == 0 { &first } else { &second };
                barrier.wait();
                if outcome.is_ok() {
                    outcome = document
                        .edit(&document.snapshot().token, revision, next.clone())
                        .and_then(|()| document.save())
                        .map_err(|error| error.to_string());
                }
                barrier.wait();
            }
            outcome
        });
        let mut observations = Vec::new();
        for revision in 1..=ROUNDS {
            barrier.wait();
            let during = fs::read(&path);
            barrier.wait();
            observations.push((during, fs::read(&path), revision));
        }
        writer.join().map_err(|_| "Atomic writer panicked")??;
        for (during, after, revision) in observations {
            let during = during?;
            assert!(during == first.as_bytes() || during == second.as_bytes());
            assert_eq!(
                after?,
                if revision % 2 == 0 {
                    first.as_bytes()
                } else {
                    second.as_bytes()
                }
            );
        }
        Ok(())
    })
}
