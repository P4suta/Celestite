use celestite_archive::{Entry, NoteId, ObjectId, Vault};
use std::{collections::BTreeMap, error::Error, path::Path};

const USAGE: &str = "Usage: celestite-archive init VAULT | import VAULT DIRECTORY | heads VAULT | verify VAULT [SNAPSHOT] | checkout VAULT DESTINATION [SNAPSHOT] | export VAULT BAG | restore BAG VAULT | ingest VAULT BAG | merge VAULT SNAPSHOT | delete VAULT NOTE-ID | resolve VAULT NOTE-ID RECORD-ID-or-deleted | search VAULT TEXT | reindex VAULT";

fn snapshot(vault: &Vault, argument: Option<&str>) -> Result<ObjectId, Box<dyn Error>> {
    match argument {
        Some(value) => Ok(value.parse()?),
        None => Ok(vault
            .current()?
            .ok_or("The archive has no current snapshot")?),
    }
}
fn print(value: &impl serde::Serialize) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<_> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["--help"] => println!("{USAGE}"),
        ["init", location] => {
            Vault::init(Path::new(location))?;
        }
        ["restore", location, destination] => {
            let vault = Vault::restore_bag(Path::new(location), Path::new(destination))?;
            print(&vault.heads()?)?;
        }
        ["import", location, input] => {
            let vault = Vault::open(Path::new(location))?;
            print(&vault.import_directory(Path::new(input), vault.current()?)?)?;
        }
        ["heads", location] => {
            print(&Vault::open(Path::new(location))?.heads()?)?;
        }
        ["verify", location, tail @ ..] if tail.len() <= 1 => {
            let vault = Vault::open(Path::new(location))?;
            let roots = if let Some(root) = tail.first() {
                vec![root.parse()?]
            } else {
                vault.heads()?
            };
            for root in roots {
                print(&vault.verify(&root)?)?;
            }
        }
        ["checkout", location, destination, tail @ ..] if tail.len() <= 1 => {
            let vault = Vault::open(Path::new(location))?;
            print(&vault.checkout(
                &snapshot(&vault, tail.first().copied())?,
                Path::new(destination),
            )?)?;
        }
        ["export", location, destination] => {
            Vault::open(Path::new(location))?.export_bag(Path::new(destination))?;
        }
        ["ingest", location, input] => {
            print(&Vault::open(Path::new(location))?.ingest_bag(Path::new(input))?)?;
        }
        ["merge", location, incoming] => {
            print(&Vault::open(Path::new(location))?.merge_snapshot(&incoming.parse()?)?)?;
        }
        ["delete", location, note] => {
            let vault = Vault::open(Path::new(location))?;
            let id: NoteId = note.parse()?;
            print(&vault.commit_notes(vault.current()?, BTreeMap::from([(id, Entry::Deleted)]))?)?;
        }
        ["resolve", location, note, selection] => {
            let vault = Vault::open(Path::new(location))?;
            let selected = if *selection == "deleted" {
                None
            } else {
                Some(selection.parse()?)
            };
            print(&vault.resolve(note.parse()?, selected)?)?;
        }
        ["search", location, text] => {
            let vault = Vault::open(Path::new(location))?;
            print(&vault.search(&snapshot(&vault, None)?, text)?)?;
        }
        ["reindex", location] => {
            let vault = Vault::open(Path::new(location))?;
            vault.rebuild_index(&snapshot(&vault, None)?)?;
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
