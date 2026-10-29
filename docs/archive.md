# Archive operations

Celestite captures named imports, recovered drafts, and checkpoints into `archive/` inside its native application-data directory.
Standalone source files retain their existing format.
Library notes are captured with their bare `UUID.md` filename as the logical path and resolve relative links against the library root.
Managed checkout adds UUID identity metadata and ordinary relative destinations, and writes the library layout: `UUID.md` files beside an `assets/` folder.
Importing a library root with `import` captures the same layout from the standalone command.
The recovery journal is independent and is retained when archive capture fails.

Use `mise x -- just archive --help` for the standalone command surface.
The CLI does not require Tauri, a webview, or an existing SQLite index.

## Capture and read

```sh
mise x -- just archive init "./my archive"
mise x -- just archive import "./my archive" "./my notes"
mise x -- just archive heads "./my archive"
mise x -- just archive verify "./my archive"
mise x -- just archive checkout "./my archive" "./portable notes"
mise x -- just archive search "./my archive" "search text"
```

Import leaves source files unchanged and captures local attachments referenced by supported Markdown destinations.
External URLs remain URLs and are not fetched.
File targets outside the selected root, missing targets, unsupported destination spans, and ambiguous path reuse remain diagnostics.
Checkout writes a fresh destination and never overwrites an existing folder.
Its `checkout.json` records the snapshot, counts, and unresolved links.
Use an explicit snapshot argument to checkout or verify a historical version.

## Preserve and restore

```sh
mise x -- just archive export "./my archive" "./archive bag"
mise x -- just archive restore "./archive bag" "./restored archive"
mise x -- just archive verify "./restored archive"
```

The bag contains complete reachable history, source originals, captured media, the public format, and device references.
It excludes device selection, ownership locks, and the disposable search index.
Restore validates the payload inventory, tag inventory, byte digests, format, and object graph before creating the destination.
A failed operation reports failure and may leave its new output folder for inspection; existing folders are refused.
Keep an independent healthy copy and exercise restoration into an empty environment.

## Offline branches

```sh
mise x -- just archive ingest "./my archive" "./other device bag"
mise x -- just archive resolve "./my archive" NOTE-UUID RECORD-OBJECT-ID
```

Ingest verifies and copies immutable objects before merging published roots.
Independent notes merge automatically; concurrent changes to one note remain alternatives.
Use `deleted` instead of an object ID to select deletion when that alternative exists.
Resolution creates another snapshot and retains previous alternatives in history.
Repeated capture of a conflicted note does not implicitly resolve it.
No hosted transport or background cross-device synchronization is configured by these commands.

## Assurance

Run `mise x -- just advisory_options=--offline verify` for the complete local gate when the advisory cache is current.
`just proofs`, `just lean`, `just fuzz`, and `just mutations` retain source-bound evidence under `target/assurance/`.
The [format](storage-format.md) and [decision](adr/keep-immutable-snapshots-with-editable-markdown.md) state representation, migration, and trust boundaries.
