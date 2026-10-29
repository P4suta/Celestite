---
date: 2026-10-26
---

# Keep Markdown files with native persistence

## Status

Proposed.
The authoritative captured-state boundary is partly superseded by [immutable snapshots with editable Markdown](keep-immutable-snapshots-with-editable-markdown.md).
The Save As naming of new notes is partly superseded by [the flat library decision](keep-notes-in-a-flat-library-of-identifier-named-files.md).
The implementation follows this decision; the detailed persistence contract remains open to owner review.

## Context

The first usable product is a quiet notepad built with Rust and Tauri.
The source must remain editable Markdown, and users must be able to manipulate their files outside the application.
The interface uses black, white, regular and bold text, underlines, and menu separators.
The design reference is the [Celestite Figma file](https://www.figma.com/design/WjRkowhJYY2SMgdKWsvsz5), including the editor, revealed controls, grouped menu, and recovery states.

## Decision

Use Tauri 2 and Rust 2024 for native windows, dialogs, persistence, and recovery.
Use Svelte 5, strict TypeScript, Vite 8, and CodeMirror 6 for the editor.
Bundle the fonts and assets locally.
Keep the document as visible Markdown, with a centered active writing line and controls that disappear while typing.
Keep each document in its own window.
Opening another file reuses only an empty, untitled document.

The [pure Rust core](../../crates/note-core/src/lib.rs) decides edit admission, guarded writes, and window reuse.
The [document adapter](../../crates/documents/src/document.rs) owns filesystem effects.
The [frontend session](../../src/lib/session.ts) serializes native requests and preserves newer input when an older acknowledgement arrives.
Validate native responses with Valibot before admitting them to the frontend state.
Scope close requests to the intended webview window; appearance preferences are shared across windows.
Register the single-instance plugin before other plugins to focus the existing application when another copy starts.
Hold an operating-system file lock for the lifetime of the note store, so another process cannot recover or remove a live process's checkpoints even if instance notification fails.

Accept UTF-8 documents up to 16 MiB.
Normalize editing text to LF, while preserving a UTF-8 BOM and uniform CRLF or CR on subsequent saves.
Mixed line endings normalize to LF on save.
Serialized output must also fit the file limit, including its BOM and preserved line endings.
Reject stale window tokens, older revisions, conflicting text at the same revision, unsafe JavaScript revision integers, invalid encodings, and oversized input.

Named documents save after an 800 ms pause.
Write a recovery checkpoint before replacing a named file.
Untitled documents keep recovery checkpoints until they are saved or explicitly discarded.
Recovery uses a versioned JSON record in the native application data directory and assigns a fresh window token after restart.
Malformed recovery records remain on disk and produce a warning.

Check the current file's SHA-256 fingerprint before autosave.
Refuse detected external changes or deletion, and offer Reload or Save As.
Write a synchronized temporary file beside the destination and replace it atomically.
Preserve permissions and refuse read-only destinations.
Update the document's path and saved revision only after the write succeeds.

## Consequences

Ordinary Markdown files remain usable without Celestite.
There is no database import, rendered preview, graph, account, synchronization service, or archival format in this implementation.
Native persistence adds more failure handling than browser storage, but keeps file access outside the webview.
The Tauri capability grants native dialogs and window controls without exposing general filesystem or shell APIs to JavaScript.

Filesystem inspection and replacement are separate operating-system operations.
A noncooperating process can write between the fingerprint check and rename; this design does not promise a filesystem compare-and-swap operation.
Checkpointing can lose input entered since the last successful checkpoint, and file synchronization does not establish durability against every power-loss or hardware failure.
Atomic replacement can change inode identity and does not preserve every extended attribute or external file coordination mechanism.
Revisit the adapter before supporting coordinated editors, file providers, or stronger archival guarantees.

The current native verification and advisory graph target Apple Silicon macOS.
Windows, Linux, distribution signing, notarization, and release publication are separate work.
The all-platform lockfile includes Linux dependencies affected by RUSTSEC-2024-0370 and RUSTSEC-2024-0429; those crates are absent from the macOS dependency graph.
Resolve and verify those upstream dependencies before advertising Linux support.

The source-bound Kani harnesses cover admitted integer and Boolean domains of the production core decisions.
They import the production core and run with explicit unwinding and reachable cover properties.
The [assurance contract](../assurance.md) records scalar and inductive obligations, exact negative controls, and source-bound verdicts.
The operating system, instance notification, file locking, WebKit, CodeMirror layout, JavaScript event loop, native dialogs, JSON decoding, and filesystem behavior cross external semantic boundaries.
Their adapter evidence is strict type checking, deterministic session tests, actual filesystem tests, browser interaction tests, and native macOS interaction checks.
Those checks establish their observed cases, not a proof of the entire application.
Reassess these boundaries when changing persistence ordering, event routing, supported platforms, or recovery formats.
