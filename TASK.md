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
- [x] P0-011 Classify exports as stable core, optional helper, experimental, legacy compatibility, or accidental.
    - Evidence: `docs/ARCHITECTURE.md` section 17 classifies the engine facade, DTOs, configuration re-exports, lower-level reader compatibility exports, transition APIs, and optional Vello backend.
- [x] P0-012 Decide whether `ReaderSession` and lower-level reader types should remain re-exported by the product-facing facade.
    - Decision: retain them temporarily as compatibility/helpers for advanced consumers and migration, but require new platform adapters to use `EngineRuntime`/`EngineReader` and do not expand the low-level surface casually.
- [x] P0-013 Decide whether persistent `Bookmark`, `Highlight`, and `HighlightColor` entities belong in engine API.
    - Decision: these are platform persistence/product entities and do not belong in the engine API; the engine retains `LocatorV1`, `SourceRange`, `SearchResult`, and overlay ranges.
- [x] P0-014 Replace product-shaped annotation entities with persistence-neutral overlay/source primitives if P0-013 removes them.
    - Evidence: removed unused persistent entities from `crates/engine/src/features.rs` and their exports from `crates/engine/src/lib.rs`; existing `OverlaySet` and `SourceRange` remain available.
    - Validation: `cargo check -p rebook-engine --locked`, `cargo test -p rebook-engine --locked` — 15 passed.
- [x] P0-015 Mark transition and Curl APIs as stable or experimental explicitly.
    - Evidence: `crates/engine/src/transition.rs` and `FrameTransition` rustdoc mark transition APIs/Curl as experimental; `docs/ARCHITECTURE.md` classifies Curl3D as optional backend infrastructure outside the Engine v1 critical path. Existing Curl3D WIP files were not modified.
- [x] P0-016 Define a deprecation policy for accidental public exports before removing them.
    - Evidence: `docs/ARCHITECTURE.md` section 17 defines additive-first compatibility, preferred replacements, deprecation duration, and deliberate removal policy.

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

- [x] P1-001 Document how `PublicationId` is generated for every enabled format.
    - Evidence: `docs/ARCHITECTURE.md` records SHA-256 byte identity for EPUB, MOBI/KF8/MOBI6, FB2, CBZ, CHM, and normal PDF opens, plus the deliberate PDF import-ID override.
- [x] P1-002 Test publication ID stability for repeated opens of identical bytes.
    - Evidence: `identical_epub_bytes_produce_a_stable_publication_id` opens the same immutable EPUB bytes twice and compares `Book.id`; all byte-backed format constructors derive IDs from the same source bytes.
    - Validation: `cargo test -p rebook-formats --locked` — 43 passed, 1 ignored.
- [x] P1-003 Test collision and changed-content behavior for publication IDs.
    - Evidence: `changed_cbz_bytes_produce_different_publication_ids` proves changed source bytes produce different IDs; collision resistance is the documented SHA-256 cryptographic assumption rather than a meaningful runtime fixture.
- [x] P1-004 Audit `SpineItemId`, source node IDs, and `SourceAnchor` generation across format adapters.
    - Evidence: direct-source adapters use `section-{n}` IDs and the shared HTML parser; EPUB uses manifest IDs; PDF uses page-index text ranges; CBZ image pages have no text anchors. Findings are recorded in `docs/ARCHITECTURE.md`.
- [x] P1-005 Audit source-anchor stability when parser normalization changes.
    - Evidence: parser node IDs are stable only for equivalent parser output; normalization/parser changes may invalidate exact anchors and are covered by the locator quote/progression fallback policy in `docs/ARCHITECTURE.md`.
- [x] P1-006 Audit href normalization, percent encoding, fragments, and relative URL resolution.
    - Evidence: `PublicationUrl` canonicalization and boundary matrix cover decoding, relative paths, fragments, query stripping, encoded traversal, invalid escapes, and external schemes.
    - Validation: `cargo test -p rebook-publication --locked` — 10 passed.
- [x] P1-007 Audit internal links, missing targets, footnotes/endnotes, and cross-section anchors.
    - Evidence: HTML and EPUB tests cover canonical internal hrefs, missing-target failure behavior, footnote/endnote roles, and authored fragment anchors; reader tests cover cross-section locator fallback.
    - Validation: `cargo test -p rebook-html --locked` — 64 passed; `cargo test -p rebook-formats --locked` — 43 passed, 1 ignored; `cargo test -p rebook-reader --locked` — 63 passed.
- [x] P1-008 Audit resource identity for images, fonts, SVG, math, PDF pages, and comic pages.
    - Evidence: resources use canonical publication URLs; CBZ and PDF generated paths are deterministic, while image/font/SVG/math resources remain source hrefs resolved through the same URL boundary. Findings are recorded in `docs/ARCHITECTURE.md`.
- [x] P1-009 Document fixed-layout versus reflowable capability representation.
    - Evidence: `RenditionLayout`, fixed page dimensions, image pages, and `FixedPageTextLayer` are documented as distinct from reflowable blocks in `docs/ARCHITECTURE.md`.

## Reading IR audit

- [ ] P1-010 Build a coverage matrix for paragraphs, headings, nested lists, quotes, code, tables, images, SVG, math, links, and language metadata.
- [x] P1-011 Audit ruby annotation support and document current limitations.
    - Evidence: `docs/ARCHITECTURE.md` records that ruby is not modeled as separate base/annotation semantics; generic descendant text may remain readable without pronunciation provenance.
- [x] P1-012 Audit vertical-writing support and document current limitations.
    - Evidence: `docs/ARCHITECTURE.md` records that `writing-mode` and `text-orientation` are not represented; `WritingSystem` remains a coarse script hint only.
- [x] P1-013 Audit accessibility metadata preserved by parsers, including alt text, roles, labels, and reading order.
    - Evidence: `rebook-html` preserves image `alt`, presentational roles, authored language hints, hidden navigation, and selected navigation roles; `docs/ARCHITECTURE.md` records the missing full ARIA tree, landmark model, and label relationships.
- [x] P1-014 Audit malformed markup recovery and ensure diagnostics retain useful context.
    - Evidence: `HtmlError::InvalidDocument` retains the publication resource and parser message; `malformed_markup_keeps_resource_context_in_diagnostics` locks this behavior.
    - Validation: `cargo test -p rebook-html --locked` — 64 passed.
- [x] P1-015 Define a normalized ingestion contract for platform-supplied derived publications such as OCR output without adding provider logic to engine.
    - Evidence: `docs/ARCHITECTURE.md` defines a normalized `BookSource`/publication contract for derived snapshots and keeps OCR providers, confidence, networking, credentials, and persistence outside the engine.

## Locator durability

- [x] P1-016 Define and document the locator restoration priority chain.
    - Evidence: `docs/ARCHITECTURE.md` defines exact source, structural, unique quote, href progression, total progression, and failure order.
- [x] P1-017 Populate `LocatorV1::text` for current reading locators with bounded before/highlight/after context.
    - Evidence: `ReaderSession::current_locator` now derives a quote from the first visible source-backed text region in `crates/reader/src/session.rs`; bounds are 64/128/64 Unicode scalars in `crates/reader/src/model.rs`.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed; `cargo check -p rebook-engine --locked` — passed.
- [x] P1-018 Evaluate whether a structural locator such as partial CFI can be produced reliably; implement or explicitly defer with evidence.
    - Evidence: parser node IDs are synthetic and no CFI producer/validator exists; `LocatorV1::partial_cfi` remains storage-only and CFI generation is explicitly deferred in `docs/ARCHITECTURE.md`.
- [x] P1-019 Implement bounded text-quote recovery scoped to the locator href.
    - Evidence: `ReaderSession::restore_locator` searches source-backed text regions across the href's prepared segments, accepts one visible match, and falls back when the quote is absent or ambiguous.
    - Validation: `locator_quote_recovers_after_source_node_changes` covers Unicode text and page-spanning quotes; `cargo test -p rebook-reader --locked` — 63 passed.
- [ ] P1-020 Return a structured recovery quality: exact, structural, quote match, href fallback, total fallback, or failure.
- [x] P1-021 Add locator schema/version migration tests.
    - Evidence: publication tests now reject unsupported `LocatorV1` versions and verify `LocatorV1::at_start` defaults for optional recovery fields.
    - Validation: `cargo test -p rebook-publication --locked` — 10 passed.
- [x] P1-022 Add tests for parser-node identity changes while text remains equivalent.
    - Evidence: `locator_quote_recovers_after_source_node_changes` changes the source node identity while preserving the text and verifies recovery at the relocated content.
- [x] P1-023 Add tests for missing href and moved section fallback.
    - Evidence: `locator_falls_back_to_total_progression_when_href_moves` verifies a changed href uses total progression; `locator_rejects_unknown_href_without_a_fallback` verifies typed failure.
- [x] P1-024 Add tests ensuring ambiguous text quotes do not silently choose unrelated content.
    - Evidence: `ambiguous_locator_quote_falls_back_without_picking_a_match` verifies repeated quote matches are rejected before href/progression fallback.
- [x] P1-025 Add tests for locator round-trip across viewport, font, margin, spacing, and spread changes.
    - Evidence: `durable_locator_survives_reader_style_matrix_changes` restores one source locator across viewport, font-size, margin, paragraph-spacing, and double-spread variants.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed.

## Untrusted content and limits

- [x] P1-026 Audit archive entry count, decompression ratio, total expanded size, and path traversal limits.
    - Evidence: EPUB and CHM already enforce archive/entry/expanded-size budgets and path validation; CBZ now enforces archive bytes, entry count, per-entry bytes, total expanded bytes, and compression-ratio limits. CBZ materializes generated resource paths rather than extracting archive names.
    - Validation: `cargo test -p rebook-formats --locked` — 43 passed, 1 ignored.
- [~] P1-027 Audit XML/HTML recursion, entity, and allocation limits.
    - Partial evidence: EPUB XML sanitization enforces 128-level depth and rejects unsafe internal subsets; `rebook-html` now rejects sections over 64 MiB, over 1,000,000 DOM nodes, or over 256 ancestor levels. Direct HTML accepts predefined/numeric XML references and rejects undeclared entities. EPUB/FB2/CBZ/CHM bounded read paths are covered, but MOBI/KF8 record/table-derived allocations still lack a unified format-wide budget.
    - Validation: `cargo test -p rebook-html --locked` — 64 passed.
- [x] P1-028 Audit image dimension and decoded-byte limits before allocation.
    - Evidence: `rebook-layout` now validates image dimensions before decode and bounds both decoded raster bytes and source bytes at 32M pixels/128MiB; source-provided rasters receive the same guard.
    - Validation: `cargo test -p rebook-layout --locked` — 102 passed.
- [~] P1-029 Audit PDF and comic page dimension arithmetic for overflow.
    - Partial evidence: PDF now rejects non-finite/non-positive page dimensions before scale arithmetic and clamps raster output; direct adversarial coverage exercises zero, negative, NaN, and infinite dimensions. CBZ bounds archive/resource bytes, and layout rejects oversized decoded dimensions; a dedicated CBZ metadata fixture remains open.
    - Validation: `cargo test -p rebook-formats --locked` — 43 passed, 1 ignored.
- [~] P1-030 Add malformed/adversarial fixtures that are safe to commit.
    - Partial evidence: HTML size/depth/entity rejection tests, a bounded CBZ compression-ratio fixture, changed-CBZ identity coverage, and PDF invalid-dimension coverage are committed; broader malformed EPUB/PDF/image fixtures and MOBI/KF8 allocation fixtures remain open.
    - Validation: `cargo test -p rebook-html --locked` — 64 passed; `cargo test -p rebook-formats --locked` — 43 passed, 1 ignored.
- [~] P1-031 Add fuzz/property targets for the highest-risk parser and URL/anchor boundaries.
    - Partial evidence: `publication_url_boundary_matrix_preserves_canonical_invariants` exercises relative paths, fragments, encoded traversal, invalid escapes, NULs, backslashes, and external schemes without adding a fuzzing dependency; parser fuzz targets and broader anchor generation remain open.
    - Validation: `cargo test -p rebook-publication --locked` — 10 passed.
- [x] P1-032 Reject stale source anchors instead of silently restoring to section/page zero.
    - Evidence: `ReaderSession::position_for_source_anchor` now returns `NavigationTargetNotFound` when no source fragment, segment, or page contains the anchor; `stale_source_anchor_is_rejected_without_moving_reader` covers state preservation.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed; `cargo test -p rebook-engine --locked` — 15 passed.

## Phase gate

- [ ] P1-GATE Durable locator recovery is implemented and reports recovery quality.
- [ ] P1-GATE Source identity and internal navigation behavior are covered across representative formats.
- [ ] P1-GATE Parsing untrusted input has documented and tested resource limits.
- [ ] P1-GATE Reading IR capability and limitation matrix is current.

---

# Phase 2 — Reading Core Correctness

## State ownership audit

- [x] P2-001 Inventory state owned by `ReaderSession`, `EngineReader`, and `EngineRuntime`.
    - Evidence: `docs/ENGINE_RUNTIME_API.md` separates committed session state, reader transient interaction state, and runtime book/viewport/lifecycle state.
- [x] P2-002 Remove or document duplicated current-position, locator, selection, navigation, and transition state.
    - Evidence: committed position and locator recovery remain in `ReaderSession`; `EngineReader` pending/interactive/selection values are documented as transient facade state and are not durable position.
- [x] P2-003 Define committed reading state separately from prepared navigation and visual transition state.
    - Evidence: runtime docs state that prepare/poll/commit and interactive destinations do not change committed position until commit.
- [x] P2-004 Document thread-safety and worker ownership for native reader sessions.
    - Evidence: native prefetch ownership, generation-tagged stale-result rejection, single session installation, and one-owner runtime driving are documented in `docs/ENGINE_RUNTIME_API.md`.
- [x] P2-005 Document cooperative scheduling ownership for single-threaded WASM.
    - Evidence: runtime docs define `EngineRuntime::tick` as the cooperative advancement path and prohibit a second WASM reader state machine.

## Layout and invalidation

- [x] P2-006 Build an invalidation matrix for viewport, DPR, font set, typography, margins, spread mode, source refresh, overlays, and transitions.
    - Evidence: `docs/ENGINE_RUNTIME_API.md` defines generation, semantic-position, cache, and interaction effects for each change class.
- [x] P2-007 Verify physical-size/DPR-only changes do not trigger logical reflow.
    - Evidence: `EngineRuntime::resize` and `physical_resize_does_not_reflow_logical_layout` keep logical viewport/layout unchanged while updating physical surface dimensions.
    - Validation: `cargo test -p rebook-engine --locked` — 15 passed.
- [x] P2-008 Verify logical viewport changes preserve semantic location.
    - Evidence: `resize_rebuilds_layout_and_preserves_approximate_progress` and locator reflow tests cover logical viewport changes with source/progression restoration.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed.
- [x] P2-009 Verify style changes preserve semantic location.
    - Evidence: font/style and locator matrix tests cover font size, margins, paragraph spacing, typesetting, and source-backed restoration.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed.
- [x] P2-010 Verify single/double spread changes preserve semantic location.
    - Evidence: `durable_locator_survives_reader_style_matrix_changes` and double-spread reader tests cover spread transitions and source-backed restoration.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed.
- [ ] P2-011 Test odd/even spread boundaries and blank companion-page policy.
- [x] P2-012 Test section, segment, and fixed-layout page boundaries.
    - Evidence: reader tests cover cross-section spreads, segment/page boundaries, fixed-page placeholders, and continuous fixed pages.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed.
- [ ] P2-013 Make invalidation reasons observable in diagnostics where broad cache clears hide the cause.

## Navigation primitives

- [x] P2-014 Verify prepare never changes committed locator or snapshot.
    - Evidence: `prepared_navigation_leaves_state_unchanged_and_commits_cleanly` compares position, locator, and snapshot before/after preparation.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed.
- [x] P2-015 Verify cancel preserves committed state.
    - Evidence: the prepared-navigation test cancels a ready token and verifies position/locator remain unchanged.
- [x] P2-016 Verify commit moves exactly once.
    - Evidence: the prepared-navigation test commits once, verifies the destination, and rejects reuse after cancellation; navigation scheduler tests cover the ready-over-ticks path.
- [x] P2-017 Verify stale and double-used navigation tokens cannot commit.
    - Evidence: `navigation_tokens_are_invalidated_by_resize_and_style` and prepared-navigation tests reject stale poll/commit/cancel operations.
- [x] P2-018 Verify resize/style/source changes invalidate prepared navigation.
    - Evidence: resize/style token invalidation tests and the invalidation matrix cover generation changes; source refresh rebuilds repository/navigation state.
- [x] P2-019 Verify current and destination spreads stay simultaneously available during preparation.
    - Evidence: prepared navigation exposes source/destination positions without changing the current snapshot; frame tests cover destination spread state during interactive transitions.
- [x] P2-020 Add engine navigation commands for locator, TOC item, href/fragment, and source anchor.
    - Evidence: `EngineReader` and `EngineRuntime` expose locator, TOC, `go_to_href`, and `go_to_source` commands; `facade_exposes_internal_href_and_source_navigation` covers the facade routing and typed stale-anchor failure.
    - Validation: `cargo test -p rebook-engine --locked` — 15 passed.
- [~] P2-021 Ensure navigation command results report moved, pending, boundary, approximate restore, and error states consistently.
    - Partial evidence: `EngineReader`/`EngineRuntime` now return navigation outcomes for locator, TOC, href, and source commands; `EngineNavigationState` reports pending/moved/boundary and typed errors propagate. Approximate locator recovery quality remains open under P1-020.
- [x] P2-022 Expose sufficient primitives for platform-owned History/Back/Peek without storing the history stack in engine.
    - Evidence: runtime exposes locators, snapshots, semantic navigation commands, and explicit outcomes while `docs/ENGINE_RUNTIME_API.md` assigns navigation history/Back policy to platforms.

## Hit testing and selection

- [ ] P2-023 Define a stable deep-hit result for text, links, images, references, and block context.
- [x] P2-024 Verify hit testing across primary and secondary pages.
    - Evidence: reader tests resolve exact and nearest hits on the secondary page with spread offsets and source-backed selection.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed.
- [ ] P2-025 Verify hit testing after reflow and overlay-only updates.
- [x] P2-026 Verify word, sentence, and paragraph selection for Latin, CJK, mixed scripts, punctuation, and code.
    - Evidence: reader selection tests cover mixed Latin/CJK text, punctuation expansion, sentence/paragraph boundaries, and source-backed text geometry.
    - Validation: `cargo test -p rebook-reader --locked` — 63 passed.
- [x] P2-027 Verify semantic selection across logical page boundaries.
    - Evidence: `paragraph_selection_covers_continuations_across_logical_pages` preserves source ranges and rectangles across page continuations.
- [x] P2-028 Decide and document whether selection may cross spine sections.
    - Decision: engine selection stays within one authored spine section; cross-section joining is a platform citation/product policy, documented in `docs/ENGINE_RUNTIME_API.md`.
- [ ] P2-029 Verify table-cell and nested-list selection semantics.
- [ ] P2-030 Verify selection geometry regenerates from source ranges after reflow.
- [x] P2-031 Expose source-backed overlay input without persistent annotation business entities.
    - Evidence: `EngineReader`/`EngineRuntime` accept highlight/focus `SourceRange` overlays and keep them separate from platform bookmark/highlight persistence entities.

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

- [x] P3-001 Define Engine v1 use cases and the minimum call sequence for each.
    - Evidence: `docs/ENGINE_V1_API.md` defines open/resume, semantic navigation, frame production, source-backed selection, and lifecycle/memory-pressure sequences.
- [x] P3-002 Group API operations into open, inspect, configure, command, tick, frame, query, lifecycle, and close.
    - Evidence: `docs/ENGINE_V1_API.md` groups the current `EngineRuntime` operations and assigns ownership to engine versus platform.
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

- [x] P4-001 Audit `PreparedReaderFrame` for backend-specific assumptions.
    - Evidence: `docs/ENGINE_V1_API.md` records that prepared frames contain reader/layout DTOs only and no window, surface, GPU, texture, database, or product types.
- [x] P4-002 Audit frame cloning for large retained structures or pixel buffers.
    - Evidence: frame DTO cloning is shallow over the retained reader spread structures; the frame contract forbids backends from retaining borrowed references beyond the frame lifetime.
- [x] P4-003 Verify frame keys and revisions uniquely identify layout/content/overlay changes.
    - Evidence: `PageFrameKey`, `SpreadFrameKey`, `layout_generation`, `content_revision`, and `overlay_revision` roles are documented in `docs/ENGINE_V1_API.md`.
- [x] P4-004 Define which changes require page rebuild, scene rebuild, overlay rebuild, transform update, or no redraw.
    - Evidence: the prepared-frame invariants and `docs/ENGINE_RUNTIME_API.md` invalidation matrix separate page, scene, overlay, transform, and no-reflow changes.
- [x] P4-005 Verify destination frame lifetime remains valid through commit/cancel.
    - Evidence: prepared frames own current/destination spread DTOs; engine transition tests cover destination availability and cancellation without changing committed state.
    - Validation: `cargo test -p rebook-engine --locked` — 15 passed.

## Vello backend boundaries

- [x] P4-006 Verify `rebook-vello-backend` does not own reader semantics.
    - Evidence: `rebook-vello-backend` consumes `PreparedReaderFrame`/spread data and owns scene/cache composition; reader navigation and locator state remain in core crates.
- [x] P4-007 Verify `rebook-renderer` remains free of Vello/wgpu types.
    - Evidence: renderer uses backend-neutral `PaintScene`/retained display-list types; architecture dependency audit records no Vello/wgpu dependency.
- [x] P4-008 Separate static page content from dynamic overlays and transitions.
    - Evidence: prepared frame overlays/revisions and backend scene-layer APIs separate retained page content, source overlays, and transition transforms.
- [x] P4-009 Audit scene cache invalidation for resize, style, spread, source, overlay, and transition changes.
    - Evidence: `SpreadSceneCache` keys static layers by `SpreadFrameKey`; logical resize/style and spread changes advance layout generations, source refresh now carries a monotonic generation across prefetch-worker replacement, overlays are composed outside static layers, and transition destinations use separate keys.
    - Validation: `cargo test -p rebook-reader --locked` — source-refresh frame-key regression passed; `cargo test -p rebook-engine --locked` — 15 passed.
- [x] P4-010 Audit image and PDF raster identity across scenes and uploads.
    - Evidence: `PageDisplayList::image_data` retains each `ImageData` Blob used by scene composition; `frame_images` enumerates current and destination spread images before GPU upload; the adapter deduplicates uploads by the Blob's globally unique ID. PDF pages use unique page resource paths and page-indexed per-publication raster caches, while recompilation creates fresh Blob IDs.
    - Validation: `cargo test -p rebook-vello-backend --locked` — 8 passed; `cargo check -p rebook-engine-wasm --target wasm32-unknown-unknown --locked` — passed.
- [x] P4-011 Verify current and destination scenes are pinned during transitions.
    - Evidence: `PreparedNavigation` owns the destination spread through retained `Arc`-backed pages; `ReaderCompositor` holds current/destination `Arc<StaticSpreadLayers>` locals through composition, so LRU eviction cannot invalidate an in-flight scene. Resize/style/source lifecycle paths cancel interactive navigation before invalidation, and commit occurs only after settle.
    - Validation: `cargo test -p rebook-engine --locked` — 15 passed; `cargo test -p rebook-vello-backend --locked` — 8 passed.
- [x] P4-012 Add backend capability and fallback reporting.
    - Evidence: `WebReader::renderer_capabilities` exposes active renderer, WebGPU/CPU fallback state, supported slide/Curl paths, static scene caching, and the original GPU fallback reason as JSON; capability policy remains in the adapter rather than `rebook-engine`.
    - Validation: `cargo check -p rebook-engine-wasm --target wasm32-unknown-unknown --locked` — passed.

## None and Slide

- [x] P4-013 Verify None renders and commits deterministically without animation.
    - Evidence: `test_prepared_reader_frame` confirms repeated `None` frames retain the same key/revisions, report no next frame, leave animation idle, and preserve committed position.
    - Validation: `cargo test -p rebook-engine --locked` — 15 passed.
- [!] P4-014 Verify Slide uses prepared source/destination scenes.
    - Blocked: `ReaderCompositor` and `CpuSurfaceRenderer` consume prepared current/destination spreads for `FrameTransition::Slide`, but `EngineReader::frame` currently emits Curl for interactive navigation and has no stable Slide producer.
    - Affected: `crates/engine/src/reader.rs`, `crates/vello-backend/src/compositor.rs`, `crates/engine-wasm/src/cpu_surface.rs`.
    - Next: define transition selection and a stable Slide producer before end-to-end verification; keep Curl outside the Engine v1 critical path.
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
