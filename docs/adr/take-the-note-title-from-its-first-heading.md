---
date: 2026-10-29
---

# Take the note title from its first heading

## Status

Proposed.
The implementation follows this decision.

## Context

Library files are named by identifier, so the filename no longer tells a person what a note is about.
Storing a separate title in metadata would duplicate what the note already says and drift from the body.
Link completion, window titles, and the file menu need a title that updates as the writer types.

## Decision

The title of a note is the rendered text of its first level-one heading after any front matter.
ATX and setext headings both qualify, inline code contributes its text, and ASCII whitespace runs collapse to one space.
A note without such a heading has no title and is shown by its filename.
[The title extraction](../../crates/archive/src/links.rs) uses the same CommonMark parser as link resolution and excludes headings inside front matter, code, and HTML.

The window title, the file menu, and the close dialog show the title when present.
[The title index](../../crates/documents/src/library.rs) scans the library root on demand, rereads only files whose size or modification time changed, and records the title at each checkpoint without rereading.
Completion candidates rank title prefix matches first, then title substrings, then identifier prefixes, and newest notes first within a rank.

Changing a heading never renames a file and never rewrites another note, because links carry identifiers rather than titles.

## Consequences

Titles are derived and can be stale for a note edited outside the application until the next scan.
Duplicate titles are allowed and are told apart by identifier in the completion list.
Ideographic spaces are preserved in titles because CommonMark only collapses ASCII whitespace.
[Title tests](../../crates/archive/tests/titles.rs), library index tests, and browser checks of the header cover the observed cases; the heading rules remain a CommonMark parser boundary rather than a proof.
