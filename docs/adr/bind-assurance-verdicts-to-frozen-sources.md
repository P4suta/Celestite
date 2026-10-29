---
date: 2026-10-26
---

# Bind assurance verdicts to frozen sources

## Status

Proposed.
The implementation follows this decision and remains open to owner review.
This extends the assurance boundaries in [native persistence](markdown-files-with-native-persistence.md).

## Context

A verification result is useful only for the implementation, assumptions, configuration, and target that produced it.
A nonzero process status can represent a counterexample, compiler failure, resource limit, or interrupted run.
Text summaries cannot establish that every required assertion and harness ran.
Tests that derive their expected limits from production constants can share the same defect.
Native interaction checks must preserve the ordinary application's notes and recovery records.

## Decision

Keep revision states private and construct them through checked transitions in the production Rust core.
Import that core into pinned Kani harnesses for complete finite integer and Boolean domains.
Use inductive loop contracts for arbitrary finite edit/save sequences, with an explicit assertion checking the decreasing unsigned iteration count.
Require the induction base, preservation, modification, progress, and postcondition checks to succeed.
Do not substitute a separate model for the production transitions.

Capture a bounded source snapshot before compiling verification inputs.
Compile a read-only mount of that snapshot with isolated temporary outputs, no network, and explicit resource limits.
Bind retained JSON, logs, and receipts to the source digest and immutable tool image.
Reject a changed snapshot or working source before admitting the result.
Reconcile the complete compiled harness inventory with the maintained expectations.
Accept negative controls only when the exact declared assertion fails with the expected property classification and process status.
Missing properties, unreachable owned assertions, incomplete runs, unsupported substitutions, timeouts, and other failures cannot establish verification.

Use pinned mutation testing to check both pure cores and their executable boundary contracts.
Require a passing unmodified baseline, a complete discovered inventory, caught viable mutants, and separate compiler-rejected mutants.
Keep unknown failures and timeouts inconclusive.
Test public numeric limits independently of their production expressions.

Exercise filesystem, process, IPC, frontend scheduling, and rendering boundaries through their real supported adapters.
Keep the scope and finite bounds in the [assurance contract](../assurance.md).
Build native interaction checks with a distinct application identifier and no file associations.
Use the production frontend and command handlers in that isolated application.
Retain recovery records until the native close request succeeds.
Revoke close permission if the native operation fails, while preserving its primary error.
Remove the acknowledged record only after successful native acceptance.
Track pending close cleanup independently of window destruction and prevent application exit until it finishes.
Route the macOS application menu and its quit accelerator through the application's exit request handler instead of a native predefined Quit action.

## Consequences

The compiler prevents invalid revision pairs from being constructed through the supported production API.
Kani adds complete scalar-domain and inductive evidence under its Rust and CBMC semantics.
Mutation testing can expose shared assumptions in tests and production code, but cannot establish semantic completeness of every possible mutation operator.
The full verification command costs more than the fast development checks.
Its inputs and raw evidence remain inspectable after a failed run.
An interrupted or failed post-close cleanup can restore an already discarded note on the next start.
This conservative duplicate is preferable to deleting recovery before a failed close.

The Rust compiler, Kani, CBMC, container engine, SHA-256 identity mechanism, and standard library remain trusted dependencies.
Operating-system persistence and GUI behavior remain explicit boundaries with native evidence.
No result establishes correctness of every platform, input method, concurrent external editor, or hardware failure.
Additional verifiers require a source refinement and a distinct property that the existing verifier cannot establish.
