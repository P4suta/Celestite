---
date: 2026-10-29
---

# Store media by content digest in the library

## Status

Proposed.
The implementation follows this decision; the paste and drop gestures remain open to owner review.
Extends [the flat library decision](keep-notes-in-a-flat-library-of-identifier-named-files.md), which named the `assets/` folder, and [the reference link decision](link-notes-with-shortcut-reference-definitions.md), which defined the image reference shape.

## Context

A library note can already reference `assets/<digest>.<extension>` through an image definition, but nothing wrote such files.
Media pasted from the clipboard arrive as bytes in the webview, while files dragged from Finder arrive as paths that only the native window should see.
The same picture pasted twice should not fill the library with copies, and a file that another tool placed under a digest name must never be overwritten.

## Decision

Pasting files into a note or dropping files onto its window stores each one in the library's `assets/` folder as `<sha256>.<extension>` and inserts `![label]` at the caret with its definition, through the same patch planner as note links.
The label is the file's name without extension, or `image` for an unnamed paste.
The extension is the lowercase spelling of a type [the link navigator](../../crates/documents/src/navigation.rs) already opens, with `jpeg` written as `jpg`; other types are refused.
[The store](../../crates/documents/src/store.rs) hashes the bytes into a temporary file beside the target, then follows the pure [asset decision](../../crates/note-core/src/lib.rs): it writes a new file without clobbering, keeps an identical existing file, and refuses a different file under the same digest name.
Pasted bytes travel as a raw IPC body with the extension in a header and stop at 64 MiB; dropped files are hashed from their path up to the archive's 4 GiB asset limit.
Dropped paths are handled by the native window event and reported to the webview only as a digest, extension, and label.
Media links are admitted only when a library is configured and the note has no path yet or lives inside that library, so the relative `assets/` destination resolves from the note's folder.

## Alternatives

Copying media under their original names was rejected because duplicate names would need renaming and two pastes of the same picture would store it twice.
Letting the webview send dropped paths was rejected because the path would then be chosen by page script rather than by the window that received the drop.
Rendering images inline stays out of scope; the source view keeps the reference text visible.

## Consequences

The `assets/` folder lists digests, so the reference label in the note is the only human-readable name of a picture.
A pasted screenshot named `image.png` by the system gets the label `image`; the writer can retype the words before or after the paste.
Standalone notes outside the library cannot receive media until they are placed in the library, and the editor reports that.
[Store tests](../../crates/documents/tests/library.rs), [IPC tests](../../src-tauri/src/tests.rs), [session tests](../../tests/unit/session.test.ts), and [browser checks](../../tests/browser/media.spec.ts) cover deduplication, collision refusal, extension normalization, the size limit, caret placement, undo, and refused types.
The native drag-and-drop event, the clipboard, and the file dialog remain external boundaries covered by the assurance build rather than proofs.
