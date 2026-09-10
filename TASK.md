# Torto Ebook Engine Tasks

This file is the execution source of truth for the roadmap in [`PLANS.md`](PLANS.md).

## How to Use This File

Status markers:

- `[ ]` not started or not yet verified;
- `[~]` in progress; add owner/session and a short note;
- `[x]` implemented and validated; add evidence;
- `[!]` blocked; keep unchecked and document the blocker;
- `[-]` intentionally cancelled or superseded; explain why.

Rules:

1. Do not mark a task complete without implementation evidence and required validation.
2. Preserve task IDs; agents and pull requests may reference them.
3. Add newly discovered work under the correct phase instead of silently expanding another task.
4. Keep task descriptions outcome-based. Put implementation detail in the task evidence or code review.
5. Product UI, persistence, networking, AI, OCR provider orchestration, sync, TTS, and OS integration do not belong in core engine tasks.
6. `apps/desktop` is inherited/reference code. Do not migrate or refactor it unless a task explicitly says so.
7. Active uncommitted work in `crates/vello-backend/src/curl_3d.rs` and `crates/vello-backend/src/shaders/` is user-owned WIP unless explicitly assigned.
8. Before editing, read `AGENTS.md` and `PLANS.md`, then record `git status --short`.

## Task Evidence Template

Add a short indented note below a completed or blocked item:

```markdown
- Evidence: `path`, relevant symbols, and commands run.
- Validation: `cargo test -p ...` — passed on YYYY-MM-DD.
```

For blockers:

```markdown
- Blocked: exact error or missing prerequisite.
- Affected: paths and symbols.
- Next: bounded recommended action.
```

---

# Baseline Already Present

These items describe code that existed when this roadmap was created. They are not proof that the API is stable or fully validated.

- [x] BASE-001 Platform-neutral engine facade exists.
    - Evidence: `crates/engine/src/lib.rs`, `Engine`, `EngineBook`, and `EngineReader`.
- [x] BASE-002 Product-neutral runtime aggregate exists.
    - Evidence: `crates/engine/src/runtime.rs`, `EngineRuntime`.
- [x] BASE-003 Non-committing navigation preparation exists.
    - Evidence: `EngineReader::{prepare_navigation,poll_navigation,commit_navigation,cancel_navigation}` and corresponding reader APIs.
- [x] BASE-004 Backend-neutral prepared frame exists.
    - Evidence: `crates/engine/src/frame.rs`, `PreparedReaderFrame`.
- [x] BASE-005 Shared Vello backend exists.
    - Evidence: `crates/vello-backend`.
- [x] BASE-006 Native engine demo exists.
    - Evidence: `apps/engine-demo`.
- [x] BASE-007 WASM adapter exists.
    - Evidence: `crates/engine-wasm`.
- [x] BASE-008 Android native host boundary exists.
    - Evidence: `apps/android/src/lib.rs`; JNI and Activity are explicitly not implemented.
- [x] BASE-009 Pointer-driven slide/curl transition prototype exists.
    - Evidence: `crates/engine/src/transition.rs` and `crates/engine/src/reader.rs`.
- [x] BASE-010 Curl3D backend work exists as an experiment/WIP.
    - Evidence: `crates/vello-backend/src/curl_3d.rs` and `crates/vello-backend/src/shaders/`.

---

# Phase 0 — Rebaseline and Boundary Protection

## Repository and dependency audit

- [x] P0-001 Capture a clean baseline report for current tracked and untracked work without modifying user WIP.
    - Evidence: commit `d52342f`; baseline status preserved `crates/vello-backend/src/curl_3d.rs`, `crates/vello-backend/src/shaders/`, and the three untracked ebook reports.
- [x] P0-002 Run and record narrow build/test status for `rebook-publication`, `rebook-reader`, `rebook-engine`, `rebook-vello-backend`, and `rebook-engine-demo`.
    - Validation: `cargo test -p rebook-publication --locked` (7 passed), `cargo test -p rebook-reader --locked` (56 passed), `cargo test -p rebook-engine --locked` (14 passed), `cargo test -p rebook-vello-backend --locked` (8 passed), `cargo test -p rebook-engine-demo --locked` (9 passed). Demo retains two warnings from Curl WIP fields/imports.
- [x] P0-003 Run and record WASM target check for `rebook-engine-wasm`.
    - Validation: `cargo check -p rebook-engine-wasm --target wasm32-unknown-unknown --locked` — passed.
- [x] P0-004 Run and record Android target check for `rebook-android-host`, or document missing local toolchain prerequisites.
    - Validation: `cargo check -p rebook-android-host --target aarch64-linux-android --locked` — passed; target is installed.
- [x] P0-005 Generate a dependency map for core crates, backend crates, adapters, demo, and inherited desktop.
    - Evidence: `cargo metadata --no-deps --format-version 1`; package manifests; `cargo tree -p rebook-engine`, `cargo tree -p rebook-engine-wasm`, and `cargo tree -p rebook-android-host`.
- [x] P0-006 Verify `rebook-engine` has no UI, GPU, platform, network, database, sync, keyring, updater, or provider dependencies.
    - Evidence: `crates/engine/Cargo.toml` and inspected normal dependency tree contain only formats, layout, publication, reader, renderer, regex, serde, serde_json, thiserror, and web-time.
- [~] P0-007 Audit target-specific format feature closure for native default, EPUB-only WASM, and Android builds.
    - Finding: native `rebook-engine` defaults to `formats-full`; WASM requests `web-epub` and its target check passes, but the dependency tree and release-size closure still need a dedicated audit.
- [x] P0-008 Audit direct low-level crate dependencies in `apps/engine-demo` and classify each as required backend/tooling access or facade gap.
    - Evidence: `apps/engine-demo/Cargo.toml`; direct layout/publication/reader/renderer imports are demo inspection/render-lab access, while Vello/wgpu/winit belong to the demo surface/backend and not core.
- [~] P0-009 Audit direct low-level crate dependencies in `crates/engine-wasm` and classify each as required backend/ABI access or facade gap.
    - Finding: `rebook-vello-backend` and `rebook-renderer::PageDisplayList` are backend painting dependencies; `rebook-layout::ReaderFontBlob` is used to construct engine fonts. The remaining direct dependencies need a follow-up API-closure decision in P3.

## Public API ownership

- [x] P0-010 Inventory every public export from `rebook-engine`.
    - Evidence: `crates/engine/src/lib.rs` exports config/error/features/frame/input/platform/reader/runtime/transition and selected lower-level reader/layout/publication types.
- [ ] P0-011 Classify exports as stable core, optional helper, experimental, legacy compatibility, or accidental.
- [ ] P0-012 Decide whether `ReaderSession` and lower-level reader types should remain re-exported by the product-facing facade.
- [x] P0-013 Decide whether persistent `Bookmark`, `Highlight`, and `HighlightColor` entities belong in engine API.
    - Decision: these are platform persistence/product entities and do not belong in the engine API; the engine retains `LocatorV1`, `SourceRange`, `SearchResult`, and overlay ranges.
- [x] P0-014 Replace product-shaped annotation entities with persistence-neutral overlay/source primitives if P0-013 removes them.
    - Evidence: removed unused persistent entities from `crates/engine/src/features.rs` and their exports from `crates/engine/src/lib.rs`; existing `OverlaySet` and `SourceRange` remain available.
    - Validation: `cargo check -p rebook-engine --locked`, `cargo test -p rebook-engine --locked` — 14 passed.
- [ ] P0-015 Mark transition and Curl APIs as stable or experimental explicitly.
- [ ] P0-016 Define a deprecation policy for accidental public exports before removing them.

## Documentation rebaseline

- [x] P0-017 Update `docs/ARCHITECTURE.md` for `crates/engine`, `crates/engine-wasm`, `apps/android`, and `crates/vello-backend`.
    - Evidence: current-boundary update in `docs/ARCHITECTURE.md` sections 15–16.
- [x] P0-018 Mark `apps/desktop` as inherited/reference architecture in repository documentation.
    - Evidence: `docs/ARCHITECTURE.md` now identifies the inherited desktop shell as a consumer, not the engine architecture.
- [x] P0-019 Update `docs/ENGINE_RUNTIME_API.md` to state that new platform adapters use `EngineRuntime`, while inherited desktop migration is optional.
    - Evidence: `docs/ENGINE_RUNTIME_API.md:3` and platform ownership section.
- [ ] P0-020 Classify each `ebook_reader_*.md` file as research, product target, or target capability architecture rather than current implementation truth.
- [ ] P0-021 Add repository baseline commit/date and engine/platform ownership statement to the ebook reports.
- [ ] P0-022 Remove or reconcile stale roadmap claims in the ebook reports that conflict with current engine, WASM, Android-host, and compositor code.

## Phase gate

- [ ] P0-GATE Dependency direction is documented and verified.
- [ ] P0-GATE Engine public API ownership is classified.
- [ ] P0-GATE Current build failures and toolchain blockers are recorded.
- [ ] P0-GATE Legacy desktop and experimental Curl work are outside the Engine v1 critical path.

---

# Phase 1 — Publication and Semantic Identity

## Publication identity audit

- [ ] P1-001 Document how `PublicationId` is generated for every enabled format.
- [ ] P1-002 Test publication ID stability for repeated opens of identical bytes.
- [ ] P1-003 Test collision and changed-content behavior for publication IDs.
- [ ] P1-004 Audit `SpineItemId`, source node IDs, and `SourceAnchor` generation across format adapters.
- [ ] P1-005 Audit source-anchor stability when parser normalization changes.
- [ ] P1-006 Audit href normalization, percent encoding, fragments, and relative URL resolution.
- [ ] P1-007 Audit internal links, missing targets, footnotes/endnotes, and cross-section anchors.
- [ ] P1-008 Audit resource identity for images, fonts, SVG, math, PDF pages, and comic pages.
- [ ] P1-009 Document fixed-layout versus reflowable capability representation.

## Reading IR audit

- [ ] P1-010 Build a coverage matrix for paragraphs, headings, nested lists, quotes, code, tables, images, SVG, math, links, and language metadata.
- [ ] P1-011 Audit ruby annotation support and document current limitations.
- [ ] P1-012 Audit vertical-writing support and document current limitations.
- [ ] P1-013 Audit accessibility metadata preserved by parsers, including alt text, roles, labels, and reading order.
- [ ] P1-014 Audit malformed markup recovery and ensure diagnostics retain useful context.
- [ ] P1-015 Define a normalized ingestion contract for platform-supplied derived publications such as OCR output without adding provider logic to engine.

## Locator durability

- [ ] P1-016 Define and document the locator restoration priority chain.
- [x] P1-017 Populate `LocatorV1::text` for current reading locators with bounded before/highlight/after context.
    - Evidence: `ReaderSession::current_locator` now derives a quote from the first visible source-backed text region in `crates/reader/src/session.rs`; bounds are 64/128/64 Unicode scalars in `crates/reader/src/model.rs`.
    - Validation: `cargo test -p rebook-reader --locked` — 56 passed; `cargo check -p rebook-engine --locked` — passed.
- [ ] P1-018 Evaluate whether a structural locator such as partial CFI can be produced reliably; implement or explicitly defer with evidence.
- [ ] P1-019 Implement nearby text-quote recovery scoped by publication, href, and approximate progression.
- [ ] P1-020 Return a structured recovery quality: exact, structural, quote match, href fallback, total fallback, or failure.
- [ ] P1-021 Add locator schema/version migration tests.
- [ ] P1-022 Add tests for parser-node identity changes while text remains equivalent.
- [ ] P1-023 Add tests for missing href and moved section fallback.
- [ ] P1-024 Add tests ensuring ambiguous text quotes do not silently choose unrelated content.
- [~] P1-025 Add tests for locator round-trip across viewport, font, margin, spacing, and spread changes.
    - Partial evidence: existing `durable_locator_restores_after_viewport_repagination` now asserts bounded quote production and source restoration across viewport changes. Font/margin/spacing/spread matrix remains open.

## Untrusted content and limits

- [ ] P1-026 Audit archive entry count, decompression ratio, total expanded size, and path traversal limits.
- [ ] P1-027 Audit XML/HTML recursion, entity, and allocation limits.
- [ ] P1-028 Audit image dimension and decoded-byte limits before allocation.
- [ ] P1-029 Audit PDF and comic page dimension arithmetic for overflow.
- [ ] P1-030 Add malformed/adversarial fixtures that are safe to commit.
- [ ] P1-031 Add fuzz/property targets for the highest-risk parser and URL/anchor boundaries.
- [x] P1-032 Reject stale source anchors instead of silently restoring to section/page zero.
    - Evidence: `ReaderSession::position_for_source_anchor` now returns `NavigationTargetNotFound` when no source fragment, segment, or page contains the anchor; `stale_source_anchor_is_rejected_without_moving_reader` covers state preservation.
    - Validation: `cargo test -p rebook-reader --locked` — 57 passed; `cargo test -p rebook-engine --locked` — 14 passed.

## Phase gate

- [ ] P1-GATE Durable locator recovery is implemented and reports recovery quality.
- [ ] P1-GATE Source identity and internal navigation behavior are covered across representative formats.
- [ ] P1-GATE Parsing untrusted input has documented and tested resource limits.
- [ ] P1-GATE Reading IR capability and limitation matrix is current.

---

# Phase 2 — Reading Core Correctness

## State ownership audit

- [ ] P2-001 Inventory state owned by `ReaderSession`, `EngineReader`, and `EngineRuntime`.
- [ ] P2-002 Remove or document duplicated current-position, locator, selection, navigation, and transition state.
- [ ] P2-003 Define committed reading state separately from prepared navigation and visual transition state.
- [ ] P2-004 Document thread-safety and worker ownership for native reader sessions.
- [ ] P2-005 Document cooperative scheduling ownership for single-threaded WASM.

## Layout and invalidation

- [ ] P2-006 Build an invalidation matrix for viewport, DPR, font set, typography, margins, spread mode, source refresh, overlays, and transitions.
- [ ] P2-007 Verify physical-size/DPR-only changes do not trigger logical reflow.
- [ ] P2-008 Verify logical viewport changes preserve semantic location.
- [ ] P2-009 Verify style changes preserve semantic location.
- [ ] P2-010 Verify single/double spread changes preserve semantic location.
- [ ] P2-011 Test odd/even spread boundaries and blank companion-page policy.
- [ ] P2-012 Test section, segment, and fixed-layout page boundaries.
- [ ] P2-013 Make invalidation reasons observable in diagnostics where broad cache clears hide the cause.

## Navigation primitives

- [ ] P2-014 Verify prepare never changes committed locator or snapshot.
- [ ] P2-015 Verify cancel preserves committed state.
- [ ] P2-016 Verify commit moves exactly once.
- [ ] P2-017 Verify stale and double-used navigation tokens cannot commit.
- [ ] P2-018 Verify resize/style/source changes invalidate prepared navigation.
- [ ] P2-019 Verify current and destination spreads stay simultaneously available during preparation.
- [ ] P2-020 Add engine navigation commands for locator, TOC item, href/fragment, and source anchor.
- [ ] P2-021 Ensure navigation command results report moved, pending, boundary, approximate restore, and error states consistently.
- [ ] P2-022 Expose sufficient primitives for platform-owned History/Back/Peek without storing the history stack in engine.

## Hit testing and selection

- [ ] P2-023 Define a stable deep-hit result for text, links, images, references, and block context.
- [ ] P2-024 Verify hit testing across primary and secondary pages.
- [ ] P2-025 Verify hit testing after reflow and overlay-only updates.
- [ ] P2-026 Verify word, sentence, and paragraph selection for Latin, CJK, mixed scripts, punctuation, and code.
- [ ] P2-027 Verify semantic selection across logical page boundaries.
- [ ] P2-028 Decide and document whether selection may cross spine sections.
- [ ] P2-029 Verify table-cell and nested-list selection semantics.
- [ ] P2-030 Verify selection geometry regenerates from source ranges after reflow.
- [ ] P2-031 Expose source-backed overlay input without persistent annotation business entities.

## Input ownership refactor

- [ ] P2-032 Audit whether `PointerGestureController` belongs in stable engine core or an optional interaction helper.
- [ ] P2-033 Define the stable semantic command API independent of touch, mouse, keyboard, and hardware keys.
- [ ] P2-034 Ensure platforms can bypass pointer helpers and issue deterministic navigation/selection commands directly.
- [ ] P2-035 If pointer helpers remain, separate platform gesture arbitration from reader-state mutation.
- [ ] P2-036 Remove Curl-specific gesture geometry from the stable core API unless justified as an optional transition contract.

## Native/WASM behavior parity

- [ ] P2-037 Run the same reader fixtures through native worker and cooperative WASM scheduling paths.
- [ ] P2-038 Compare snapshots, locators, navigation outcomes, and selections across scheduling models.
- [ ] P2-039 Verify cancellation and close release pending work in both models.
- [ ] P2-040 Verify no API relies on blocking loops to wait for cooperative work.

## Phase gate

- [ ] P2-GATE Reading state has one authoritative owner at each layer.
- [ ] P2-GATE Reflow and configuration changes preserve semantic context.
- [ ] P2-GATE Navigation transactions satisfy prepare/commit/cancel/stale-token invariants.
- [ ] P2-GATE Hit testing and selection remain source-backed across layout changes.
- [ ] P2-GATE Native and WASM paths pass shared behavioral tests.

---

# Phase 3 — Engine API v1 and Adapter Contract

## API design

- [ ] P3-001 Define Engine v1 use cases and the minimum call sequence for each.
- [ ] P3-002 Group API operations into open, inspect, configure, command, tick, frame, query, lifecycle, and close.
- [ ] P3-003 Define structured runtime status for idle, pending, frame-ready, needs-frame, boundary, recoverable error, and fatal error.
- [ ] P3-004 Define operation/token semantics for work spanning multiple calls.
- [ ] P3-005 Define cancellation behavior for open, navigation, layout, prefetch, and close.
- [ ] P3-006 Define error taxonomy and preserve source errors without exposing platform details.
- [ ] P3-007 Define capability reporting for enabled formats and optional engine/backend behavior.
- [ ] P3-008 Define public API compatibility and deprecation rules.
- [ ] P3-009 Define serialization/versioning policy for `LocatorV1` and other durable engine DTOs.
- [ ] P3-010 Document ownership/lifetime of publication bytes, fonts, resources, frames, and selections.

## Facade cleanup

- [ ] P3-011 Remove accidental low-level exports or mark them as advanced/unstable.
- [ ] P3-012 Keep backend types out of `rebook-engine`.
- [ ] P3-013 Keep persistent platform entities out of `rebook-engine`.
- [ ] P3-014 Separate optional search and interaction helpers from mandatory reader runtime if this reduces dependencies cleanly.
- [ ] P3-015 Ensure `OpenReaderRequest` contains only state required before the first visible frame.
- [ ] P3-016 Review whether focus ranges are a generic overlay primitive or product-specific concept and refactor accordingly.

## Accessibility semantics

- [ ] P3-017 Define platform-neutral accessible node roles needed for rendered book content.
- [ ] P3-018 Expose current/visible semantic reading order and readable text.
- [ ] P3-019 Expose labels/descriptions for images and non-text content when available.
- [ ] P3-020 Expose semantic reading actions without importing platform accessibility APIs.
- [ ] P3-021 Expose selection and progress semantics.
- [ ] P3-022 Add conformance tests for accessibility snapshots.

## Adapter convergence

- [ ] P3-023 Make the native demo use the high-level runtime for all reader behavior.
- [ ] P3-024 Remove unjustified low-level reader/layout imports from the native demo.
- [ ] P3-025 Make WASM use the high-level runtime for all reader behavior.
- [ ] P3-026 Remove unjustified low-level reader/layout imports from the WASM adapter.
- [ ] P3-027 Keep Android host dependent only on the engine facade except explicit backend integration.
- [ ] P3-028 Verify no adapter owns pagination, locator recovery, navigation commit policy, or semantic hit testing.
- [ ] P3-029 Add minimal integration examples for native Rust, WASM, and Android host.

## Phase gate

- [ ] P3-GATE Engine v1 API draft is documented and reviewed.
- [ ] P3-GATE Demo, WASM, and Android host share the same reader behavior API.
- [ ] P3-GATE Stable API contains no UI, storage, network, provider, or concrete GPU concepts.
- [ ] P3-GATE Durable DTO versioning, errors, cancellation, and lifetimes are documented.

---

# Phase 4 — Backend-Neutral Frames and Vello Backend

## Prepared frame audit

- [ ] P4-001 Audit `PreparedReaderFrame` for backend-specific assumptions.
- [ ] P4-002 Audit frame cloning for large retained structures or pixel buffers.
- [ ] P4-003 Verify frame keys and revisions uniquely identify layout/content/overlay changes.
- [ ] P4-004 Define which changes require page rebuild, scene rebuild, overlay rebuild, transform update, or no redraw.
- [ ] P4-005 Verify destination frame lifetime remains valid through commit/cancel.

## Vello backend boundaries

- [ ] P4-006 Verify `rebook-vello-backend` does not own reader semantics.
- [ ] P4-007 Verify `rebook-renderer` remains free of Vello/wgpu types.
- [ ] P4-008 Separate static page content from dynamic overlays and transitions.
- [ ] P4-009 Audit scene cache invalidation for resize, style, spread, source, overlay, and transition changes.
- [ ] P4-010 Audit image and PDF raster identity across scenes and uploads.
- [ ] P4-011 Verify current and destination scenes are pinned during transitions.
- [ ] P4-012 Add backend capability and fallback reporting.

## None and Slide

- [ ] P4-013 Verify None renders and commits deterministically without animation.
- [ ] P4-014 Verify Slide uses prepared source/destination scenes.
- [ ] P4-015 Verify Slide commit happens only after successful settle.
- [ ] P4-016 Verify Slide cancel returns to the exact source locator.
- [ ] P4-017 Verify surface/device errors do not silently commit navigation.
- [ ] P4-018 Verify double-page spreads animate as one reading surface.

## Curl experiment

- [ ] P4-019 Complete a code and shader safety review of current Curl3D WIP without overwriting user changes.
- [ ] P4-020 Put Curl3D behind an explicit experimental capability/feature if not already isolated.
- [ ] P4-021 Define fallback behavior for unsupported adapters and shader/pipeline failures.
- [ ] P4-022 Verify Curl visual state never becomes authoritative reader state.
- [ ] P4-023 Verify Curl failure/cancel preserves committed locator.
- [ ] P4-024 Defer visual polish until None/Slide and warm-frame invariants pass.

## Rendering validation

- [ ] P4-025 Add offscreen render smoke tests with graceful no-adapter handling.
- [ ] P4-026 Add row-padding, zero-size, dimension-overflow, and target-reuse tests.
- [ ] P4-027 Add perceptual image comparisons for a small deterministic fixture set.
- [ ] P4-028 Test surface resize/loss/recovery in available adapters.

## Phase gate

- [ ] P4-GATE Core engine and retained renderer remain backend-neutral.
- [ ] P4-GATE Static/dynamic invalidation behavior is tested.
- [ ] P4-GATE None and Slide are reliable and preserve navigation state.
- [ ] P4-GATE Curl is isolated as experimental unless it passes all mandatory gates.

---

# Phase 5 — Performance, Memory, and Reliability

## Instrumentation

- [ ] P5-001 Define a versioned metrics schema.
- [ ] P5-002 Measure publication open separately from reader creation and first layout.
- [ ] P5-003 Measure section parse, layout, display-list compile, scene build, GPU submit, and optional GPU completion separately.
- [ ] P5-004 Measure current/destination preparation latency.
- [ ] P5-005 Measure cache hit, miss, build, eviction, and pin counts.
- [ ] P5-006 Measure p50, p95, and p99 frame CPU/GPU timing where available.
- [ ] P5-007 Report unavailable metrics as unavailable, never fabricated zero.
- [ ] P5-008 Make metrics optional with negligible disabled overhead.

## Warm-frame invariants

- [ ] P5-009 Assert/report zero section parsing during prepared animation frames.
- [ ] P5-010 Assert/report zero layout during prepared animation frames.
- [ ] P5-011 Assert/report zero display-list compilation during prepared animation frames.
- [ ] P5-012 Assert/report zero image decode/PDF raster during prepared animation frames.
- [ ] P5-013 Assert/report zero static scene build after transition preparation.
- [ ] P5-014 Assert/report zero render-target recreation without size/format/device change.
- [ ] P5-015 Verify transform-only frames do not alter content or overlay revisions.

## Cache and memory

- [ ] P5-016 Document ownership and budget of publication, section, layout, display-list, scene, raster, and GPU caches.
- [ ] P5-017 Remove duplicate caches only where metrics prove duplication.
- [ ] P5-018 Add exact byte accounting where possible and label estimates clearly.
- [ ] P5-019 Verify moderate memory pressure drops speculative resources.
- [ ] P5-020 Verify critical memory pressure retains only visible/active-transition requirements.
- [ ] P5-021 Verify current/destination resources cannot be evicted mid-transaction.
- [ ] P5-022 Audit large `Arc`, scene, and pixel-buffer clone behavior.
- [ ] P5-023 Verify close/suspend releases transient resources and worker work.

## Corpus and benchmarks

- [ ] P5-024 Define ignored/local corpus configuration without copyrighted repository content.
- [ ] P5-025 Add representative Latin EPUB case.
- [ ] P5-026 Add representative CJK/mixed-script EPUB case.
- [ ] P5-027 Add image-heavy and large-section EPUB cases.
- [ ] P5-028 Add table/math/footnote case.
- [ ] P5-029 Add fixed-layout PDF and high-resolution comic cases where supported locally.
- [ ] P5-030 Add legacy format cases for enabled native features.
- [ ] P5-031 Record viewport, DPR, spread mode, adapter, backend, transition, and cold/warm state in benchmark output.
- [ ] P5-032 Add bounded regression thresholds with documented hardware/environment context.

## Reliability and fuzzing

- [ ] P5-033 Add property tests for dimension and progression arithmetic.
- [ ] P5-034 Add property tests for navigation token state transitions.
- [ ] P5-035 Add fuzz targets for archive/markup entry points selected in Phase 1.
- [ ] P5-036 Audit panic paths reachable from malformed publications.
- [ ] P5-037 Audit worker/channel shutdown and poisoned/error states.
- [ ] P5-038 Verify idle native demo rendering approaches zero redraw activity.

## Phase gate

- [ ] P5-GATE Stage-specific reproducible metrics exist.
- [ ] P5-GATE Warm-frame no-heavy-work invariants pass.
- [ ] P5-GATE Memory-pressure behavior is tested and safe.
- [ ] P5-GATE Representative corpus runs are documented.
- [ ] P5-GATE Untrusted-input and arithmetic reliability coverage is in place.

---

# Phase 6 — Cross-Platform Conformance

## Shared conformance harness

- [ ] P6-001 Define a serializable conformance scenario format or equivalent shared Rust fixtures.
- [ ] P6-002 Cover open, metadata, TOC, first snapshot, and first locator.
- [ ] P6-003 Cover resize, style, and spread changes.
- [ ] P6-004 Cover next/previous and section-boundary navigation.
- [ ] P6-005 Cover prepare/poll/commit/cancel behavior.
- [ ] P6-006 Cover href/TOC/source/locator navigation.
- [ ] P6-007 Cover hit testing and semantic selection.
- [ ] P6-008 Compare semantic outputs rather than backend pixels where appropriate.
- [ ] P6-009 Document allowed target-specific differences.

## Native demo

- [ ] P6-010 Verify inspect and pagination commands use engine API.
- [ ] P6-011 Verify offscreen and window rendering use prepared frames and shared backend.
- [ ] P6-012 Add deterministic command/input replay for conformance scenarios.
- [ ] P6-013 Add machine-readable metrics output for corpus runs.
- [ ] P6-014 Keep demo free of product library/settings/persistence features.

## WASM

- [ ] P6-015 Verify EPUB-only dependency closure and absence of disabled native format dependencies.
- [ ] P6-016 Verify release WASM builds with the documented Binaryen/optimization strategy.
- [ ] P6-017 Record raw, gzip, and Brotli release sizes.
- [ ] P6-018 Remove JSON strings from hot ABI paths.
- [ ] P6-019 Verify cooperative `tick` budgeting prevents long browser main-thread stalls.
- [ ] P6-020 Verify CSS size, physical size, and DPR changes are handled correctly.
- [ ] P6-021 Verify pointer cancellation and browser lifecycle events leave engine state valid.
- [ ] P6-022 Verify web rendering consumes real prepared frames rather than duplicate text pagination.
- [ ] P6-023 Run the shared conformance scenarios through WASM where tooling allows.

## Android host

- [ ] P6-024 Verify host survives surface recreation without reopening the book.
- [ ] P6-025 Verify lifecycle and memory-pressure mappings.
- [ ] P6-026 Verify byte-based open contract supports future Storage Access Framework integration.
- [ ] P6-027 Verify logical/physical viewport and density handling.
- [ ] P6-028 Define the minimal JNI/Kotlin ABI without implementing product features in Rust core.
- [ ] P6-029 Add a rendering-surface spike that keeps reader behavior in `EngineRuntime`.
- [ ] P6-030 Verify platform font provisioning and fallback contract.
- [ ] P6-031 Run the shared conformance scenarios on Android host where tooling allows.

## Phase gate

- [ ] P6-GATE Native, WASM, and Android paths share one reader behavior implementation.
- [ ] P6-GATE Shared scenarios produce equivalent snapshots, locators, outcomes, and ranges.
- [ ] P6-GATE Adapter-specific behavior is limited to ABI, scheduling, lifecycle, input mapping, and surfaces.
- [ ] P6-GATE Platform capability differences are explicit.

---

# Phase 7 — Engine v1 Stabilization

## API and compatibility

- [ ] P7-001 Freeze the Engine v1 public API candidate.
- [ ] P7-002 Freeze/version durable locator and semantic DTO schemas.
- [ ] P7-003 Publish compatibility and semantic-versioning policy.
- [ ] P7-004 Add migration notes for deprecated exports and adapters.
- [ ] P7-005 Verify docs examples compile or are tested.

## Capability and limitations

- [ ] P7-006 Publish supported format matrix by target.
- [ ] P7-007 Publish reflow/fixed-layout capability matrix.
- [ ] P7-008 Publish typography/script/ruby/vertical-writing limitations.
- [ ] P7-009 Publish search, hit-test, selection, and locator guarantees.
- [ ] P7-010 Publish rendering backend and transition capability matrix.
- [ ] P7-011 Mark Curl3D stable or keep it explicitly experimental.

## Quality and release readiness

- [ ] P7-012 Complete dependency and license audit.
- [ ] P7-013 Complete panic, overflow, and unsafe-code audit.
- [ ] P7-014 Complete lifecycle, cancellation, and memory audit.
- [ ] P7-015 Run formatting checks.
- [ ] P7-016 Run focused package tests for all core and backend crates.
- [ ] P7-017 Run target checks for WASM and Android.
- [ ] P7-018 Run workspace clippy with warnings denied.
- [ ] P7-019 Run workspace tests and record approved unrelated failures if any.
- [ ] P7-020 Run representative corpus and performance suite.
- [ ] P7-021 Publish Engine v1 integration guide.
- [ ] P7-022 Publish troubleshooting and known-limitations guide.

## Engine v1 final gate

- [ ] V1-GATE Engine builds independently of inherited desktop.
- [ ] V1-GATE Core has no forbidden product/platform/backend dependencies.
- [ ] V1-GATE Native, WASM, and Android host share the runtime contract.
- [ ] V1-GATE Publication identity and locator recovery are durable and tested.
- [ ] V1-GATE Reflow, navigation, hit testing, and selection invariants pass.
- [ ] V1-GATE Backend-neutral frames support a reusable rendering backend.
- [ ] V1-GATE None and Slide transitions are reliable.
- [ ] V1-GATE Warm frames satisfy no-heavy-work requirements.
- [ ] V1-GATE Memory pressure and lifecycle cancellation are safe.
- [ ] V1-GATE Malformed input has bounded failure behavior.
- [ ] V1-GATE Public API and durable DTOs are documented and versioned.
- [ ] V1-GATE Known limitations and target capability differences are published.

---

# Post-v1 Backlog

These items are intentionally outside the Engine v1 critical path.

- [ ] POST-001 Evaluate production-quality Curl3D after stable fallback and performance gates.
- [ ] POST-002 Evaluate an additional rendering backend to validate backend neutrality.
- [ ] POST-003 Improve vertical-writing support.
- [ ] POST-004 Improve ruby annotation support.
- [ ] POST-005 Expand fixed-layout and comic-specific reading primitives.
- [ ] POST-006 Evaluate optional persistent search-index extension points without adding database ownership to core.
- [ ] POST-007 Add C ABI or Apple bindings when a real consumer exists.
- [ ] POST-008 Evaluate extracting the engine into a dedicated repository after API stabilization.
- [ ] POST-009 Optionally migrate inherited desktop to `EngineRuntime` if maintaining it as an official consumer becomes a priority.

---

# Platform-Owned Backlog Reference

These are intentionally **not engine tasks**. They may be copied into platform-specific plans when those projects begin.

- [ ] PLATFORM Android Activity, JNI glue, Surface integration, and Storage Access Framework UI.
- [ ] PLATFORM Web application shell, file picker, DOM accessibility bridge, and browser persistence.
- [ ] PLATFORM Desktop or mobile library/database.
- [ ] PLATFORM Bookmark/highlight/note persistence and user-facing management.
- [ ] PLATFORM Navigation History, Back, and Peek policy using engine locators and navigation primitives.
- [ ] PLATFORM WebDAV/cloud sync and conflict UI.
- [ ] PLATFORM AI, translation, dictionary, and OCR provider integrations.
- [ ] PLATFORM TTS playback and operating-system media integration.
- [ ] PLATFORM AccessKit, Android accessibility, and web ARIA/DOM adapters.
- [ ] PLATFORM Credentials, updater, telemetry, store, and account features.

The presence of these reference items must not be used to introduce their implementation or dependencies into core engine crates.
