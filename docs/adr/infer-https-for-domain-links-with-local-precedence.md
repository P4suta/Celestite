---
date: 2026-10-27
---

# Infer HTTPS for domain links with local precedence

## Status

Proposed.
Implemented as an extension of [current-source link navigation](resolve-links-from-the-current-markdown-source.md).

## Context

An editable Markdown note can contain a domain destination such as `[google.com](google.com)`.
CommonMark treats this destination as relative, while the intended interactive destination can be a website.
Inferring a Web address must preserve explicit relative paths and local note and media links.

## Decision

Infer an HTTPS URL for a dotted domain destination with no explicit scheme, credentials, control characters, or backslashes.
Use the pinned URL parser and strict IDNA validation instead of maintaining a hostname parser.
Preserve supported note and media filename extensions as local destinations.
Preserve an existing source-directory entry, including a directory or symbolic link, as local even when its name resembles a domain.
Propagate filesystem inspection errors instead of assuming that the entry is absent.
Use the [pure preference decision](../../crates/note-core/src/lib.rs) after collecting those facts.
The Kani contract covers all eight Boolean combinations.
The URL, IDNA, and filesystem adapters have known-answer and negative-control cases in the [navigation tests](../../crates/documents/tests/navigation.rs).

Keep the original Markdown unchanged.
Allow explicit `https://` and `./` or `../` destinations to resolve ambiguity.
Do not issue a DNS or HTTP request to decide whether a destination is local.
Do not reinterpret a failed explicit file link as a website.

## Consequences

Domain links work in both saved and untitled notes.
Paths, queries, fragments, IDNA domains, and valid ports pass through the URL parser.
Inference is an application convention and does not change preservation of the original CommonMark source.
Strict domain validation can reject unusual hosts that remain accessible through an explicit HTTPS URL.
The preference proof does not establish domain registration, DNS resolution, remote availability, or operating-system browser behavior.
