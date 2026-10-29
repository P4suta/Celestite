# Development

Install the pinned tools with `mise install` and dependencies with `mise x -- pnpm install --frozen-lockfile`.
Native macOS builds require Xcode Command Line Tools.
Run `mise x -- just dev` to start the desktop application.
Run `mise x -- just build` to create the local application bundle under `target/release/bundle/macos/`.
This build is a local preview; no signing or notarization credentials are configured.

## Interaction

Type directly in Markdown.
The active line remains vertically centered.
Move the pointer to the top of the window or press Escape to reveal the controls.
Open the filename menu for file operations, search, and Light or Dark appearance.
The three menu rules separate file metadata, file commands, search, and appearance.
Only text weight, underlines, lines, and spacing establish visual hierarchy.

Use Command on macOS or Control on other hosts with N, O, S, F, and W for New, Open, Save, Find, and Close.
Add Shift to Save for Save As.
Named files save automatically after a pause.
New notes start with front matter holding their lowercase, hyphenated UUIDv7 identity and are written as `UUID.md` into the library at their first checkpoint with a nonempty body.
The library is the `Celestite` folder in the documents directory unless the `library` preference names another folder; its `assets/` folder holds media named by content digest.
The window title and file menu show the note's first level-one heading when it has one.
Type `[` and a word to link another library note by title; see [link navigation](link-navigation.md) for the reference-definition form this writes.
Paste an image or drop files onto the window to store them in `assets/` by digest and insert an image reference at the caret.
The identity belongs to the document and survives edits, cancelled save dialogs, and recovery.
Without a configured library, untitled drafts are checkpointed locally and still require a Save As destination to become a Markdown file.
On a conflict, Reload replaces the local buffer with the file on disk, while Save As keeps the local text in another file.

## Checks

Run `mise x -- just check` for formatting, Clippy, Biome, strict Svelte and TypeScript checks, Rust and session tests, the macOS advisory graph, JavaScript dependency advisories, and source secret scanning.
The dependency graph currently targets `aarch64-apple-darwin`, matching the native preview.
Run `mise x -- pnpm exec playwright install chromium webkit` once before `mise x -- just ui`.
Browser tests use the official Tauri IPC mock only in Vite test mode; production bundles do not include that mock.
They cover editing, caret centering, visible Markdown emphasis, search, grouped menus, appearance, and close cancellation.
Chromium and WebKit sample the caret after each animation frame during line insertion, line joining, and vertical navigation, including wrapped Japanese text.
The trace checks these observed frames against the viewport center; it does not prove rendering behavior for every platform or input method.
Filesystem tests use real temporary files and exercise Unicode, line endings, stale edits, external changes, read-only files, recovery, interrupted cleanup, and malformed inputs.

`cargo-deny` refreshes its project-local RustSec cache under `target/advisory-dbs` by default.
If a local policy blocks its cache reset, clone the official database into the configured cache and compare its clean checkout identity against the official remote before using `mise x -- just advisory_options=--offline check`.
Offline advisory checks reject databases older than seven days.
Do not interpret a cached advisory pass as a scan of unsupported platforms.

Run `mise x -- just proofs` with a Docker-compatible Linux engine.
The Rust xtask builds the pinned Kani image, uses the returned immutable image identity, and runs the complete compiled harness inventory against a frozen snapshot of production source.
Each container has no network, a read-only source mount, a temporary target directory, 2 CPUs, 2 GiB of memory including swap, a 256-process limit, and a 900-second stop limit.
A stop, skipped harness, empty inventory, compilation error, missing cover, or unexpected negative-control verdict must fail verification.
The admitted domains, external trust boundaries, and limits are recorded in the [persistence decision](adr/markdown-files-with-native-persistence.md).
`mise x -- just lean` checks the general history theorems and the generated complete Boolean refinement with fresh kernel replay.
`mise x -- just fuzz` executes the fixed-seed parser campaign and retains its replay corpus.
`mise x -- just mutations-list` lists the bounded mutation inventory in all three pure cores.
`mise x -- just mutations` executes that inventory and requires every viable mutation to be caught.
`mise x -- just verify` combines the fast gate, browser interactions, Kani and Lean proofs, parser fuzzing, and mutation checks.
The [assurance contract](assurance.md) records source bindings, domains, bounds, trust assumptions, and boundary evidence.
The [archive operations](archive.md) cover independent verification, immutable backup, restoration, conflict resolution, and portable Markdown checkout.

Native verification must also check actual dialogs, saved bytes, external-edit handling, multiple-window close isolation, menu grouping, and Japanese text on the target macOS version.
Use `mise x -- just build-assurance` for an isolated `Celestite Assurance.app` with a distinct application identifier and no file associations.
Browser IPC mocks cannot establish those native contracts.
Check that launching both a development bundle and a release bundle keeps one owning process and focuses its existing window.

Markdown formatting remains outside Biome because its Markdown parser rewrites YAML front matter as a heading.
Keep documentation and replies at one sentence per source line.
