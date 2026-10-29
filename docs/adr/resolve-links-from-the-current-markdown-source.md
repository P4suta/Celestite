---
date: 2026-10-27
---

# Resolve links from the current Markdown source

## Status

Proposed.
Implemented; the detailed interaction contract remains open to owner review.
The consequence that moved files are never located through archival identities is partly superseded by [reference definitions](link-notes-with-shortcut-reference-definitions.md), which resolve identifier-named destinations in the library.

## Context

The editor displays editable Markdown and keeps each note in its own window.
Ordinary clicks must continue to position the caret.
Opening a link must preserve the current note and reject requests from another editing session.
Operating-system URL and file opening crosses the native application's trust boundary.

## Decision

Use Command/Ctrl-click or Command/Ctrl-Enter to follow a Markdown link.
Checkpoint current input before navigation and stop if checkpointing fails.
The native command requires the exact current token, revision, text, and a valid UTF-16 position before resolving any destination.
The [pure admission decision](../../crates/note-core/src/lib.rs) is checked by the source-bound Kani inventory.

Resolve CommonMark inline, reference, image, and angle-autolink destinations from source through the same parser dependency used by preservation.
Exclude front matter, code, and HTML links.
Resolve relative paths against the saved source file, using URL percent decoding and canonical local paths.
Open another note in another window and pass an optional initial heading position through bootstrap.
Use the current buffer for links to the same file.
The [navigation contract](../link-navigation.md) defines heading identifiers and supported destinations.

Open HTTP and HTTPS destinations through the native opener plugin.
Allow local PDF, image, audio, and video files through that same native boundary.
Keep the plugin's automatic JavaScript link interception disabled and grant no general opener capability to the webview.
Reject other schemes and executable file types.

Do not replace the current note or rewrite Markdown to implement navigation.
Do not use an ordinary-click handler, because it conflicts with editing.
Do not trust a destination supplied independently by JavaScript, because it can differ from the checked source.
Do not infer a renamed target from a reused path or silently choose an archival conflict alternative.

## Consequences

Navigation adds no persistent editor controls, tabs, or preview mode.
CommonMark parsing and URL conversion remain external semantic boundaries, covered by parser, filesystem, IPC, fuzz, and browser tests rather than a whole-parser proof.
The [assurance contract](../assurance.md) distinguishes those checks from scalar proofs.
Missing destinations and headings produce a dismissible error while preserving the source.
Relative links in untitled notes require saving first.
Opening a web link or attachment delegates rendering to the operating system's configured application.
This interaction does not yet provide a history of visited notes or locate moved files through archival identities.
