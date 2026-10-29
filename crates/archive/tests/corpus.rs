use celestite_archive::{NoteId, ObjectId};
#[path = "../../../fuzz/invariants.rs"]
mod invariants;
use std::{error::Error, fs, path::Path};

#[test]
fn every_identifier_corpus_case_replays_through_the_public_parsers() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/corpus/archive_ids");
    let mut count = 0;
    let mut accepted_notes = 0;
    let mut accepted_objects = 0;
    for seed in fs::read_dir(root)? {
        let bytes = fs::read(seed?.path())?;
        invariants::check(&bytes)?;
        count += 1;
        if let Ok(source) = std::str::from_utf8(&bytes) {
            for text in [source, source.trim_end_matches(['\r', '\n'])] {
                if let Ok(note) = text.parse::<NoteId>() {
                    accepted_notes += 1;
                    assert_eq!(note.to_string().parse::<NoteId>()?, note);
                    assert_eq!(
                        serde_json::from_str::<NoteId>(&serde_json::to_string(&note)?)?,
                        note
                    );
                }
                if let Ok(object) = text.parse::<ObjectId>() {
                    accepted_objects += 1;
                    assert_eq!(object.to_string().parse::<ObjectId>()?, object);
                    assert_eq!(
                        serde_json::from_str::<ObjectId>(&serde_json::to_string(&object)?)?,
                        object
                    );
                }
            }
        }
    }
    assert!(count > 0 && accepted_notes > 0 && accepted_objects > 0);
    Ok(())
}
