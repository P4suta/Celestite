---
date: 2026-10-29
---

# Link notes with shortcut reference definitions

## Status

Proposed.
The implementation follows this decision; the completion gesture remains open to owner review.
Partly supersedes the consequence in [source navigation](resolve-links-from-the-current-markdown-source.md) that moved files are never located through archival identities.

## Context

Inline links to identifier-named notes read as `[Object](019a….md)`, which repeats the title in the label and hides the target behind an identifier.
Title-named destinations would break on duplicate titles and require rewriting every referrer when a heading changes.
The owner requires plain CommonMark that other tools render, with no wikilink syntax.

## Decision

A link between notes is a CommonMark shortcut reference link.
The body contains only the writer's words in brackets, such as `[Object]`, and a link reference definition at the end of the note binds those words to the target, such as `[Object]: 019a3f25-9b0e-7d12-a3c7-5e1f8a2b6d90.md`.
Images use the same shape with `![図]` and `[図]: assets/<digest>.png`.
The label is free text that only needs to be distinct within one note under CommonMark label matching, so two notes titled `Object` are told apart by choosing different words only where both are cited in the same note.

A destination whose filename is a lowercase hyphenated nonreserved UUID with the `.md` extension names that note by identity.
[Link resolution](../../crates/archive/src/links.rs) binds such destinations to the identity before any capture of the target, and binds `assets/<digest>.<extension>` destinations to the captured object, refusing an asset whose name does not match its content.
Every definition is bound by its destination byte span, including definitions with no body usage, so a layout rewrite touches only definition lines.

The application writes Markdown on the user's behalf only when a completion is chosen.
[The patch planner](../../crates/archive/src/definitions.rs) escapes `[`, `]`, and `\` in the words, validates the label with the pure [label verdict](../../crates/note-core/src/lib.rs), looks up an existing definition through the parser's label matching, and plans two patches: the bracketed words and one appended definition line.
The pure [definition decision](../../crates/note-core/src/lib.rs) appends a new definition, keeps an existing definition for the same destination, and refuses a label that already points elsewhere.
The pure [separator decision](../../crates/note-core/src/lib.rs) adds the blank line a definition needs so it never interrupts a paragraph.
The planner reparses the patched text and returns patches only when exactly one shortcut link to the destination appears at the insertion point and the definition resolves.
Positions inside front matter, code, HTML, and math are refused, and the byte budget includes the appended definition.

Completion opens on an unclosed bracket in the editor, offers library titles with their short identifiers, and applies the native patches as one undoable editor transaction.
Navigation follows a shortcut reference through its definition to the sibling file and reports a missing identifier-named note as absent from the library.

Collapsed, full, and inline links remain accepted on read and are never written by the application.
No wikilink syntax, title-based filenames, or rename propagation is introduced.

## Consequences

A definition anywhere in the note is legal and the first one wins, so the planner edits only the parser's winning definition and never relocates hand-written definitions.
A missing definition renders as literal brackets without an error in the output; the archive records it as unresolved.
Label uniqueness is judged on the case-folded, whitespace-collapsed key, so the refusal message shows the existing spelling.
Non-ASCII whitespace is not collapsed, so visually identical Japanese labels can be distinct.
Prose such as `[Alice]: yes` at a line start parses as a definition and is reported as unresolved rather than rewritten.
[Definition tests](../../crates/archive/tests/definitions.rs), [link binding tests](../../crates/archive/tests/links.rs), the library navigation test, IPC tests, the seeded fuzz invariants, Kani harnesses for the three pure decisions, and browser completion checks cover the contract.
CommonMark parsing and CodeMirror completion remain external boundaries covered by those checks rather than by a whole-parser proof.
