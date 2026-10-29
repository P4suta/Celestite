use celestite_archive::{NoteId, Record, Vault};
use std::{error::Error, fs};

#[test]
fn restoration_never_modifies_its_input_bag_by_writing_inside_it() -> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let vault = Vault::init(&area.path().join("vault"))?;
    vault.capture(NoteId::new(), "note.md", b"preserve", None, None)?;
    let bag = area.path().join("bag");
    vault.export_bag(&bag)?;
    assert!(Vault::restore_bag(&bag, &bag.join("data/restored")).is_err());
    Vault::verify_bag(&bag)?;
    Ok(())
}

#[test]
fn duplicate_json_map_keys_and_normalized_identity_aliases_are_refused() {
    let id = "019a1234-5678-7000-8000-000000000001";
    for alias in [id.to_owned(), id.replace('-', "")] {
        let source = format!(
            r#"{{"type":"shard","notes":{{"{id}":{{"state":"deleted"}},"{alias}":{{"state":"deleted"}}}}}}"#
        );
        assert!(serde_json::from_str::<Record>(&source).is_err());
    }
    let source = format!(
        r#"{{"type":"tree","shards":{{"01":"sha256:{}","01":"sha256:{}"}}}}"#,
        "a".repeat(64),
        "b".repeat(64)
    );
    assert!(serde_json::from_str::<Record>(&source).is_err());
}

#[test]
fn existing_output_is_not_touched_when_export_is_refused() -> Result<(), Box<dyn Error>> {
    let area = tempfile::tempdir()?;
    let vault = Vault::init(&area.path().join("vault"))?;
    vault.capture(NoteId::new(), "note.md", b"preserve", None, None)?;
    let destination = area.path().join("existing");
    fs::create_dir(&destination)?;
    fs::write(destination.join("keep"), "keep")?;
    assert!(vault.export_bag(&destination).is_err());
    assert_eq!(fs::read_to_string(destination.join("keep"))?, "keep");
    Ok(())
}

fn store(vault: &Vault, record: &Record) -> Result<celestite_archive::ObjectId, Box<dyn Error>> {
    let bytes = serde_json::to_vec(record)?;
    let id = celestite_archive::ObjectId::of(&bytes);
    let path = vault.object_path(&id);
    fs::create_dir_all(path.parent().ok_or("No object parent")?)?;
    fs::write(path, bytes)?;
    Ok(id)
}

#[test]
fn empty_shards_cannot_hide_invalid_prefixes_and_capture_times_must_be_decimal()
-> Result<(), Box<dyn Error>> {
    use std::collections::BTreeMap;
    let area = tempfile::tempdir()?;
    let vault = Vault::init(area.path())?;
    let shard = store(
        &vault,
        &Record::Shard {
            notes: BTreeMap::new(),
        },
    )?;
    for prefix in ["", "00", "0", "gg", "AB", "000"] {
        for time in ["0", "", "-1", "1.5", "yesterday"] {
            let tree = store(
                &vault,
                &Record::Tree {
                    shards: BTreeMap::from([(prefix.to_owned(), shard.clone())]),
                },
            )?;
            let snapshot = store(
                &vault,
                &Record::Snapshot {
                    parents: Vec::new(),
                    tree,
                    device: NoteId::new(),
                    captured_at: time.to_owned(),
                },
            )?;
            assert_eq!(
                vault.verify(&snapshot).is_ok(),
                prefix == "00" && time == "0",
                "prefix {prefix:?}, time {time:?}"
            );
        }
    }
    Ok(())
}
