# Torto Ebook Engine Roadmap

## 1. Purpose

This document is the strategic roadmap for extracting, hardening, and shipping a standalone ebook engine from this repository.

It answers:

- what the engine is;
- what the engine is not;
- which existing code is authoritative, inherited, experimental, or platform-specific;
- which audits, refactors, and capabilities must be completed;
- the order in which work should happen;
- the quality gates required before moving to the next phase.

Detailed executable work is tracked in [`TASK.md`](TASK.md). This file must not become a duplicate task checklist.

## 2. Repository Context

This repository is a fork and contains an inherited desktop application from the original author.

The inherited desktop application is useful as:

- a source of proven parsing, layout, rendering, and reading behavior;
- a compatibility and regression reference;
- a corpus of real product use cases;
- an optional consumer of the new engine.

It is **not** the target architecture and is **not** the source of truth for engine ownership.

The project priority is:

> Build a small, correct, measurable, platform-neutral ebook engine first. Product features and operating-system integrations are implemented by each platform outside the engine.

Migrating the inherited desktop application is optional and must never block engine progress unless a change would regress shared low-level crates that desktop still consumes.

## 3. Product Vision

The engine should let any platform build a high-quality reader without reimplementing ebook semantics.

A platform provides bytes, viewport constraints, fonts, reader preferences, high-level commands, lifecycle signals, and a rendering target. The engine returns stable publication data, reading state, semantic locations, layout/render data, and deterministic operation results.

The engine must make these guarantees possible:

1. The same publication has stable semantic identity across platforms.
2. Reflow, resize, typography changes, and spread changes preserve reading meaning.
3. Navigation can be prepared, committed, or cancelled without corrupting current state.
4. Selection and hit testing resolve to source-backed semantic ranges rather than visual page coordinates alone.
5. Parsing, layout, decoding, and display-list compilation do not occur in a warm animation frame.
6. Native, Android, and WASM consumers exercise the same reader behavior.
7. Platform services can persist engine DTOs without copying engine algorithms.

## 4. Scope and Ownership

### 4.1 Engine core owns

The engine core owns only platform-independent ebook processing and reading behavior:

- format detection and publication opening from immutable bytes;
- publication metadata, spine, sections, resources, links, and TOC;
- normalized semantic Reading IR;
- reflowable and fixed-layout representation;
- typography inputs, shaping, layout, pagination, and spread semantics;
- backend-neutral retained display lists;
- semantic reading location and durable locator restoration;
- reader session state and progression;
- navigation primitives and prepare/poll/commit/cancel transactions;
- internal href, anchor, TOC, and source-range resolution;
- semantic hit testing and text/image context;
- source-backed text selection and range geometry;
- content search primitives where semantic parsing is required;
- current/adjacent content caching and prefetch policy;
- cooperative work scheduling for single-threaded targets;
- lifecycle-safe cancellation and memory-pressure signals;
- semantic output required by platform accessibility adapters;
- stable diagnostics and metrics for engine work.

### 4.2 Rendering backend owns

A rendering backend such as `rebook-vello-backend` owns:

- converting backend-neutral display lists into backend scenes;
- scene and GPU-resource caching;
- static/dynamic layer composition;
- surface-independent rendering preparation;
- backend-specific page transitions and effects;
- backend metrics and resource invalidation.

A backend must not own publication parsing, reading location, pagination, selection semantics, or persistence.

The core engine must not depend on Vello, wgpu, a window system, DOM, Android, or Apple frameworks.

### 4.3 Platform/app owns

Each platform application owns:

- library and bookshelf behavior;
- file picker, content URI, drag-and-drop, and permissions;
- database and persistence;
- bookmarks, highlights, notes, tags, and their lifecycle;
- history stack and user-facing Back/Peek policy;
- sync, WebDAV, cloud accounts, and conflict UI;
- AI, translation, dictionary, OCR provider orchestration, and networking;
- TTS playback, voices, media controls, and speech settings;
- credentials and secure storage;
- reader chrome, menus, dialogs, onboarding, and settings UI;
- clipboard, share, external links, and haptics;
- accessibility bridge to the operating system or DOM;
- updater, analytics, telemetry, store, social, and DRM services;
- platform event loop, window/surface, and application lifecycle translation.

Platforms may store or display `LocatorV1`, `SourceRange`, semantic snapshots, and overlay inputs. They must not duplicate locator recovery, pagination, navigation transactions, source mapping, or hit testing.

### 4.4 Explicit non-goals for the engine

The engine will not become a complete ebook application. Do not add:

- SQLite or another product database;
- WebDAV or sync protocols;
- API clients for AI, OCR, translation, or dictionary providers;
- platform keyrings;
- app update logic;
- bookshelf UI or file management;
- account, social, recommendation, or gamification systems;
- platform accessibility APIs;
- platform-specific TTS implementations;
- a second desktop product architecture merely to match inherited code.

## 5. Current Baseline

The repository already contains a substantial engine foundation:

```text
book bytes
  -> rebook-formats / rebook-html
  -> rebook-publication
  -> rebook-layout
  -> rebook-renderer
  -> rebook-reader
  -> rebook-engine
  -> backend/platform adapter
```

Current important components:

| Area                   | Current role                       | Baseline assessment                                                              |
| ---------------------- | ---------------------------------- | -------------------------------------------------------------------------------- |
| `crates/formats`       | Format adapters                    | Mature, broad native format support; feature closure still requires audit        |
| `crates/html`          | Reflow parsing                     | Mature but large; semantics and source identity require focused audit            |
| `crates/publication`   | Canonical model                    | Correct architectural center; locator durability needs strengthening             |
| `crates/layout`        | Shaping/layout/pagination          | Working core; correctness and invalidation matrix need formalization             |
| `crates/renderer`      | Backend-neutral display lists      | Working retained boundary; API size and backend leakage need audit               |
| `crates/reader`        | Session/navigation/cache/selection | Strong behavioral kernel; public/internal boundaries need cleanup                |
| `crates/engine`        | Product-neutral facade/runtime     | Exists and is usable; currently exposes some low-level and product-shaped types  |
| `crates/vello-backend` | Shared Vello compositor            | Extracted and active; Curl3D is experimental/WIP                                 |
| `crates/engine-wasm`   | WASM/WebGPU adapter                | Exists; dependency closure, ABI, scheduling, and production readiness need audit |
| `apps/android`         | Android native host boundary       | Host exists; no JNI/Activity/platform product yet                                |
| `apps/engine-demo`     | Engine lab and benchmark consumer  | Primary native conformance/profiling consumer                                    |
| `apps/desktop`         | Inherited desktop product          | Reference and compatibility consumer, not target architecture                    |

Several milestones in the previous `PLANS.md` are already implemented: the engine facade, runtime, demo, shared compositor, navigation preparation, event-driven rendering, metrics, cache policy, slide transition, interactive drag, WASM adapter, and Android host boundary. They are now baseline to audit and harden, not future milestones.

## 6. Architectural Rules

### 6.1 Dependency direction

Allowed direction:

```text
formats/html -> publication -> layout -> renderer -> reader -> engine
                                                        |
                                                        v
                                             optional render backend
                                                        |
                                                        v
                                                 platform adapter
```

Actual crate dependencies may include shared lower-level models, but no dependency may point from core engine crates into an app or platform crate.

Forbidden in `rebook-engine`:

- egui or another application UI toolkit;
- winit or platform window APIs;
- wgpu, Vello, or concrete GPU surfaces;
- wasm-bindgen, web-sys, JNI, Android, or Apple frameworks;
- networking, database, sync, keyring, updater, and provider SDKs.

### 6.2 One behavioral implementation

New platforms must use `EngineRuntime` or an explicitly versioned successor. ABI crates may translate types and schedule calls but must not implement a second reader state machine.

Lower-level public crates remain usable for tests, tooling, and the inherited desktop application. They are not the preferred product-facing integration path.

### 6.3 Commands over raw platform input

The stable engine contract should prefer semantic commands:

- next/previous;
- navigate to locator/TOC/source;
- begin/update/end selection;
- set style/viewport;
- prepare/commit/cancel navigation;
- tick and prepare frame.

Raw touch, mouse, wheel, keyboard, and hardware-key arbitration is platform-specific. Existing pointer recognition in `rebook-engine` must be audited and either:

1. reduced to an optional platform-neutral interaction helper; or
2. moved to an adapter module that emits semantic engine commands.

The reader kernel must not require a touch metaphor.

### 6.4 Persistence-neutral DTOs

Engine DTOs describe ebook meaning, not storage policy. IDs, timestamps, tags, sync conflict metadata, provider configuration, and database concerns belong to platforms unless required to identify publication content.

### 6.5 No speculative refactors

Large modules are split only when one of these is true:

- ownership is currently wrong;
- a platform adapter is forced to depend on internals;
- testing is blocked;
- a hot path cannot be measured or isolated;
- a module has multiple independent reasons to change.

File size alone is not sufficient justification.

## 7. Definition of the Engine v1 Contract

Engine v1 is not defined by the number of formats or visual effects. It is defined by a stable behavioral contract.

At minimum, a platform must be able to:

```text
create runtime
open publication bytes
inspect metadata and TOC
configure fonts/style/viewport
restore a durable locator
obtain current snapshot and locator
obtain a backend-neutral prepared frame
navigate and poll pending work
resolve href/TOC/source targets
hit-test visible content
create and update semantic text selection
supply external overlay ranges
resize/reflow without losing semantic context
handle suspend/surface loss/memory pressure
close and release resources
```

The API must define:

- ownership and lifetime of book bytes and resources;
- sync versus cooperative/asynchronous operations;
- cancellation behavior;
- invalidation and revision behavior;
- error taxonomy;
- thread-safety expectations;
- target-specific capability differences;
- serialization/versioning policy for durable DTOs.

## 8. Delivery Strategy

Work proceeds through gated phases. A later phase may be explored, but it cannot be declared complete while an earlier mandatory gate is open.

Each phase follows this sequence:

1. Audit and record evidence.
2. Freeze or clarify the relevant contract.
3. Refactor only what blocks the contract.
4. Implement missing behavior.
5. Add tests and metrics.
6. Validate narrow targets, then broader targets.
7. Update `TASK.md` with evidence.

## 9. Phase 0 — Rebaseline and Protect Boundaries

### Objective

Establish an accurate current-state baseline and prevent inherited product code or experiments from silently defining the engine architecture.

### Audit

- Map dependencies among all core, backend, adapter, demo, and inherited desktop crates.
- Classify every public export from `rebook-engine` as core contract, helper, legacy compatibility, or accidental exposure.
- Audit `rebook-engine` for product entities and platform assumptions.
- Audit direct dependencies in `engine-demo` and `engine-wasm` that bypass the engine facade.
- Audit feature flags and native/WASM format closure.
- Classify Curl3D and shader work as experimental until validated.
- Record current build, test, clippy, target, and dependency-tree baselines.

### Refactor

- Add documentation that marks `apps/desktop` as inherited/reference code.
- Correct stale architecture documents that still describe the compositor as desktop-owned.
- Remove or deprecate persistent `Bookmark`/`Highlight` entities from the engine facade if they have no engine-owned behavior; retain source-range overlay primitives.
- Introduce explicit module/status labels for stable, experimental, and adapter-only APIs.
- Remove unused or unjustified direct low-level dependencies from adapters and the demo.

### Gate

Phase 0 is complete when:

- dependency direction is documented and mechanically inspectable;
- `rebook-engine` has no forbidden dependencies;
- public API ownership is classified;
- current target build failures are recorded rather than assumed;
- legacy desktop and experimental Curl code are clearly outside the engine-v1 completion gate.

## 10. Phase 1 — Publication and Semantic Identity

### Objective

Make publication semantics and durable content identity trustworthy enough for every later feature.

### Audit

- Audit source anchors across EPUB, FB2, MOBI/KF8, CHM, PDF, and CBZ.
- Audit publication ID stability and collision behavior.
- Audit href normalization, internal links, fragments, footnotes, and resource identity.
- Audit malformed archive/document handling and resource limits.
- Audit fixed-layout versus reflowable capability representation.
- Audit Reading IR coverage for headings, paragraphs, lists, quotes, code, tables, images, math, ruby, vertical writing, language, and accessibility metadata.

### Refactor

- Split format-specific identity generation from common publication identity policy where currently mixed.
- Centralize normalized href/fragment resolution.
- Version any parser behavior that affects durable anchors.
- Keep provider-generated OCR/translation data outside the engine; define a normalized publication ingestion boundary if platforms need to supply derived content.

### Develop

- Populate locator text context for durable recovery.
- Implement a documented locator restoration chain: exact source anchor, structural locator when available, nearby text quote, href progression, total progression.
- Add diagnostic recovery outcomes so platforms can distinguish exact, approximate, and fallback restoration.
- Add conformance fixtures for malformed and adversarial content without committing copyrighted books.

### Gate

- Locators survive viewport and typography changes.
- Recovery degrades explicitly rather than silently jumping to an unrelated location.
- Internal href/anchor resolution behaves consistently across supported formats.
- Publication parsing has bounded resource behavior for untrusted input.

## 11. Phase 2 — Reading Core Correctness

### Objective

Freeze the behavioral invariants of layout, pagination, session state, navigation, and selection.

### Audit

- Audit all state owned by `ReaderSession`, `EngineReader`, and `EngineRuntime` for duplication.
- Audit style/resize/source invalidation and generation counters.
- Audit single/double spread behavior at section and fixed-layout boundaries.
- Audit continuous/reflow/fixed mode assumptions in shared crates.
- Audit selection across logical pages, sections, mixed scripts, tables, and images.
- Audit prefetch cancellation, worker shutdown, and single-threaded cooperative scheduling.

### Refactor

- Keep one authoritative current position and one authoritative locator path.
- Separate committed reading state from transient prepared navigation and visual transition state.
- Reduce unnecessary re-exports of `ReaderSession` internals from the product-facing facade.
- Split reader modules only where needed to enforce ownership and tests.
- Make invalidation reasons explicit instead of relying on broad cache clears where practical.

### Develop

- Add semantic navigation targets for locator, TOC item, href/fragment, and source anchor.
- Preserve non-committing prepare/poll/commit/cancel semantics for linear page navigation.
- Expose enough primitives for a platform to implement history, Back, Peek, and bookmarks without putting their policy into the engine.
- Complete semantic hit-test results for text, links, images, references, and relevant block context.
- Complete source-backed selection operations and geometry regeneration after reflow.

### Gate

- Reflow and mode/style changes preserve semantic reading context.
- Prepared navigation never changes committed state before commit.
- Cancel and stale tokens cannot move the reader.
- Selection remains source-backed and can regenerate geometry.
- Native worker and WASM cooperative paths pass the same behavioral conformance suite.

## 12. Phase 3 — Engine API v1 and Adapter Contract

### Objective

Provide a coarse, stable API that platforms can consume without importing lower-level reader/layout/renderer crates.

### Audit

- Compare native demo, WASM, and Android host call paths.
- Identify all direct low-level imports required only because the facade is incomplete.
- Audit API calls that expose backend types, platform input, storage policy, or unstable internals.
- Audit error conversion and cancellation at ABI boundaries.

### Refactor

- Organize the engine API around open, inspect, configure, command, tick, frame, query, and close operations.
- Separate stable core commands from optional interaction helpers and experimental transitions.
- Replace hot-path JSON with typed or zero-copy-friendly ABI representations.
- Add explicit capability reporting for formats, fixed layout, search, selection, and backend requirements.
- Define compatibility and deprecation policy before declaring v1 stable.

### Develop

- Complete Android-safe and WASM-safe runtime operations.
- Add structured engine status: idle, pending work, frame ready, needs frame, boundary, recoverable error, fatal error.
- Add operation IDs or tokens where cooperative work can outlive one call.
- Add semantic accessibility snapshot primitives; platform adapters remain responsible for OS integration.
- Add API examples for native Rust, WASM, and Android host consumers.

### Gate

- Demo, WASM, and Android host use the same high-level runtime for reader behavior.
- ABI adapters contain translation and scheduling, not pagination or reader logic.
- Stable API types contain no UI, database, network, or provider concepts.
- Public contract, errors, cancellation, and versioning are documented.

## 13. Phase 4 — Backend-Neutral Frames and Vello Backend

### Objective

Make rendering reusable, measurable, and independent from any application shell.

### Audit

- Audit `PreparedReaderFrame` for backend leakage and unnecessary cloning.
- Audit static versus dynamic scene invalidation.
- Audit image/PDF resource identity and upload behavior.
- Audit Vello scene cache correctness across resize, style, spread, overlay, and source changes.
- Audit current Curl3D shaders for safety, portability, resource lifetime, and hot-frame work.

### Refactor

- Keep `rebook-renderer` backend-neutral.
- Keep Vello/wgpu dependencies confined to `rebook-vello-backend` and platform surface adapters.
- Separate static page content, dynamic overlays, transforms, and backend effects.
- Treat transition effects as optional backend capabilities; None must always work.
- Keep Curl3D behind an experimental feature/status until correctness and fallback behavior are proven.

### Develop

- Complete deterministic None and Slide paths first.
- Ensure destination content is prepared and pinned before transition frames.
- Add backend capability/fallback reporting.
- Add offscreen golden/perceptual tests where deterministic enough.
- Add robust surface/device-loss and zero-size handling at adapters.

### Gate

- Warm transition frames perform no parse, layout, display compilation, image decode, PDF rasterization, or target recreation.
- Transform-only animation does not rebuild static scenes.
- Overlay-only updates do not rebuild static page content.
- Backend absence or unsupported Curl falls back without changing reader state.

## 14. Phase 5 — Performance, Memory, and Reliability

### Objective

Turn performance expectations into repeatable budgets and prevent regressions on constrained devices.

### Audit

- Measure cold open, first readable frame, warm navigation, reflow, selection, and transition stages separately.
- Audit allocation volume and large `Arc`/pixel-buffer clones.
- Audit cache ownership across publication, layout, reader, compositor, images, and GPU.
- Audit idle CPU/GPU behavior and worker lifetime.
- Audit malformed-input CPU, memory, recursion, and decompression limits.

### Refactor

- Remove work from warm frames based on traces, not assumptions.
- Consolidate duplicate caches only when ownership and metrics prove duplication.
- Make cache budgets and memory-pressure reactions explicit.
- Add bounded scheduling so WASM and low-end devices remain responsive.

### Develop

- Maintain a local non-copyrighted or externally configured corpus matrix.
- Produce stable JSON benchmark output with schema versioning.
- Add CI-friendly smoke/performance regression checks with realistic tolerance.
- Add fuzz/property testing to parser, locator, navigation transaction, and dimension arithmetic boundaries.

### Initial budgets

Budgets are targets to validate and revise with evidence:

| Operation                                | Desktop development target | Mid-range mobile target |
| ---------------------------------------- | -------------------------: | ----------------------: |
| Input/command handling                   |                     < 1 ms |                  < 1 ms |
| Warm navigation decision                 |                   < 0.5 ms |                  < 1 ms |
| Static scene cache hit                   |                   < 0.5 ms |                  < 1 ms |
| Dynamic transform composition            |                     < 1 ms |                  < 2 ms |
| CPU work per 60 Hz frame                 |                     < 4 ms |                  < 6 ms |
| Complete GPU work per 60 Hz frame        |                    < 12 ms |                 < 14 ms |
| First readable page, representative EPUB |                   < 250 ms |                < 500 ms |

Report p50, p95, and p99. Do not hide unsupported GPU timing as zero.

### Gate

- Benchmarks are reproducible and stage-specific.
- Idle rendering is event-driven.
- Critical memory pressure retains only currently required content.
- Current and prepared destination content cannot be evicted mid-transaction.
- Security/resource-limit tests cover untrusted publication input.

## 15. Phase 6 — Cross-Platform Conformance

### Objective

Prove that the engine contract works across native, WASM, and Android boundaries without platform-specific reader implementations.

### Native demo

The demo remains a laboratory, not a product. It must support deterministic inspection, pagination, offscreen rendering, window rendering, input simulation, metrics, and corpus runs.

### WASM

The WASM adapter must prove:

- intended format feature closure;
- cooperative scheduling without long main-thread stalls;
- typed ABI for hot operations;
- correct DPR/resize behavior;
- browser lifecycle cancellation;
- real prepared-frame rendering rather than duplicate HTML pagination;
- measured raw, gzip, and Brotli release sizes.

### Android

The Android boundary must prove:

- stable runtime ownership across surface recreation;
- byte-based opening compatible with Storage Access Framework;
- viewport and density correctness;
- lifecycle and memory-pressure translation;
- rendering surface integration without reader logic in JNI/Kotlin;
- platform fonts and fallback behavior.

A production Android UI, library, database, TTS, accessibility bridge, and sync remain platform projects, not engine milestones.

### Gate

- One shared conformance suite produces equivalent snapshots, locators, navigation outcomes, and semantic ranges across targets.
- Target-specific differences are capability-reported and documented.
- No adapter duplicates reader semantics.

## 16. Phase 7 — Engine v1 Stabilization

### Objective

Declare a supportable engine release rather than an indefinitely moving prototype.

### Work

- Freeze the v1 public API and serialized durable DTO schemas.
- Publish supported format/capability matrix with known limitations.
- Publish threading, lifecycle, memory, and cancellation contracts.
- Add migration notes for deprecated facade exports.
- Complete licensing and dependency audit.
- Complete panic/unsafe/error-handling audit; workspace policy continues to forbid unsafe code.
- Document integration examples and troubleshooting.
- Define semantic versioning and compatibility policy.

### Definition of Done — Engine v1

Engine v1 is complete when:

- it builds independently of `apps/desktop`;
- core has no UI, GPU, network, database, sync, credential, updater, or provider dependencies;
- native demo, WASM, and Android host consume the same reader runtime contract;
- publication, locator, reflow, navigation transaction, hit-test, and selection invariants have conformance tests;
- backend-neutral frames are sufficient for at least one reusable backend;
- None and Slide are reliable; Curl may remain experimental;
- warm frames meet the no-heavy-work invariant;
- memory pressure and lifecycle cancellation are tested;
- malformed input has bounded failure behavior;
- public APIs and durable DTOs are versioned and documented;
- known limitations are explicit;
- all mandatory checks in `TASK.md` pass or have approved, documented exceptions.

## 17. Phase 8 — Post-v1 Options

Only begin these after Engine v1 stabilization unless evidence changes priority:

- additional rendering backends;
- production-quality Curl3D;
- richer fixed-layout and comic transitions;
- vertical writing and advanced ruby support;
- advanced accessibility semantics;
- incremental/persistent search indexes exposed as optional extension points;
- C ABI or Apple bindings;
- extraction into a dedicated repository;
- optional migration of the inherited desktop application to `EngineRuntime`.

Each option requires its own proposal and must preserve the core boundary.

## 18. Cross-Cutting Audit Matrix

The following audits repeat at phase gates:

| Audit         | Required evidence                                                  |
| ------------- | ------------------------------------------------------------------ |
| Dependency    | `cargo tree`, target-specific features, forbidden dependency check |
| Correctness   | Focused unit/integration tests and cross-target conformance        |
| API           | Public export diff, ownership classification, version impact       |
| Performance   | Stage metrics, p50/p95/p99, corpus and viewport metadata           |
| Memory        | Cache counts/budgets, pressure behavior, large allocation review   |
| Security      | Malformed archives/documents, limits, panic and overflow review    |
| Portability   | Native, WASM, Android checks and documented capability differences |
| Rendering     | Static/dynamic invalidation and no-heavy-work warm-frame evidence  |
| Documentation | Current architecture, limitations, and task evidence updated       |
| Licensing     | New dependencies and bundled assets reviewed                       |

## 19. Validation Strategy

Run the narrowest relevant checks first. Typical progression:

```bash
cargo fmt --all --check
cargo check -p rebook-publication
cargo test -p rebook-publication
cargo check -p rebook-reader
cargo test -p rebook-reader
cargo check -p rebook-engine
cargo test -p rebook-engine
cargo check -p rebook-vello-backend
cargo test -p rebook-vello-backend
cargo check -p rebook-engine-demo
cargo test -p rebook-engine-demo
cargo check -p rebook-engine-wasm --target wasm32-unknown-unknown
cargo check -p rebook-android-host --target aarch64-linux-android
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Target checks depend on installed toolchains and GPU/browser availability. A task may not be marked complete merely because validation could not run; record the blocker and leave the relevant verification item open.

Do not format, reset, overwrite, or clean unrelated user work. In particular, active Curl3D/shader work must be treated as user-owned WIP unless a task explicitly owns those files.

## 20. Agent Execution Protocol

Agents working from `TASK.md` must:

1. Read `AGENTS.md`, `PLANS.md`, and the relevant task section.
2. Record `git status --short` before editing.
3. Preserve unrelated and untracked work.
4. Inspect exact APIs before changing them.
5. Work on one bounded task ID or one explicitly related task group.
6. Avoid product/platform features inside core engine crates.
7. Add or update tests for behavior changes.
8. Run narrow validation before broader validation.
9. Update task status and evidence only after validation.
10. Document blockers without checking incomplete work.

A checked task means implementation and required validation are complete. Discovery alone should update notes but must not close an implementation task.

## 21. Stop Conditions

Stop and document the blocker rather than improvising a broad redesign when:

- a change would introduce a forbidden dependency into core;
- a platform requires duplicating reader behavior;
- durable identity cannot be preserved through a proposed refactor;
- optimization changes pagination, source mapping, selection, or locator semantics;
- an ABI needs unstable internal types;
- safe ownership appears to require unsafe code;
- a benchmark cannot isolate the stage being optimized;
- a task overlaps uncommitted user WIP;
- a supposedly engine-level feature is actually persistence, service, UI, or OS policy.

A blocker report must include affected paths/symbols, evidence, attempted bounded approach, alternatives, and recommended next action.
