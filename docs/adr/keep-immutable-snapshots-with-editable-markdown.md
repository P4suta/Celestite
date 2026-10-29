---
date: 2026-10-26
---

# Keep immutable snapshots with editable Markdown

## Status

Accepted.
Partly supersedes the authoritative-storage boundary in [native persistence](markdown-files-with-native-persistence.md).

## Context

Source files must remain editable Markdown with ordinary relative destinations.
Long-lived preservation also needs original bytes, captured attachments, stable identity, semantic link bindings, and recoverable history.
File paths alone cannot identify a renamed note or disambiguate a reused path.
Personal offline devices can produce concurrent revisions without a trustworthy total clock order.

## Decision

Use an additive content-addressed archive as the authoritative captured state.
Retain original imported bytes separately from managed Markdown with UUID front matter.
Use SHA-256 object identities, typed JSON records, prefix-sharded snapshot trees, and parent snapshot references.
Keep capture time descriptive and use ancestry for merges.
Preserve both alternatives for concurrent changes to the same note, including deletion against editing.
Require explicit resolution and keep the previous snapshots after resolution.
Use opaque device reference files for local publication and exclude machine-local state from interchange.

Keep ordinary relative Markdown destinations and record their source-bound note identities or media object references.
Rewrite only checked destination spans when materializing a different layout.
Keep ambiguous path reuse and unsupported syntax explicit instead of guessing intent.
Preserve original media and source encodings independently of managed views and derived representations.

Integrate archive capture into native checkpoints before acknowledging persistence.
Capture named imports and recovered drafts before editing them.
Keep standalone files in their original format; managed checkout writes the ID-bearing portable representation.
Separate persistent note identity, editing tokens, and recovery-record keys so windows of the same note cannot overwrite each other's draft.
Retain the native recovery journal before attempting archive publication and keep it on archive failures.

Store content as regular files and structure as JSON with an explicit format generation.
Keep SQLite disposable and provide an application-independent Rust CLI for verification, checkout, search, interchange, and restoration.
Use a documented BagIt profile for complete history interchange, with checksums over payload and tag inventories.
Refuse an existing checkout or restoration destination.

The [format](../storage-format.md) defines the public representation and trust boundary.
Kani checks production decisions; Lean checks the general ordered-history and retention model with complete Boolean refinements generated from Rust.
Native file/process tests, parser fuzzing, corruption fixtures, and independent Markdown interpretation cover external boundaries.

## Alternatives

A mutable SQLite database as the only original would couple recovery to the database schema and reader.
A plain folder as the only original would require a separate durable source for historical bytes and semantic link identity.
OCFL informs the preservation goals, while the application format explicitly represents concurrent branches.
Git remains useful for development but does not replace the chosen identity and source-bound link contracts.
Character-level collaborative editing and a hosted synchronization service are outside this preservation change.

## Consequences

Originals, attachments, and previous snapshots consume storage and are not silently pruned.
Unmanaged standalone files and managed checkouts have different roles and remain independently readable.
Portable checkout may rewrite link destinations and is not an assertion of byte-identical source export.
BagIt preserves exact captured bytes and history instead.
Unresolved links and conflicting notes remain visible rather than selecting an interpretation automatically.
Cryptography, standard libraries, filesystem synchronization, and healthy independent copies remain assumptions rather than proved physical guarantees.
The in-process verification cache assumes published objects remain immutable; explicit verification and interchange perform complete audits without that cache.
