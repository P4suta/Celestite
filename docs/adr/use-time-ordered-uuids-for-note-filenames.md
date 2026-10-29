---
date: 2026-10-26
---

# Use time-ordered UUIDs for note filenames

## Status

Proposed.
The implementation follows this decision.
The Save As suggestion becomes the actual library filename under [the flat library decision](keep-notes-in-a-flat-library-of-identifier-named-files.md).

## Context

New notes need default filenames that resist collisions and have a useful ordering without adding visible controls.
The filename should follow a recognized format and remain stable through editing, cancelled save dialogs, and recovery.

## Decision

Generate the document's internal identity once with `Uuid::now_v7()` in [the native document constructor](../../crates/documents/src/document.rs).
Use its lowercase, hyphenated text with a `.md` extension as the name of an unsaved note and the initial Save As suggestion.
Preserve the actual basename and full Save As path of an already named file.
Continue generating independent random window session tokens.

[RFC 9562](https://www.rfc-editor.org/rfc/rfc9562.html#section-5.7) defines UUIDv7 with a leading Unix timestamp in milliseconds and additional entropy.
The locked `uuid` crate's [`now_v7` generator](https://docs.rs/uuid/1.29.0/uuid/struct.Uuid.html#method.now_v7) guarantees creation ordering within the same process.
Use this maintained generator rather than implementing a timestamp formatter or a custom random suffix.

Keep the existing recovery format and preserve stored document identities, including earlier UUIDv4 identities.
Do not regenerate an identity when a recovered note receives a fresh session token.
Do not rename existing files.

## Consequences

Lexical filename order reflects generator time, while existing names remain under the user's control.
The timestamp is encoded rather than displayed as a readable calendar date.
Clock differences between machines can affect ordering, and random generation does not establish mathematical collision freedom across independent generators.
Calendar-only filenames can collide within their timestamp resolution; random UUIDv4 names have no time ordering.
A date prefix plus a random suffix would add a private convention to the standard identifier.

The operating-system clock, entropy source, and UUID generator are external dependencies.
[Document tests](../../crates/documents/tests/naming.rs) check UUID version, ordering, extension, and legacy recovery.
[Recovery tests](../../crates/documents/tests/recovery.rs) check stable naming across recovery, and [session tests](../../tests/unit/session.test.ts) check cancelled Save As retries and existing paths.
Browser and native dialog checks establish the observed Save As suggestion rather than a proof of collision freedom or cross-machine chronology.
Reassess the ordering contract before adding synchronization or globally ordered imports.
