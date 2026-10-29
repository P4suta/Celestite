# Celestite archive format

The archive is the authoritative additive record; the workspace is an editable projection and SQLite is a disposable search index.
Format `celestite.archive`, generation `1`, uses UTF-8 JSON records and SHA-256 identifiers of exact file bytes.
The descriptor is `format.json`; this document is copied to `FORMAT.md` at creation.

## Objects and identities

`objects/sha256/HH/DIGEST` stores an immutable byte stream identified by its lowercase 64-digit SHA-256 digest.
`HH` is the first two digest characters; serialized references use `sha256:DIGEST`.
New note and device identities use UUIDv7; existing nonreserved UUID identities remain valid.
Notes use CommonMark with front matter containing an `id` UUID string.
Imported original bytes are retained independently of the managed Markdown source.
Unknown syntax, encoding details, and media originals are never replaced by derived representations.

## Records and publication

Records have a `type` discriminator: `note`, `shard`, `tree`, or `snapshot`.
A note binds its identity, logical path, managed source, original bytes, and source-bound link bindings.
An optional origin records the source location without authorizing restoration to that location.
A shard maps note IDs sharing their first two compact UUID digits to live, deleted, or conflicting entries.
A tree maps these prefixes to shard objects.
A snapshot contains its tree, parent snapshot references, device identity, and capture time as a decimal millisecond string.
Capture time is descriptive and never establishes ancestry.
Record fields use the checked Rust serialization contract with deterministic ordered maps.

The exact JSON fields are as follows.
`ObjectId` means a `sha256:` reference, `NoteId` means a nonreserved UUID, and all paths are UTF-8 relative paths using `/`.
Unknown fields and duplicate map identities are refused.
Readers normalize accepted UUID spellings before checking duplicate identities.

| Record | Fields after `type` |
| --- | --- |
| `note` | `note`: an object containing `id`, `path`, `source`, `original`, optional nullable `origin`, and `links` |
| `shard` | `notes`: a map from `NoteId` to an entry |
| `tree` | `shards`: a map from a two-digit UUID prefix to `ObjectId` |
| `snapshot` | `parents`: an array of snapshot `ObjectId` values; `tree`: `ObjectId`; `device`: `NoteId`; `captured_at`: decimal text |

Entries use `state: "live"` with `record`, `state: "deleted"`, or `state: "conflict"` with `alternatives` containing distinct note record references or null.
A binding contains `start`, `end`, `destination`, and `target`.
`start` is inclusive and `end` is exclusive, measured in UTF-8 bytes of the managed source.
Spans must be ordered, nonoverlapping, and lie on character boundaries.
Targets use `type: "note"` with `id`, `type: "file"` with `path` and `object`, `type: "external"`, `type: "unresolved"` with `reason`, or `type: "ambiguous"` with `previous` and nullable `current` note identities.
Unresolved interpretations can use an empty span and cannot authorize rewriting.
Canonical JSON is not required to read an object; the digest always identifies its actual bytes.
Current readers bound Markdown to 16 MiB, JSON records to 32 MiB, captured files to 4 GiB, and structural graph traversal to one million objects.

Each live entry points to a note record with the same identity.
A deleted entry retains history through snapshot parents.
A conflict contains distinct alternatives, including a null alternative for deletion.
Published history is a directed acyclic graph with complete hash-checked structural references.
Publication incrementally validates new objects against previously verified immutable closures within one process.
Explicit verification, backup, and restoration audit the complete closure without trusting that cache.
Unresolved semantic hyperlinks are diagnostics and differ from missing structural objects.

`refs/DEVICE-UUID` contains the device's current snapshot identifier.
Object files and parent closures must be synchronized before publishing that reference.
Reference replacement and directory synchronization precede local commit acknowledgement.
An uncertain synchronization result is not a successful acknowledgement.
Previous immutable snapshots remain retained even when publication fails after reference replacement.
`local/` contains machine-local writer ownership and device selection and is excluded from archival interchange.
Concurrent writers use a native file lock and compare the expected current snapshot.
Search and index replacement share that writer lock so a query stays on the snapshot whose cache metadata it checked.
Native recovery records use a separate persistent key per editing window, independently of the note ID and rotating token.
Legacy recovery records retain their note ID as the recovery key until explicitly removed.

## Links and interpretation

Markdown destinations remain normal relative links.
Each binding records its exact managed-source byte span, decoded destination, and target note identity or captured file object.
A reference-style link is bound at its definition's destination span, and every link reference definition is bound even without a body usage.
A relative destination whose filename is a lowercase hyphenated nonreserved UUID with the `.md` extension binds to that note identity whether or not the note is captured.
A destination of the form `assets/DIGEST.EXT` binds to the captured file only when the digest matches its content; otherwise it is unresolved.
Notes captured from the library use their bare filename as the logical path; other editor files use `files/UUID/NAME`.
Bindings apply only to that source object and must not be applied to another revision.
External URLs are references, not preserved web content.
Unknown or ambiguous interpretations retain original text and an explicit diagnostic.
Repairs require matching bytes, valid UTF-8 boundaries, and a unique target.

## Interchange and recovery

Lossless interchange uses BagIt 1.0 with SHA-256 payload and tag manifests.
The payload contains the descriptor, specification, reachable objects, and published references.
Restoration verifies the bag and archive graph before admitting references.
Portable Markdown export is a separate transformation with destinations rewritten for the selected layout.
Its note paths are `UUID.md` at the destination root; media paths are `assets/DIGEST.EXT` with a bounded ASCII extension or `bin`.
That layout is the application's library layout, so a checkout is a valid library.
It requires explicit conflict selection and reports unresolved links.
No automatic history pruning is performed.
Unknown generations refuse writes without modifying original content.
Migrations add representations and retain source versions.
Cryptographic collision resistance, filesystem synchronization semantics, and healthy independent copies remain explicit trust assumptions.
Checksums detect corruption; restoration requires a healthy copy.
