---
date: 2026-10-29
---

# Keep notes in a flat library of identifier-named files

## Status

Proposed.
The implementation follows this decision; the library location and its native controls remain open to owner review.

## Context

The notepad saves each note wherever a Save As dialog points, so a growing collection has no single place and relative links depend on where each file was saved.
Long-term personal notes need one predictable folder that other tools can read without the application.
Folder hierarchies were rejected because moving a note changes every relative link to it and the hierarchy itself becomes a maintenance task.
Human-readable filenames were rejected because legitimate duplicate titles exist, and refusing them or appending a numeric suffix interrupts writing.

## Decision

Keep one flat library folder.
Every note is a regular Markdown file named `<uuid>.md` directly inside that folder, using the lowercase hyphenated UUIDv7 from [the filename decision](use-time-ordered-uuids-for-note-filenames.md).
Media live in an `assets/` folder inside the library, named by the SHA-256 digest of their bytes with a lowercase extension.
The library defaults to a `Celestite` folder in the user's documents directory and can be overridden by the `library` preference.

A new note starts with front matter containing only its `id`, so the identity survives an external rename.
The note has no file until its first checkpoint with a nonempty body, when [the store](../../crates/documents/src/store.rs) writes `<id>.md` into the library through the ordinary atomic save.
The pure [library write decision](../../crates/note-core/src/lib.rs) admits that write only for an untitled note with a nonempty body while a library is configured.
Open, Save As, file associations, and recovery keep working for standalone files outside the library.
Notes in the library are captured with their bare filename as the logical path, so the archive and the library describe the same layout.
Portable checkout materializes that same layout and is therefore a valid library.

The library is a source folder that checkpoints capture.
It is not an archive-maintained checkout, because checkout refuses existing destinations and rewrites destinations, which would make the archive the writer of the user's files.

## Consequences

The folder lists identifiers rather than titles; the application shows titles through [the heading decision](take-the-note-title-from-its-first-heading.md).
An external rename to a non-identifier name keeps the note readable but removes it from link completion until the identifier is restored, and the store reports such files as warnings.
Front matter is visible at the top of every library note as plain Markdown.
Existing standalone notes keep the private `files/{id}/{name}` logical path until they are moved into the library by the user.
Identity by filename and by front matter can disagree after external copying; the front matter wins, as before, and the store excludes the mismatched file from completion.
[Library tests](../../crates/documents/tests/library.rs) and [IPC tests](../../src-tauri/src/tests.rs) check first-checkpoint placement, body emptiness, standalone behavior, and recovery.
The native documents-directory default, folder creation, and case-insensitive filesystems remain external boundaries covered by those tests rather than proofs.
