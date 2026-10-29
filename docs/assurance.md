# Assurance contract

Run `mise x -- just verify` for the complete automated inventory.
Run `mise x -- just check` for the fast development gate.
Run `mise x -- just mutations-list` before inspecting the bounded mutation scope.
Native GUI checks use `mise x -- just build-assurance` and the resulting `Celestite Assurance.app` bundle.

## Source-bound proofs

The [harnesses](../verification/src/lib.rs) import the [note core](../crates/note-core/src/lib.rs), [archive core](../crates/archive-core/src/lib.rs), and [assurance core](../crates/assurance-core/src/lib.rs).
The [compiled inventory contract](../xtask/src/proof_report.rs) requires every declared proof, negative control, owned assertion, and cover property.
Unknown or missing compiled harnesses fail the gate.
No harness replaces production functions with stubs.

| Contract | Production boundary | Evidence and domain |
| --- | --- | --- |
| Edit admission rejects old tokens, old revisions, conflicting replay, unsafe integers, and oversized text | `Document::edit` and `RevisionState::edit` | Complete `u64`, `usize`, and Boolean domains in edit and revision harnesses |
| Saved revisions never exceed the current revision or JavaScript's safe integer limit | `RevisionState::restore`, `edit`, and `mark_saved` | Complete scalar transitions and induction over arbitrary finite sequences |
| Closing requires the exact current token and revision, with explicit discard for dirty text | `Document::may_close` and `confirm_close` | Complete scalar close decision and actual registered IPC tests |
| Application exit waits for every live window and pending checkpoint cleanup | `plan_exit`, the native quit menu, and `confirm_close` | Complete Boolean exit policy; injected native acceptance, cleanup, duplicate-close, and window-target tests |
| Encoded text fits 16 MiB including BOM and CRLF expansion | `TextFormat::encode` | Complete scalar arithmetic compared with widened mathematical lengths; actual encoding corpus and file boundary tests |
| Recovery admits supported formats, ordered revisions, bounded text, and coherent absolute locations | `Document::from_recovery` | Complete scalar metadata admission; real JSON corruption and truncation tests |
| Writes reject stale revisions and detected external changes | `Document::save` | Complete write decision plus actual temporary-file conflict, permission, and deletion tests |
| Opening reuses only an empty untitled note | `Document::may_reuse` and `open_document` | Complete Boolean decision and actual multi-window IPC tests |
| Link navigation requires the exact current source and an admitted position | `Document::follow_link`, `plan_navigation`, and `follow_link` IPC | Complete `u64`, `usize`, and Boolean admission domains; malformed, stale, closed-window, initial-selection, and source-preservation IPC tests |
| Inferred HTTPS never overrides a supported local filename or an existing local entry | `prefer_web_link` and the document navigation adapter | All eight Boolean input combinations in Kani and unit tests; real filesystem collisions, explicit paths, missing files, Unicode domains, credentials, ports, and unsupported-protocol cases |
| Appends reject overflow and exceeded byte budgets | JSON serialization and source/report readers | Complete `usize` arithmetic; exact-boundary serialization and snapshot tests |
| Proof and mutation verdicts require current, complete, nonempty, consistent evidence | Rust xtask | Complete scalar admission proofs; actual exported JSON and corrupted-report tests |
| Concurrent note changes retain both alternatives unless one side is unchanged from the base | `merge_maps` and `merge_choice` | Complete Boolean merge decisions; real stale-edit and independent-device interchange cases |
| Publication requires synchronized payloads, an unchanged head, and successful transitions | `Vault::publish` and `Publication` | Complete scalar publication decisions; faults at each of four publication stages |
| Repairs require the exact source and valid byte bounds | `Vault::checkout_selected` and `admit_repair` | Complete `usize` span decisions; UTF-8, overlapping-span, and stale-source boundary checks |
| New graph extensions require a fresh identity and complete parents and payloads | `Vault::publish` and `admit_extension` | Complete Boolean decisions; structural graph and corruption refusals |
| Object identifiers have exactly the SHA-256 prefix and 64 lowercase hexadecimal digits | `ObjectId::from_str` and `valid_object_id` | All machine-integer lengths and every admitted fixed-width byte representation; parser corpus replay and fuzz round trips |
| A checkpoint acknowledges only the selected note's unconflicted source bytes | `NoteStore::checkpoint` and `admit_capture` | Complete Boolean decisions; unseen-revision, repeated-conflict, and independent recovery-key cases |
| Fuzz admission requires a successful nonempty fixed-run campaign on unchanged source | Rust xtask and `admit_fuzz_run` | Complete scalar admission; exact run-completion marker and corpus checks |
| A library file is written only for an untitled note with a nonempty body while a library is configured | `NoteStore::place_in_library` and `library_write` | All eight Boolean input combinations; real first-checkpoint placement, standalone, and IPC tests |
| A link label is accepted exactly when it is nonempty, single-line, bracket-balanced, not caret-prefixed, and within 999 scalars | `plan_link_patch` and `label_verdict` | Every four-byte label shape in Kani; Japanese, escaped, and boundary-length labels in tests |
| A definition is appended once, kept for the same destination, and never retargeted silently | `plan_link_patch` and `plan_definition` | Complete Boolean decision with its exact negative control; duplicate-label and idempotence tests |
| An appended definition never interrupts a paragraph | `plan_link_patch` and `definition_separator` | All sixteen Boolean input combinations; trailing-block, CRLF, and front-matter-only corpus |

The proof target is `x86_64-unknown-linux-gnu`, including 64-bit `usize` and `u64` semantics.
The native preview currently targets `aarch64-apple-darwin`, with the same scalar widths.
This does not establish equivalence of arbitrary platform APIs or of the entire native executable.
Scalar harnesses have explicit unwinding bounds and no input-size restrictions beyond their Rust types.
The sequence harness checks an inductive invariant using actual production transitions and an explicit decreasing counter assertion.
Its unwind setting covers the instrumented induction step rather than limiting the number of modeled edits.
The gate checks that induction properties and the progress assertion exist and pass.
Paired compiling and compile-fail examples check that callers use the revision constructor instead of constructing private fields directly.

Kani and CBMC versions are pinned in the container and checked against the export metadata.
The two deliberately false contracts must fail at their exact assertions.
Compiler errors, resource stops, signals, missing JSON, incomplete inventories, unresolved checks, and unrelated failed properties do not count as these expected failures.
The source snapshot includes Rust workspace inputs, the format specification, parser corpus, verifier configuration, locks, and the verification driver.
Generated targets and Tauri generated files are excluded.
Snapshots reject symlinks and inputs exceeding 64 MiB.
Raw evidence and source/image-bound receipts remain under `target/assurance/`.

## General history theorems

`just lean` checks five graph and retention theorems in [Archive.lean](../verification/Archive.lean) with pinned Lean 4.35.0.
They establish preservation of prior edges, decreasing ancestry, absence of cycles under birth ordering, and retention of both conflict alternatives in that model.
The runner generates the complete eight-case truth tables from the production Rust extension and merge decisions and proves their equality with the Lean definitions.
Both compiled modules are independently replayed by `leanchecker --fresh`.
The gate checks the exact theorem inventory and permits only Lean's named standard logical axioms.
No `sorry`, extra axiom, native evaluation certificate, or missing module is admitted.
The graph theorem assumes hash identity, complete validated parents, and immutable prior objects.
The Rust verifier audits concrete graph structure separately; this is an explicit model connection, not a proof of every filesystem operation or of the entire graph traversal implementation.

## Parser corpus and fuzzing

`just fuzz` runs the pinned cargo-fuzz target [archive_ids](../fuzz/fuzz_targets/archive_ids.rs) for exactly 4,096 executions with seed 4,701 and inputs bounded to 8,192 bytes.
The [shared invariants](../fuzz/invariants.rs) check note and object identifier parse/display/JSON round trips, typed record round trips, managed-source identity and idempotence, Markdown link and heading position bounds, title extraction, filename identity round trips, and link patch planning for note and asset destinations.
Ordinary archive tests replay every seed through that same checker and require positive note and object examples.
The generated corpus and failure artifacts remain with the source-bound receipt.
The container uses the pinned nightly compiler, no network, two CPUs, 2 GiB including swap, 256 processes, and a 900-second inconclusive stop limit.
This campaign uses `--sanitizer none`; it is bounded parser exploration, not an AddressSanitizer claim or a proof of arbitrary parser inputs.

## Executable boundary evidence

- Every two-byte sequence is checked for UTF-8 admission, independent newline normalization, and encoding stability.
- An 11-symbol Unicode alphabet generates every three-symbol string with an ASCII prefix and final newline, across three line endings and both BOM states.
- Recovery tests reject every truncation of a representative Unicode record and targeted invalid metadata while preserving the original bytes.
- Preferences and recovery serialization enforce their read budgets before replacing an existing record.
- A synchronized writer and reader check 64 replacements for whole-file visibility, including reads during each replacement.
- A child process acknowledges a completed checkpoint before its parent kills it, then the parent checks ownership release and actual recovery.
- The same Tauri command registry used by production receives malformed, stale, cross-window, conflict, save, reload, appearance, open, and close requests under Tauri's mock runtime with real temporary files.
- An injected native close failure exercises the production close adapter and checks retained recovery and revoked close permission.
- Native acceptance and rollback fault tests check checkpoint ordering, pending cleanup, duplicate close refusal, and preservation of the primary error.
- Archive tests preserve mixed newlines, BOMs, Unicode normalization forms, arbitrary media bytes, prior snapshots, and explicit deletion alternatives.
- Bag round trips verify complete payload and tag inventories, corrupt and missing files, unsafe paths, existing output refusal, independent device merges, and restoration without an index or native application.
- Portable export is parsed as Markdown and every rewritten local destination must exist.
- Link tests distinguish code, HTML, and front matter from Markdown destinations, preserve renamed identities, and retain ambiguity when a path is reused.
- Library link tests bind `UUID.md` destinations by identity before the target is captured, keep the referrer's record unchanged when the target's heading changes, bind unused definitions, and refuse digest-named assets whose content differs.
- Definition tests cover first and subsequent definitions, CRLF, escaped and Japanese labels, refused positions in code and front matter, the 16 MiB boundary, and the reparse postcondition that rejects an insertion which would not parse as a shortcut link.
- Title tests cover ATX and setext headings, inline code, BOM, CRLF, excluded front matter and fences, and the library index follows external additions, edits, deletions, identifier mismatches, and ranking.
- Browser completion checks open the monochrome list on an unclosed bracket, insert the label and definition as one undoable change, dismiss on Escape, stay silent inside code spans, complete images, and show a refused label as a dismissible error.
- Navigation tests cover CommonMark references, escaped destinations, Unicode and percent-encoded fragments, duplicate heading identifiers, UTF-16 boundaries, parent paths, current-buffer headings, unsupported protocols, and nonexecutable attachment types.
- The actual standalone executable performs import, verification, export, restore, and checkout in a fresh temporary directory.
- Index tests check native writer exclusion, snapshot changes, literal wildcard queries, and reconstruction from corrupt cache bytes.
- The frontend session executes all 42 valid orderings of three ordered edit, flush, and acknowledgement streams, checking every prefix against the latest text and acknowledged revision.
- Frontend fault tests cover invalid acknowledgements, revision exhaustion, failed subscriptions, disposal, retry, cancellation, and conflicts.
- Chromium and WebKit exercise keyboard flows, errors, source text, search, menu separators, monochrome computed styles, and the close dialog.
- Both engines check ordinary versus modifier clicks, modifier-Enter, link updates after editing, new-window initial headings, missing destinations, and centered in-note jumps.
- Both browser engines sample eight post-animation-frame caret positions for insertion, joining, and vertical navigation in short and wrapped Japanese lines.

These are finite behavioral checks, not proofs of every string, asynchronous history, filesystem schedule, or rendered frame.
Tauri's mock runtime does not execute native webviews or operating-system dialogs.
The native assurance build uses a separate application identifier and data directory, and registers no file associations.
Check its real save dialog, saved bytes, external-edit handling, multiple-window close isolation, recovery, and Japanese text before relying on native interaction evidence.
Check both Command-W and the application's Command-Q menu action, including Cancel and the last window's checkpoint cleanup.
Keep ordinary notes outside that test application's data and temporary files.

## Mutation adequacy

`just mutations` discovers all operators supported by pinned cargo-mutants in all three pure-core source files.
It executes the entire discovered inventory with all three core test packages, one mutation worker, no random shuffle, and no accepted-survivor exclusions.
The scope is limited to 384 discovered mutants; exceeding that scope stops the check instead of silently sampling it.
The runner requires matching discovery and outcome identities, the pinned tool version, an unmodified successful baseline, exact phase statuses, and current source bytes.
Every viable mutation must fail its tests.
Compiler-rejected mutations are reported separately and do not count as test detections.
Timeouts and unknown or partial outcomes cannot establish adequacy.
The selected operators and files define this claim; constructors and unsupported operator shapes are not implicitly covered by a mutation percentage.

## Trust and limits

The [persistence decision](adr/markdown-files-with-native-persistence.md) defines external-editor races, checkpoint intervals, and power-loss limits.
The [assurance decision](adr/bind-assurance-verdicts-to-frozen-sources.md) defines source binding and tool trust.
JSON parsing, UTF-8 libraries, cryptographic primitives, filesystem operations, process scheduling, CodeMirror, browser layout, native dialogs, and hardware remain external semantic boundaries.
Scheme-less Web inference additionally trusts the pinned URL and IDNA libraries for parsing and strict domain validation; the proved preference decision does not establish DNS resolution or remote availability.
Their current substitute evidence is the checked adapters and explicitly bounded native or browser cases above.
Reassess a boundary when its implementation, supported platform, persistence ordering, parser, runtime, or promised durability changes.
Do not report the scoped proofs and tests as a proof of the whole application.
