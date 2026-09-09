# Torto Engine Demo Execution Plan

## 1. Objective

Build a small, measurable, platform-neutral reader-engine laboratory inside this repository. It must reuse Torto's existing parser, Reading IR, layout, retained renderer, and reader session; render book pages with Vello and wgpu without egui; and provide the foundation for deep rendering optimization and future Android integration.

The first usable result must support:

```text
book file or bytes
    -> rebook-formats / rebook-html
    -> rebook-publication Reading IR
    -> rebook-layout pagination
    -> rebook-renderer PageDisplayList
    -> rebook-reader ReaderSession
    -> demo Vello compositor
    -> offscreen texture or winit/wgpu surface
```

This plan is intentionally incremental. Complete and verify each milestone before starting the next one.

## 2. Repository Context

Read `docs/ARCHITECTURE.md` before implementation. The existing reusable engine is already distributed across:

- `crates/publication`
- `crates/html`
- `crates/formats`
- `crates/math`
- `crates/layout`
- `crates/renderer`
- `crates/reader`

Reusable Vello scene construction currently lives under:

- `apps/desktop/src/reader/render/scene.rs`
- `apps/desktop/src/reader/render/vello.rs`

Desktop surface and egui texture integration live in:

- `apps/desktop/src/platform/gpu.rs`

Do not assume that code under `apps/desktop` is all UI. Do not copy the whole desktop reader into the demo.

## 3. Non-Negotiable Constraints

1. Reuse existing engine crates; do not fork or duplicate their implementations.
2. The demo must not depend on `rebook-desktop` or import source files from `apps/desktop` by path.
3. `rebook-engine` must not depend on egui, winit, wgpu, Vello, Tokio, networking, SQLite, keyring, sync, updater, AI, or desktop application modules.
4. The windowed demo may depend on winit, wgpu, and Vello, but not egui.
5. Preserve existing public APIs unless a milestone explicitly requires a small additive API.
6. Do not refactor the large core files merely to complete the demo. Internal modularization is a separate architecture task.
7. Do not implement Cover or Curl/3D in the initial execution pass.
8. Do not add AI, knowledge, annotation persistence, library management, settings UI, sync, or file pickers.
9. Open books from a command-line path; internally favor `open_bytes` so the engine remains compatible with Android content providers later.
10. No parsing, shaping, pagination, display-list compilation, image decoding, PDF rasterization, or render-target recreation may occur inside a warm animation frame.
11. Keep idle rendering event-driven. The demo must not continuously redraw when nothing changes.
12. Do not commit copyrighted test books. Use environment-provided local corpus paths.
13. Keep commits small and ensure the workspace builds after every milestone.

## 4. Intended Workspace Shape

Create:

```text
crates/engine/
  Cargo.toml
  src/
    lib.rs
    book.rs
    config.rs
    error.rs
    reader.rs

apps/engine-demo/
  Cargo.toml
  src/
    main.rs
    cli.rs
    commands/
      mod.rs
      inspect.rs
      paginate.rs
      render.rs
      window.rs
    render/
      mod.rs
      compositor.rs
      metrics.rs
      scene.rs
      scene_cache.rs
      target.rs
      vello.rs
    application.rs
    frame_clock.rs
    input.rs
    metrics.rs
    surface.rs
  corpus/
    README.md
    books.example.toml
```

Do not create `crates/vello-backend` initially. Keep the renderer/compositor in `apps/engine-demo/src/render` until it is independent, tested, and proven useful to a second consumer. A later milestone may mechanically promote it into a shared crate.

Add both new packages to the root workspace members.

## 5. Milestone 0 — Establish Baseline

### Tasks

1. Record initial `git status --short` and preserve unrelated user changes.
2. Run:

   ```bash
   cargo check --workspace
   cargo test --workspace
   ```

3. Record any pre-existing failures. Do not fix unrelated failures as part of this plan.
4. Inspect exact APIs before wrapping them:
   - `rebook_formats::{open_bytes, OpenedPublication, BookFormat}`
   - `rebook_reader::ReaderSession`
   - `ReaderSession::{open_with_fonts_at_locator,current_spread,prefetch_adjacent,try_turn_page,resize,current_locator}`
   - `rebook_renderer::PageDisplayList`
   - `apps/desktop/src/reader/render/vello.rs`
   - `apps/desktop/src/reader/render/scene.rs`
   - `apps/desktop/src/platform/gpu.rs`

### Deliverable

A short implementation note in the eventual PR/commit message describing baseline checks and any failures. Do not add a new baseline document unless necessary.

### Acceptance

- Existing failures are distinguished from regressions.
- No source changes are made during baseline inspection.

## 6. Milestone 1 — Add a Minimal `rebook-engine` Facade

### Purpose

Provide one stable entry point for future platform wrappers without moving or duplicating existing algorithms.

### Dependencies

`crates/engine/Cargo.toml` should initially depend only on:

```toml
rebook-formats = { path = "../formats" }
rebook-layout = { path = "../layout" }
rebook-publication = { path = "../publication" }
rebook-reader = { path = "../reader" }
rebook-renderer = { path = "../renderer" }
thiserror.workspace = true
```

Remove unused dependencies rather than retaining them speculatively.

### Initial API

Implement a thin facade around real existing APIs. Prefer transparent delegation over new abstractions.

```rust
pub struct Engine {
    fonts: Arc<[ReaderFontBlob]>,
}

pub struct EngineBook {
    opened: OpenedPublication,
}

pub struct EngineReader {
    session: ReaderSession,
}

pub struct EngineConfig {
    pub fonts: Arc<[ReaderFontBlob]>,
}

pub struct ReaderConfig {
    pub viewport: LayoutViewport,
    pub style: ReaderStyle,
    pub locator: Option<LocatorV1>,
}
```

Exact ownership may be adjusted to compile cleanly with existing non-`Clone` types. Do not invent unsafe lifetime workarounds.

Required methods:

```rust
Engine::new
Engine::open_bytes
Engine::create_reader

EngineBook::book
EngineBook::source
EngineBook::format
EngineBook::cover_bytes

EngineReader::book
EngineReader::snapshot
EngineReader::current_spread
EngineReader::current_locator
EngineReader::prefetch_adjacent
EngineReader::try_turn_page
EngineReader::resize
EngineReader::set_style
```

Expose existing engine types through method signatures where practical. Do not create an FFI-safe model yet.

### Error Handling

Create `EngineError` with transparent variants for format, publication, layout/reader errors actually crossed by the facade. Do not convert errors into strings internally.

### Tests

Add focused unit tests using a tiny synthetic or existing in-memory fixture:

- unsupported file name produces a typed error;
- opening bytes exposes `Book` metadata;
- reader creation produces a valid snapshot and current spread;
- locator can be read after creation.

### Acceptance

- `cargo check -p rebook-engine` passes.
- `cargo test -p rebook-engine` passes.
- `cargo tree -p rebook-engine` contains no egui, winit, wgpu, Vello, Tokio, reqwest, rusqlite, or desktop platform crates.
- No parser/layout/renderer code is copied into the facade.

## 7. Milestone 2 — Add the Headless Demo Shell

### CLI Contract

Support these commands without adding a large CLI framework unless it clearly reduces code:

```bash
cargo run -p rebook-engine-demo -- inspect BOOK
cargo run -p rebook-engine-demo -- paginate BOOK
cargo run -p rebook-engine-demo -- render BOOK --output PAGE.png
cargo run -p rebook-engine-demo -- window BOOK
```

Implement `inspect` and `paginate` first. `render` and `window` may return a clear “not implemented” error until their milestones are complete, but the parser should recognize their options.

### `inspect`

1. Read the selected file into `Arc<[u8]>`.
2. Call `Engine::open_bytes` with the file name.
3. Print:
   - format;
   - publication ID;
   - title/authors/languages/layout;
   - section count;
   - TOC entry count;
   - cover presence.
4. Avoid parsing every section unless `--all-sections` is explicitly added later.

### `paginate`

1. Accept logical width and height, with small safe defaults such as `800x1000`.
2. Create an `EngineReader`.
3. Obtain the current spread.
4. Print:
   - open duration;
   - reader creation/first-page duration;
   - current `ReaderSnapshot`;
   - primary and secondary page dimensions;
   - display-command and text-region counts;
   - segment cache count when exposed by the current reader API.
5. Queue adjacent prefetch.
6. Optionally traverse a bounded number of pages with `--pages N`. Never default to the whole book.

### Timing

Use `std::time::Instant`. Keep timing labels stable so results can be compared between revisions.

### Acceptance

- Both commands work without initializing wgpu or a window.
- Neither command imports desktop application code.
- Failures include the book path and retain the underlying error source.

## 8. Milestone 3 — Offscreen Vello Rendering

### Purpose

Prove that `PageDisplayList` can reach pixels without egui, winit, or the desktop reader aggregate.

### Vello Adapter

Create `apps/engine-demo/src/render/vello.rs` by adapting the behavior—not blindly copying unrelated code—from `apps/desktop/src/reader/render/vello.rs`.

Implement `anyrender::RenderContext` and `anyrender::PaintScene` for a small `VelloScene<'_>` wrapper around `vello::Scene`. Preserve:

- layers and clipping;
- strokes and fills;
- glyph drawing, normalized coordinates, synthetic emboldening and transforms;
- image drawing;
- box shadows if required by the trait.

If exact reusable code is identical, first consider moving the adapter into a neutral shared location. Do not make the engine facade depend on Vello merely to share 155 lines prematurely.

### Offscreen Target

Implement a renderer that:

1. Initializes a headless wgpu instance/adapter/device/queue.
2. Creates a reusable `Rgba8Unorm` storage/copy-source texture.
3. Builds a Vello scene from `ReaderSpread` and page offsets.
4. Calls `vello::Renderer::render_to_texture`.
5. Copies the texture into a mapped staging buffer with correct row padding.
6. Removes padding and writes a PNG.

Keep readback timing separate from render timing; GPU-to-CPU readback is diagnostic work, not part of an interactive frame.

### Initial Scene Composition

For the first version:

```text
new Scene
  -> primary.paint_at(primary_offset_x)
  -> optional secondary.paint_at(secondary_offset_x)
```

Use existing `PageDisplayList` replay methods. Do not inspect its private display commands.

### Metrics

Record separately:

- scene construction CPU time;
- Vello render submission CPU time;
- readback/map time;
- PNG encoding time;
- output dimensions;
- referenced image count if available.

### Tests

- Unit-test row-padding removal.
- Unit-test physical dimension validation and overflow handling.
- Run a smoke render when an adapter is available; allow a clear skip in environments with no compatible adapter.

### Acceptance

- `render BOOK --output PAGE.png` creates a valid, non-empty PNG.
- No egui dependency appears in `cargo tree -p rebook-engine-demo`.
- The output uses `PageDisplayList` replay, not a second layout/render implementation.
- Render target dimensions are clamped and validated before allocation.

## 9. Milestone 4 — Extract a Demo Compositor

### Purpose

Separate static retained-page scene construction from GPU target/surface ownership and later animation transforms.

### Types

Introduce equivalents of:

```rust
pub struct PageSceneKey {
    pub position: ReaderPosition,
    pub layout_generation: u64,
}

pub struct SpreadSceneKey {
    pub primary: PageSceneKey,
    pub secondary: Option<PageSceneKey>,
    pub width: u32,
    pub height: u32,
}

pub struct StaticSpreadScene {
    pub scene: Arc<vello::Scene>,
    pub images: Arc<[peniko::ImageData]>,
    pub key: SpreadSceneKey,
}

pub struct OverlaySet {
    pub highlights: Vec<SourceRange>,
    pub selection: Vec<SourceRange>,
    pub focus: Vec<SourceRange>,
}
```

Use a real pagination/layout generation if the reader exposes one. If it does not, keep cache lifetime scoped to a reader instance and explicitly clear it on resize/style/source change; do not fabricate a generation that can collide.

### Layering

Match the useful existing desktop behavior:

```text
static underlay: background + images
dynamic overlays: highlights + selection + focus
static foreground: non-image content
dynamic foreground borders/icons as required
```

The first demo may use an empty `OverlaySet`; design the composition boundary so overlays can be added without rebuilding static glyph/image scenes.

### Scene Cache

Implement a small LRU with these properties:

- current spread is pinned;
- a prepared destination spread may also be pinned;
- key is based on page identity and valid layout generation/lifetime;
- animation progress is never part of the static cache key;
- changing only a transform does not rebuild the static scene;
- changing only an overlay does not rebuild the static page scene;
- cache exposes hit, miss, build, eviction, and approximate entry count metrics.

Start with entry-count limits. Add byte budgeting only when size estimation is trustworthy.

### Acceptance

- Offscreen rendering uses `ReaderCompositor` rather than constructing scenes in the command handler.
- Rendering the same spread twice produces a cache hit.
- Resize/style changes clear or invalidate stale scenes.
- A transform-only frame performs zero static scene builds.

## 10. Milestone 5 — Minimal winit/wgpu Window

### Purpose

Create a fast render lab for interactive profiling, not a user-facing application.

### Application State

Keep the aggregate small:

```rust
struct DemoApplication {
    engine: Engine,
    book: EngineBook,
    reader: EngineReader,
    compositor: ReaderCompositor,
    gpu: SurfaceRenderer,
    transition: TransitionController,
    metrics: RuntimeMetrics,
    viewport: PhysicalSize<u32>,
    dirty: DirtyState,
}
```

Adjust ownership to satisfy winit/wgpu surface lifetimes without unsafe code; workspace policy forbids unsafe code.

### Input

Initial controls:

```text
Right / Space    next spread
Left             previous spread
1                transition None
2                transition Slide (after Milestone 8)
F1               toggle metrics in window title/logging
R                invalidate/re-render current scene
Escape           close
window resize    resize reader and surface
```

Do not add file picker, toolbar, settings screen, or text UI.

### Surface Renderer

The surface owner is responsible only for:

- wgpu instance/adapter/device/queue;
- surface configuration and recovery;
- reusable Vello renderer;
- render target/surface size;
- rendering a composed Vello scene;
- presentation.

It must not call parser, layout, reader navigation, or cache policy directly.

### Event-Driven Rendering

Represent invalidation explicitly:

```rust
enum DirtyState {
    Clean,
    Scene,
    Surface,
    Animation,
}
```

Request redraw only for input, resize/exposure, destination readiness, scene invalidation, or active animation. When idle, `AboutToWait` must not continuously request redraw.

### Acceptance

- `window BOOK` shows the current spread.
- Left/right navigation works with `None` transition.
- Resize preserves a valid reading location and renders at the new viewport.
- Closing the window terminates workers and GPU resources cleanly.
- CPU usage approaches idle when no animation or background preparation is active.

## 11. Milestone 6 — Performance Instrumentation

### Stable Pipeline Metrics

Track at least:

```rust
pub struct PipelineMetrics {
    pub file_read: Duration,
    pub publication_open: Duration,
    pub reader_create_and_first_page: Duration,
    pub scene_build: Duration,
    pub gpu_submit: Duration,
    pub gpu_readback: Option<Duration>,
}
```

If accurate section-parse/layout/display-compile timings require invasive changes, add a small optional observer/instrumentation interface in the owning crate. Do not infer those timings by subtracting unrelated measurements.

### Frame Metrics

Track:

- frame interval;
- event-to-submit CPU duration;
- animation update duration;
- scene composition duration;
- static scene cache hit/miss;
- static scene builds per frame;
- render-target recreations per frame;
- image upload/atlas refresh count when observable;
- destination readiness latency;
- missed 60 Hz and 120 Hz deadlines.

Report p50, p95, and p99 over a bounded rolling window. Average alone is insufficient.

### Output

Support:

```bash
--metrics text
--metrics json
--metrics-file PATH
```

Do not add CSV until a consumer requires it.

### GPU Timing

Use wgpu timestamp queries only when adapter capabilities support them. Fall back cleanly to CPU submission/frame timing; timestamp queries must not be a runtime requirement.

### Acceptance

- Metrics distinguish cold first page, warm current-page render, warm navigation, and animation frames.
- JSON field names remain stable and include viewport, adapter/backend, spread mode, and transition kind.
- Metrics collection can be disabled and has negligible overhead when disabled.

## 12. Milestone 7 — Cache and Resource Policy

### Cache Layers

Treat these as separate ownership layers:

```text
BookSource/SectionRepository       parsed/prepared semantic sections
ReaderSession segment cache       PageDisplayList pages
ReaderCompositor scene cache       static Vello scenes
decoded image/PDF raster storage  CPU pixels
Vello/wgpu                         GPU atlas, targets and textures
```

Do not introduce a second cache for data already owned and bounded by `ReaderSession` without evidence.

### Resource Profiles

Add demo profiles:

```rust
pub enum ResourceProfile {
    Low,
    Balanced,
    High,
}
```

Profiles should initially configure only policies the demo can actually control, such as compositor entries, prefetch distance when supported, and optional render resolution. Do not expose fake limits for opaque Vello internals.

### Memory Estimates

Calculate exact sizes where possible:

```text
RGBA CPU image = width * height * 4
RGBA8 render target = width * height * 4
staging buffer = padded_bytes_per_row * height
```

For shared `Arc` data, distinguish referenced bytes from uniquely owned bytes. Label Vello atlas/scene sizes unknown or approximate when they cannot be measured reliably.

### Memory Pressure

Add a demo/compositor hook:

```rust
enum MemoryPressure {
    Moderate,
    Critical,
}
```

Policy:

- Moderate: drop speculative scene entries, retain current and prepared destination.
- Critical: cancel speculative preparation where possible and retain only content required to display/finish the active interaction.

Do not modify Android-specific lifecycle code in this milestone.

### Acceptance

- Current and active destination scenes cannot be evicted mid-transition.
- Critical pressure reduces observable cache entries.
- No multiplication overflow is possible in byte calculations.

## 13. Milestone 8 — Non-Committing Navigation Preparation

### Problem

Current `ReaderSession::try_turn_page` is non-blocking but commits as soon as the destination is ready. Slide needs both source and destination spreads while the current locator remains unchanged until animation completion.

### Required Contract

Add the smallest reader-level transaction that supports:

```text
prepare destination without committing
poll readiness without blocking
obtain source and destination spreads simultaneously
commit once
cancel without moving
invalidate stale preparation on resize/style/source change
```

Preferred conceptual API:

```rust
pub struct NavigationToken { /* opaque identity */ }

pub enum NavigationPreparation {
    Ready(PreparedNavigation),
    Pending(NavigationToken),
    Boundary,
}

pub struct PreparedNavigation {
    token: NavigationToken,
    direction: PageDirection,
    source: ReaderPosition,
    destination: ReaderPosition,
    destination_spread: ReaderSpread,
}

ReaderSession::prepare_navigation
ReaderSession::poll_navigation
ReaderSession::commit_navigation
ReaderSession::cancel_navigation
```

The exact representation must respect lifetimes and ownership of `Arc<PageDisplayList>`. Keep token internals private.

### Invariants

1. Preparation never changes `current_locator` or `snapshot`.
2. Cancel never changes position.
3. Commit changes position exactly once.
4. A token is tied to the active pagination/prefetch generation.
5. Stale tokens after resize, style change, source refresh, or superseding navigation are rejected or reported cancelled.
6. Direct navigation retains priority over speculative prefetch.
7. Boundary never creates a transition.
8. Double-page spreads move and commit as one navigation unit.

### Tests

- ready prepare leaves current state unchanged;
- pending prepare eventually becomes ready;
- cancel preserves snapshot and locator;
- commit installs the expected destination;
- double commit is rejected/no-op according to documented behavior;
- resize invalidates prepared navigation;
- style change invalidates prepared navigation;
- previous/next crossing a segment boundary works;
- previous/next crossing a spine section works;
- double spread produces the correct destination pages;
- prioritized navigation does not wait behind stale speculative work.

### Acceptance

- Demo can hold two complete retained spreads before changing current reader state.
- Existing `try_turn_page` behavior and tests remain compatible unless deliberately implemented through the new additive contract.

## 14. Milestone 9 — Slide Transition

### State Machine

Implement explicit state:

```rust
enum TransitionState {
    Idle,
    Preparing {
        direction: PageDirection,
        token: NavigationToken,
        requested_at: Instant,
    },
    Interactive {
        prepared: PreparedNavigation,
        progress: f32,
        velocity: f32,
    },
    Settling {
        prepared: PreparedNavigation,
        from: f32,
        to: f32,
        started_at: Instant,
        duration: Duration,
    },
}
```

An initial keyboard-triggered transition may skip `Interactive` and settle from 0 to 1. Pointer dragging is added after that path is stable.

### Composition

Prepare and cache source and destination static spread scenes once. Each animation frame changes only transforms and small dynamic overlays.

For a normalized progress `p`:

```text
source_x      = -direction_sign * p * viewport_width
destination_x =  direction_sign * (1 - p) * viewport_width
```

Append cached Vello scenes with `Affine::translate`. Animate the entire `ReaderSpread`, not each logical page independently.

### Animation Curve

Start with a simple monotonic ease-out curve and a bounded duration such as 160–260 ms. Centralize the curve and duration policy; do not scatter magic constants across input/render code.

### Commit/Cancel

- Commit only after a transition settles to `1.0`.
- Cancel only after it settles to `0.0`.
- A GPU/surface error must not silently commit navigation.
- If destination preparation fails, retain and redraw the source spread and report the error.

### Performance Invariants

Instrumentation must assert/report during warm animation:

```text
section parses                 == 0
layout calls                   == 0
display-list compilations      == 0
image decodes/PDF raster calls == 0
static scene builds            == 0 after preparation
render-target recreations      == 0
```

### Acceptance

- Keyboard next/previous performs a smooth Slide.
- None remains available as baseline and fallback.
- Source and destination are visually stable during animation.
- Locator changes only on successful commit.
- A transition can be cancelled before commit.
- A double spread moves as one surface.
- Warm Slide meets the initial frame budget in Section 17 on the development machine, or metrics clearly identify the missed stage.

## 15. Milestone 10 — Interactive Drag

### Gesture State

Track a bounded set of velocity samples:

```rust
struct DragGesture {
    pointer_id: u64,
    start_x: f32,
    current_x: f32,
    started_at: Instant,
    samples: VecDeque<VelocitySample>,
}
```

### Rules

- Horizontal intent must exceed a small slop before claiming the gesture.
- Clamp progress to a safe overscroll range; do not pass arbitrary values into transforms.
- Commit based on either distance threshold or directional release velocity.
- Cancel if neither threshold is met.
- Ignore/reject a second pointer in v1.
- Disable text selection/image interaction because the demo does not implement them.
- Resize or focus loss cancels safely.

Start with tunable defaults:

```text
distance threshold: approximately 28% of spread width
velocity threshold: approximately 700 logical pixels/second
settle duration: clamp to approximately 120–280 ms
```

Do not treat these values as final UX constants; expose them in one policy struct for device testing.

### Acceptance

- Drag follows the pointer without layout or static scene rebuild.
- Release commits/cancels predictably.
- Cancel returns to the exact original spread and locator.
- Slow drag and fast flick both work in both directions.

## 16. Milestone 11 — Deep Render Optimization

Only optimize after metrics identify a bottleneck. Apply changes in this order.

### 16.1 Remove Work from the Hot Frame

Audit traces and eliminate any parsing, layout, compilation, decode, raster, target allocation, or static scene construction from animation frames.

### 16.2 Reduce Scene Recomposition

- Keep static source/destination scenes immutable and cached.
- Keep overlays separate.
- Treat animation as transform-only composition.
- Avoid cloning large pixel buffers; clone `Arc`/cheap handles.

### 16.3 Reuse GPU Resources

- Recreate surface/targets only on actual size/format/device changes.
- Reuse command infrastructure supported by Vello/wgpu.
- Avoid creating staging buffers in the windowed path.
- Do not perform GPU readback during normal presentation.

### 16.4 Control Image Uploads

- Give resources stable identity based on publication/resource identity and decode parameters.
- Pin images used by current/destination spreads.
- Measure the existing Vello image-atlas refresh workaround before carrying it into the shared backend.
- If the workaround remains necessary, refresh only at scene installation/preparation when possible, not every animation frame.

### 16.5 Frame Pacing

- Render on redraw events rather than a busy loop.
- Request the next frame only while animation is active.
- Use actual elapsed time, not a fixed frame increment.
- Clamp very large time deltas after stalls/suspend.
- Log missed 60 Hz and 120 Hz deadlines separately.

### 16.6 Resolution Scaling

Do not implement dynamic resolution unless GPU fill rate is demonstrated as the bottleneck. If implemented later:

- keep it behind an explicit option/resource profile;
- avoid oscillation with hysteresis;
- restore full resolution after settling;
- evaluate text sharpness on real mobile-density screens.

### 16.7 Shader Boundary

Do not add a custom shader for None or Slide. Use cached Vello scene transforms and clipping. Cover may also use ordinary transform/clip/gradient composition.

Curl/3D is a future separate path:

```text
render source/destination spread scenes to textures
    -> draw a tessellated page mesh
    -> vertex deformation and perspective
    -> front/back fragment shading and shadows
    -> final wgpu surface
```

Do not put Curl shader logic into `rebook-renderer` or `PageDisplayList`.

## 17. Initial Performance Budgets

Budgets are targets for measurement and regression tracking, not reasons to hide errors or reduce correctness.

| Operation | Development desktop target | Future Android mid-range target |
|---|---:|---:|
| Input processing | < 1 ms | < 1 ms |
| Warm navigation decision | < 0.5 ms | < 1 ms |
| Static scene cache hit | < 0.5 ms | < 1 ms |
| Dynamic/transform composition | < 1 ms | < 2 ms |
| CPU work per 60 Hz frame | < 4 ms | < 6 ms |
| Complete GPU work per 60 Hz frame | < 12 ms | < 14 ms |
| First readable page | < 250 ms | < 500 ms |
| Warm prefetched page turn | no visible stall | no visible stall |

Track both frame deadlines:

```text
60 Hz  = 16.67 ms
120 Hz = 8.33 ms
```

Report p50, p95, and p99. Averages are not sufficient.

## 18. Corpus and Benchmark Matrix

Use local paths supplied by environment variables or a local ignored configuration:

```text
small Latin EPUB
CJK EPUB
image-heavy EPUB
large-section EPUB
table/math/footnote EPUB
text PDF
scanned PDF
high-resolution CBZ
legacy MOBI/AZW3
```

Run each relevant case as:

```text
cold process / warm process
cache miss / cache hit
single page / double spread
portrait / landscape
1x / high-density target
None / Slide
```

Benchmark stages:

```text
read bytes
open publication
create reader and first page
prepare current spread scene
render first frame
prepare adjacent destination
render deterministic transition frames
commit navigation
render settled destination
```

For deterministic Slide benchmarking, render a fixed sequence such as 120 normalized progress values without sleeping. Interactive frame pacing is measured separately in the windowed demo.

## 19. Metrics JSON Shape

Use a stable format similar to:

```json
{
  "schema_version": 1,
  "book_case": "cjk-epub",
  "viewport": { "width": 1080, "height": 1920 },
  "adapter": { "name": "...", "backend": "Vulkan" },
  "spread_mode": "single",
  "transition": "slide",
  "pipeline_ms": {
    "file_read": 2.1,
    "publication_open": 11.2,
    "first_readable_page": 34.8,
    "destination_prepare": 4.1
  },
  "frame_ms": {
    "cpu_p50": 1.2,
    "cpu_p95": 1.8,
    "cpu_p99": 2.3,
    "gpu_p95": 5.4
  },
  "cache": {
    "scene_hits": 238,
    "scene_misses": 2,
    "scene_builds": 2,
    "evictions": 0
  },
  "hot_path_work": {
    "section_parses": 0,
    "layouts": 0,
    "display_compiles": 0,
    "image_decodes": 0,
    "target_recreates": 0
  }
}
```

Use `null` for unavailable GPU metrics. Do not report fabricated zero values for unavailable measurements.

## 20. Test Plan

### Engine Facade Tests

- typed failure for unsupported format;
- open in-memory bytes;
- create reader and obtain spread;
- locator round trip through reader creation;
- resize preserves a valid durable location.

### Compositor Tests

- same spread produces a cache hit;
- transform-only frame produces no static scene rebuild;
- overlay change does not rebuild static glyph/image scene;
- destination pin prevents eviction;
- cache clear on layout invalidation;
- primary/secondary offsets match `ReaderSpread`.

### Navigation Transaction Tests

- prepare does not move;
- pending poll becomes ready;
- cancel preserves locator;
- commit moves exactly once;
- stale token cannot commit;
- resize/style/source changes invalidate preparation;
- cross-segment and cross-section navigation;
- double-spread semantics.

### Renderer Tests

- offscreen texture row padding;
- output dimension/overflow validation;
- empty/invalid surface sizes do not allocate;
- device/surface lost recovery where testable;
- render smoke test with graceful adapter absence.

### Animation Tests

- progress and transforms at 0, 0.5, and 1;
- previous/next directions;
- monotonic settle curve;
- cancel ends at source;
- commit ends at destination;
- no forbidden hot-path work after preparation;
- large elapsed-time delta clamps safely.

### Visual Validation

PNG output may be compared perceptually with a tolerance. Do not require byte-identical GPU output across backends and devices.

## 21. Commands Required at Each Milestone

Run the narrow checks first, then workspace checks:

```bash
cargo fmt --check
cargo check -p rebook-engine
cargo test -p rebook-engine
cargo check -p rebook-engine-demo
cargo test -p rebook-engine-demo
cargo clippy -p rebook-engine -p rebook-engine-demo --all-targets -- -D warnings
cargo check --workspace
cargo test --workspace
```

Do not run a source-changing formatter across unrelated user changes. Format only files changed for the milestone, then use `cargo fmt --check` for validation.

For dependency-boundary verification:

```bash
cargo tree -p rebook-engine
cargo tree -p rebook-engine-demo
```

For manual smoke tests:

```bash
cargo run -p rebook-engine-demo -- inspect "$BOOK"
cargo run -p rebook-engine-demo -- paginate "$BOOK"
cargo run -p rebook-engine-demo -- render "$BOOK" --output /tmp/torto-page.png
cargo run -p rebook-engine-demo -- window "$BOOK"
```

Use a task-specific environment variable such as `TORTO_DEMO_BOOK`; do not repurpose common system variables.

## 22. Commit Sequence

Implement in this order, with the workspace buildable after each commit:

1. `Add minimal rebook-engine facade`
2. `Add engine-demo inspect and paginate commands`
3. `Add offscreen Vello spread rendering`
4. `Add reusable demo spread compositor`
5. `Add compositor scene cache and metrics`
6. `Add minimal winit/wgpu window`
7. `Make demo redraw event-driven`
8. `Add stable pipeline and frame metrics`
9. `Add cache resource profiles and memory-pressure handling`
10. `Add non-committing reader navigation preparation`
11. `Add keyboard-driven Slide transition`
12. `Add interactive drag settle and cancel`
13. `Add deterministic transition benchmarks`
14. `Optimize measured render bottlenecks`
15. `Promote Vello compositor to a shared crate only if reuse is proven`

Do not combine navigation API work, compositor extraction, window setup, and animation into one commit.

## 23. Definition of Done — Engine Demo v1

The first engine demo is complete when all of the following are true:

- It builds independently from `apps/desktop`.
- It contains no egui, AI, network, database, sync, updater, shelf, or settings dependencies.
- It opens EPUB, PDF, and CBZ through `rebook-engine`.
- It renders a current spread to PNG offscreen.
- It renders a current spread directly to a winit/wgpu surface.
- Single- and double-page spreads use `ReaderSpread` semantics.
- Window resize produces valid new pagination and preserves a durable location.
- None and Slide transitions are available.
- Slide uses simultaneous cached source and destination spreads.
- Navigation commits only after successful animation completion.
- Warm Slide performs no parsing, layout, display compilation, image decoding, PDF rasterization, static scene building, or target recreation per frame.
- Idle mode does not continuously redraw.
- Metrics report cold/warm pipeline timings, p50/p95/p99 frame timings, cache behavior, adapter/backend, and forbidden hot-path work.
- Memory-pressure handling can shrink compositor caches without losing the visible spread.
- Existing workspace tests continue to pass, aside from explicitly recorded pre-existing failures.

## 24. Stop Conditions and Escalation

Stop and document the blocker instead of improvising a large redesign when:

- a required API cannot be added without breaking existing desktop behavior;
- sharing the Vello adapter would force GPU/UI dependencies into `rebook-engine`;
- wgpu surface lifetime handling appears to require unsafe code;
- accurate transition preparation requires cloning or duplicating `ReaderSession` state;
- an optimization changes pagination, source mapping, hit testing, or locator behavior;
- a dependency materially conflicts with the permissive-license direction;
- a benchmark cannot distinguish the stage being optimized.

When blocked, provide:

1. exact file and symbols involved;
2. the attempted minimal approach;
3. why it fails;
4. two bounded alternatives;
5. recommended alternative and compatibility impact.

## 25. Work Explicitly Deferred

Do not include these in engine-demo v1:

- Android JNI/Kotlin wrapper;
- iOS/macOS C ABI or XCFramework;
- annotations persistence;
- search/index database;
- knowledge graph or AI retrieval;
- accessibility platform bridge;
- production file picker/library management;
- Cover transition unless Slide is complete and measured;
- Curl/3D or custom page-deformation shaders;
- migration of the existing desktop application to the facade;
- extraction into a separate repository.

The next project after this plan should be an Android surface/font/file/lifecycle spike consuming the same `rebook-engine` and shared Vello compositor, not a second reader implementation.

---

# Part II — Web-First Engine Completion and WASM Recovery Plan

## 26. Mission

Turn `apps/engine-web` into the primary cross-platform reader-engine laboratory. It must exercise the same parsing, publication, pagination, retained display list, navigation, cache, locator, selection, TOC, and transition logic that native shells use. The web application is not allowed to implement a second reader.

The end state is:

```text
DOM / browser lifecycle / input / RAF
                 ↓ thin events
          rebook-engine-wasm
       ABI + cooperative scheduler
                 ↓ coarse API
            rebook-engine
 publication + reader + prepared frames
                 ↓
       shared Vello compositor
                 ↓
        WebGPU / native wgpu
```

The web shell may own DOM, file selection, HTTPS setup, `ResizeObserver`, `requestAnimationFrame`, pointer capture, accessibility elements, and developer panels. It must not own pagination, page text extraction, navigation transactions, locators, spread semantics, transition policy, cache policy, or semantic hit testing.

## 27. Mandatory Baseline Audit Before Editing

The branch is dirty and contains partially implemented work. Do not assume it compiles. Before changing anything, record:

```bash
git status --short
git diff --stat
git diff --check
cargo metadata --no-deps --format-version 1
cargo tree -p rebook-engine-wasm --target wasm32-unknown-unknown -e normal
```

Inspect and classify every current modification in:

- `crates/engine/**`
- `crates/engine-wasm/**`
- `crates/formats/**`
- `crates/reader/src/lib.rs`
- `apps/engine-web/**`
- root `Cargo.toml` and `Cargo.lock`

Known partial state that must be verified rather than trusted:

- target-specific native wgpu features and the RGBA intermediate target fix exist in `apps/engine-demo`;
- transition state was moved toward `rebook-engine`;
- a single-thread WASM prefetch implementation was added to `rebook-reader`;
- format features were started in `rebook-formats`;
- `engine-wasm` was trimmed toward `rebook-engine`-only dependencies;
- navigation was being changed from a blocking 64-iteration loop to a cooperative transaction;
- the web UI still renders concatenated text in an HTML `<article>`, not a `PageDisplayList`;
- generated `pkg/` artifacts may be stale and must never be treated as source truth;
- React type dependencies may be declared but not installed;
- a `ResizeObserver` cleanup bug was partially fixed and must be audited.
- the current `/usr/bin/wasm-opt` failed validation because the module uses bulk-memory operations while the installed Binaryen invocation did not enable bulk memory; AGY must upgrade/configure Binaryen or disable wasm-pack's optimizer and run a compatible explicit optimization pass. Do not silently ship the unoptimized artifact.

Create a baseline table in the implementation report:

```text
check | command | result | pre-existing/new | action
```

Do not reset or discard dirty work. Repair or supersede it in small reviewable commits.

## 28. Hard Architectural Boundaries

### 28.1 `rebook-engine`

This crate owns platform-independent reader behavior:

- opening immutable bytes through enabled format adapters;
- lifetime of the opened publication/source;
- reader creation and durable locator restoration;
- viewport, style, spread-mode, and reflow commands;
- current snapshot, locator, TOC, page/spread identity;
- navigation prepare/poll/commit/cancel;
- cache and prefetch policy;
- transition state and source/destination spread pinning;
- construction of backend-independent prepared-frame data;
- semantic hit testing, selection, source ranges, and deep-context lookup.

It must not depend on `wasm-bindgen`, `web-sys`, DOM, React, winit, egui, Android, JNI, Apple frameworks, or a concrete GPU surface.

### 28.2 `rebook-engine-wasm`

This crate is a thin adapter only:

- `wasm-bindgen` ABI types;
- conversion of JS-owned bytes and strings;
- mapping browser timestamps/pointers into engine events;
- cooperative scheduling entry points such as `tick(budget_ms)`;
- ownership of WebGPU objects only if the shared compositor requires a platform surface adapter;
- structured error/status conversion.

It must not duplicate reader logic. It should normally depend on `rebook-engine` plus the minimal ABI/platform crates. Direct dependencies on `rebook-reader`, `rebook-layout`, `rebook-publication`, or `rebook-renderer` require a written justification. Direct dependencies on Parley, ICU, Vello, or wgpu are only acceptable when implementing an actual backend boundary, never for unused placeholder modules.

### 28.3 `apps/engine-web`

This app owns only:

- DOM and CSS;
- file picker / drag-and-drop;
- canvas element and CSS size observation;
- device-pixel-ratio reporting;
- pointer, touch, wheel, and keyboard forwarding;
- RAF scheduling according to engine status;
- TOC/settings/debug presentation;
- loading/error/accessibility UI.

It must not concatenate page text, paginate, infer spreads, calculate transition progress, decide navigation commit/cancel, or implement source mapping.

## 29. Target Engine API Contract

Design an additive, coarse-grained Rust API. Exact names may follow repository conventions, but responsibilities must match:

```rust
Engine::open_bytes(bytes, file_name) -> EngineBook
Engine::create_reader(&EngineBook, ReaderConfig) -> EngineReader

EngineReader::set_viewport(ViewportSpec) -> WorkStatus
EngineReader::set_style(ReaderStyle) -> WorkStatus
EngineReader::begin_navigation(PageDirection) -> NavigationStatus
EngineReader::tick(Duration) -> TickResult
EngineReader::frame() -> PreparedReaderFrame
EngineReader::snapshot() -> ReaderSnapshot
EngineReader::toc_items() -> &[TocViewItem]
EngineReader::navigate_to_locator(&LocatorV1) -> WorkStatus
EngineReader::hit_test(Point) -> Option<DeepReaderContext>
```

`PreparedReaderFrame` must contain stable IDs/revisions and references to current and optional destination spreads. It must not expose DOM concepts. It should allow a backend to determine whether layout, retained scenes, textures, overlays, or only transforms changed.

The WASM ABI should be coarse:

```text
open_begin(bytes, file_name, viewport) -> operation id
tick(budget_ms) -> status/frame revision
resize(css_width, css_height, device_pixel_ratio)
navigate(direction)
pointer_down/move/up/cancel(...)
key(...)
render(timestamp_ms) -> WebFrameStatus
toc() -> structured JS value
snapshot() -> structured JS value
close()
```

Avoid returning JSON strings in the hot path. Use `serde-wasm-bindgen`, typed arrays, numeric enums, or stable wasm-bindgen structs. JSON is acceptable only for cold diagnostics until typed bindings exist.

## 30. Milestone W0 — Restore a Trustworthy Build

Tasks:

1. Finish or repair feature declarations in `crates/formats/Cargo.toml` and `src/lib.rs`.
2. Verify default native features still support every format.
3. Verify `rebook-engine` defaults remain source-compatible for desktop.
4. Verify `rebook-engine-wasm` enables only the intended web format set.
5. Fix cfg-specific unused imports and non-exhaustive matches cleanly.
6. Add missing React/DOM/WebGPU type packages and a Vite environment declaration.
7. Ensure generated `pkg/`, `dist/`, and local certificates are ignored unless intentionally versioned.

Required checks:

```bash
cargo check -p rebook-formats
cargo test -p rebook-formats
cargo check -p rebook-engine
cargo test -p rebook-engine
cargo check -p rebook-engine-wasm --target wasm32-unknown-unknown
cd apps/engine-web && bun install && bun run build
```

Done only when native default behavior and EPUB-only WASM both compile from a clean target directory.

## 31. Milestone W1 — EPUB-Only Bundle Closure

Implement Cargo features without changing native defaults:

```text
rebook-formats:
  default = all-formats
  epub
  mobi
  fb2
  cbz
  chm
  pdf

rebook-engine:
  default = formats-full
  web-epub = rebook-formats/epub
```

Make heavy dependencies optional and tied to features. At minimum, EPUB-only must exclude:

- `hayro` and PDF dependencies;
- `libchm` and CHM dependencies;
- MOBI/KF8 parser dependencies;
- FB2 dependencies;
- CBZ-only code.

Audit with:

```bash
cargo tree -p rebook-engine-wasm --target wasm32-unknown-unknown -e normal > /tmp/engine-wasm-tree.txt
rg 'hayro|libchm|mobi|vello|wgpu' /tmp/engine-wasm-tree.txt
```

For the current HTML-preview stage, the command must find no `hayro`, `libchm`, Vello, or wgpu. When real Vello rendering is added later, Vello/wgpu may return only through the explicit backend crate.

Build release and record raw and compressed sizes:

```bash
wasm-pack build crates/engine-wasm --release --target web --out-dir ../../apps/engine-web/pkg
wasm-opt -Oz apps/engine-web/pkg/rebook_engine_wasm_bg.wasm -o /tmp/rebook_engine_wasm_bg.opt.wasm
gzip -9 -c /tmp/rebook_engine_wasm_bg.opt.wasm > /tmp/rebook_engine_wasm_bg.wasm.gz
brotli -q 11 -c /tmp/rebook_engine_wasm_bg.opt.wasm > /tmp/rebook_engine_wasm_bg.wasm.br
ls -lh apps/engine-web/pkg/*.wasm /tmp/rebook_engine_wasm_bg.*
```

Initial acceptance budget before the GPU backend is included:

- raw optimized WASM: less than 15 MiB;
- Brotli transfer size: less than 6 MiB;
- no disabled parser symbols/dependencies in `cargo tree`;
- size regression threshold in CI: fail at more than 10% growth without an approved note.

Do not claim success based on Vite dev-server transfer size; always report release raw, gzip, and Brotli sizes.

## 32. Milestone W2 — Engine-Owned Cooperative Work Scheduler

The current WASM prefetch path calls `compile_segment()` synchronously from `try_recv()`. Replace this accidental behavior with an explicit cooperative work abstraction.

Requirements:

- Native continues using a background thread.
- Single-thread WASM owns a queue of explicit jobs.
- `tick(budget)` performs bounded work and reports whether more work remains.
- No `for 0..64` or other polling loop may block a JS call.
- Navigation token/state stays inside engine/adapter, never in React.
- Opening large books exposes loading state and supports cancellation/close.
- A stale generation cannot commit results after resize, style change, new book, or cancellation.

Recommended concepts, derived from current code rather than imposed as crates:

```text
WorkScheduler trait or internal enum
NativeWorker
CooperativeWorker
WorkQueue
WorkGeneration
TickBudget
TickResult
```

Important limitation: a monolithic `compile_segment()` cannot be preempted halfway. First bound work at section/segment granularity. If one segment still exceeds the frame budget on large chapters, instrument it and then introduce smaller parse/layout phases; do not pretend a loop around a synchronous call is cooperative.

Tests:

- pending navigation becomes ready over ticks;
- cancellation discards stale results;
- resize invalidates old generation;
- opposite-direction navigation cancels/replaces pending navigation;
- boundary navigation never mutates location;
- no WASM path calls `std::thread::spawn`;
- deterministic scheduler tests use synthetic clocks/budgets.

## 33. Milestone W3 — Publication and Large-Book Lifetime

Audit ownership and copies for `File → ArrayBuffer → Uint8Array → WASM → Arc<[u8]> → ZIP source`.

Requirements:

- quantify every full-file copy;
- remove avoidable `bytes.to_vec()` copies at the ABI boundary where possible;
- `EngineBook` or its source lifetime must remain explicit and testable;
- closing/replacing a book releases reader, source, caches, images, and pending jobs;
- opening a second book cannot retain the first archive accidentally;
- errors distinguish unsupported format, corrupt ZIP, resource budget, parse, layout, and GPU failure;
- resource limits remain enforced for archive size, entry count, expanded bytes, ratio, and XML depth.

Large-book tests should include generated non-copyright fixtures with:

- 1 MiB EPUB;
- 20–50 MiB EPUB with images;
- one very long chapter;
- thousands of short spine items;
- malformed ZIP/XML;
- oversized image/resource.

Record peak JS heap where available, WASM linear memory, open-to-first-frame latency, and memory after close/reopen.

## 34. Milestone W4 — Prepared Frame Boundary in `rebook-engine`

Stop exporting diagnostic page text as the rendering path. Add a platform-independent prepared-frame API around existing `ReaderSpread` and `PageDisplayList`.

The frame model must expose:

- source/current spread;
- optional navigation destination spread;
- stable page/spread keys;
- viewport and spread geometry;
- layout generation;
- scene/content revision;
- overlay revision;
- transition kind/state/progress/transforms;
- whether another frame or cooperative tick is required;
- locator/snapshot after committed navigation only.

The frame must preserve the existing semantic maps for hit testing and selection. AI/search/knowledge remain outside render preparation.

Tests must prove:

- reading a frame does not parse, paginate, or mutate reader position;
- source/destination remain pinned during a transition;
- cancel returns exactly to source without locator change;
- commit changes locator only after settle completes;
- double-page spreads preserve reading direction and pairing;
- resize produces a new generation and invalidates stale frames.

## 35. Milestone W5 — Shared Vello Compositor

Do not put reusable compositor logic inside `engine-wasm`. Promote only the already-proven platform-neutral pieces currently duplicated/misplaced under demo/desktop/WASM into one shared internal crate or module after confirming two consumers.

Likely reusable responsibilities:

- `PageDisplayList → Vello Scene` replay;
- static underlay/content layers;
- dynamic highlight/selection/focus overlays;
- spread scene keys and revisions;
- image-resource collection;
- scene cache;
- source/destination transform composition.

Keep outside it:

- DOM/canvas;
- winit window/surface lifecycle;
- egui texture registration;
- Android surface/JNI;
- RAF/event-loop policy.

Acceptance:

- native demo and WASM backend call the same compositor API;
- no copy-pasted `VelloScene`, `ReaderCompositor`, or cache implementation;
- retained scene build occurs only when its revision changes;
- transform-only animation does not rebuild static scenes;
- images are marked dirty only when new/changed, not every frame.

## 36. Milestone W6 — Real WebGPU Page Rendering

Replace `<article class="page-text">` with real engine output.

Required flow:

```text
EngineReader::frame
  → ReaderSpread/PageDisplayList
  → shared compositor
  → Vello Scene
  → RGBA8Unorm intermediate texture
  → fullscreen blit to preferred canvas format
  → GPUCanvasContext present
```

The intermediate RGBA texture is required because Vello storage texture format must match its pipeline; do not bind a BGRA sRGB canvas view where RGBA8Unorm is expected.

Implement:

- adapter/device initialization once;
- preferred canvas format selection;
- physical target size = CSS size × DPR;
- logical layout viewport remains CSS pixels unless engine scaling policy says otherwise;
- target recreation only on physical-size/format changes;
- image atlas refresh only for changed image identities;
- device-lost and surface/configuration error recovery;
- one render request per revision/animation frame;
- no continuous idle redraw.

Visual acceptance corpus:

- plain Latin EPUB;
- Vietnamese diacritics;
- CJK;
- RTL sample;
- inline emphasis/links;
- images;
- table;
- math;
- long paragraph crossing pages;
- fixed-layout sample when enabled.

For every corpus item, compare native demo and browser screenshots at the same logical viewport. Define tolerances for antialiasing, but page breaks, glyph order, geometry, images, and source ranges must agree.

## 37. Milestone W7 — Responsive Viewport and Reflow

The browser measures; the engine decides layout.

Web responsibilities:

- observe the actual reader surface, not the whole window;
- debounce/coalesce observer events to one RAF;
- pass CSS width, CSS height, DPR, safe-area insets, and desired spread policy;
- update canvas backing dimensions separately from CSS dimensions.

Engine responsibilities:

- derive page/spread geometry;
- preserve current durable locator across reflow;
- choose single/double spread according to explicit policy;
- invalidate layout/scene/texture generations correctly;
- prefetch the new neighbors after reflow.

Audit the `ResizeObserver` lifecycle: cleanup must be returned by the React effect itself, not from an inner async function. Ensure changing canvas backing dimensions does not recursively trigger reflow.

Test matrix:

```text
360×640 DPR 2/3       phone portrait
640×360 DPR 2/3       phone landscape
768×1024 DPR 2        tablet portrait
1024×768 DPR 2        tablet landscape
1366×768 DPR 1        low-end desktop
1920×1080 DPR 1/2     desktop
```

Acceptance: no document scrollbar, no clipped controls, no blank page caused by zero-size startup, stable locator after resize, and page breaks generated from the measured reader surface.

## 38. Milestone W8 — Navigation, TOC, and Locator Parity

Implement through engine APIs:

- previous/next page;
- keyboard navigation;
- boundary reporting;
- TOC retrieval and navigation;
- current active TOC path;
- progression;
- durable locator serialization/restoration;
- non-committing destination preparation;
- adjacent prefetch.

The web UI presents TOC but does not resolve hrefs or section indices itself.

Acceptance scenarios:

- open at beginning and traverse every page to end;
- traverse back to beginning;
- jump through every TOC entry;
- reload and restore locator;
- resize during pending navigation;
- open a new book during pending navigation;
- rapid alternating next/previous input;
- no duplicate commits and no page skip.

## 39. Milestone W9 — Touch, Pointer, and Slide

Web captures raw pointer events and forwards normalized samples. Engine owns gesture semantics and transition policy.

Required events:

```text
pointer_down(id, x, y, timestamp)
pointer_move(id, x, y, timestamp)
pointer_up(id, x, y, timestamp)
pointer_cancel(id, timestamp)
focus_lost()
```

Requirements:

- pointer capture after horizontal intent is claimed;
- 8 logical-pixel slop unless policy changes;
- vertical movement remains available to browser/shell until horizontal claim;
- distance and velocity commit thresholds come from engine policy;
- cancel and settle are monotonic;
- source/destination textures stay pinned;
- navigation commits only at settle completion;
- resize/focus loss/pointer cancel safely abort;
- respect `prefers-reduced-motion` by selecting `None` policy;
- no parser/layout/scene build/texture allocation during warm animation frames.

Support mouse, touch, and pen through Pointer Events; do not create separate touch logic.

## 40. Milestone W10 — Texture-Only Warm Animation

Before Slide begins:

1. prepare destination navigation without commit;
2. ensure source and destination display lists exist;
3. ensure static scenes exist;
4. rasterize source and destination into cached textures;
5. pin both textures;
6. start transition.

During every animation frame, do only:

- calculate progress/transform;
- draw two textured quads;
- submit/present;
- schedule next RAF if needed.

Forbidden during warm frames:

- EPUB/HTML parsing;
- section loading;
- text shaping;
- pagination;
- display-list compilation;
- image decoding;
- Vello static scene construction;
- texture creation/recreation;
- atlas rebuild;
- JSON serialization;
- React state update for every pointer sample.

Measure p50/p95/p99 CPU frame submission and GPU duration when timestamps are available. Test under Chrome CPU throttling and low-power integrated GPU profiles.

## 41. Milestone W11 — Selection, Hit Testing, and Deep Context

Forward screen coordinates through the backend transform into engine page coordinates. Use existing `PageDisplayList` semantic regions.

Expose:

- hit-tested glyph/text byte span;
- source range/anchor;
- block identity where available;
- section/chapter/TOC context;
- current locator;
- selection rectangles;
- selected text;
- link/image/footnote hits.

Web may show selection handles and context menus, but it must not reconstruct source ranges from DOM text. Search, annotations, knowledge, and AI retrieval consume canonical publication/source identifiers, not rendered HTML nodes.

## 42. Performance and Bundle Budgets

Create automated reports for every release build:

```text
raw WASM bytes
wasm-opt bytes
gzip bytes
Brotli bytes
JS/CSS bytes
dependency tree fingerprint
open-to-metadata
open-to-first-page
section parse
layout/pagination
display-list compilation
scene build
texture raster
frame CPU submit p50/p95/p99
navigation prepare latency
peak WASM memory
cache hits/misses/evictions
forbidden warm-frame counters
```

Initial budgets:

- EPUB-only core without GPU backend: Brotli under 6 MiB;
- complete EPUB reader with Vello/WebGPU: establish baseline, then require explicit approval for >10% regression;
- open-to-first-page for a small EPUB on reference desktop: under 500 ms warm-cache, under 1 s cold;
- input-to-first-motion: under 50 ms;
- warm frame CPU p95: under 8 ms;
- warm frame total p95: under 16.6 ms at 60 Hz;
- idle: zero continuous RAF;
- warm transition forbidden-work counters: all zero.

Do not optimize only for high-end GPUs. Run Chrome 4×/6× CPU throttling, DPR 3 phone viewport, and memory pressure/cache shrink scenarios.

## 43. Automated Test Architecture

### Rust unit/integration tests

- format feature matrix;
- EPUB parsing and resource limits;
- engine facade lifecycle;
- cooperative scheduler;
- navigation transactions;
- locator preservation;
- viewport reflow;
- frame revisions;
- cache/prefetch;
- transition state;
- hit testing and selection.

### WASM tests

Use `wasm-bindgen-test` for ABI/state tests that require a browser runtime. Include open, tick, navigate, resize, cancel, close, and structured error cases.

### Browser E2E tests

Use Playwright or the repository's chosen equivalent. Cover Chromium desktop and mobile emulation:

- load app over HTTPS;
- verify WebGPU capability/fallback message;
- open fixture EPUB;
- wait for first rendered page revision;
- screenshot compare;
- next/previous and TOC navigation;
- resize/orientation change;
- drag commit/cancel/fling;
- reload/locator restore;
- large EPUB loading/cancel;
- device-lost simulation where practical;
- ensure idle RAF stops.

### Golden comparisons

Generate native and web screenshots from the same fixture, viewport, style, locator, and font bundle. Compare page breaks and geometry separately from pixel antialiasing.

## 44. Independent Done Audit

After implementation, run a fresh audit from a clean target directory. The implementer must not merely state tests pass.

### Boundary audit

```bash
cargo tree -p rebook-engine --edges normal
cargo tree -p rebook-engine-wasm --target wasm32-unknown-unknown --edges normal
rg -n 'rebook_(reader|layout|renderer|publication)' crates/engine-wasm/Cargo.toml
rg -n 'page_text|text_region_text|try_turn_page|NavigationToken|PageDirection' apps/engine-web/src
```

Expected:

- `rebook-engine` has no platform UI/GPU dependencies;
- `engine-wasm` contains no duplicated reader policy;
- web source contains no pagination/navigation transaction logic;
- production web rendering does not use `page_text()` or DOM text reconstruction.

### Bundle audit

```bash
cargo tree -p rebook-engine-wasm --target wasm32-unknown-unknown -e normal
bun run build:wasm
bun run analyze:wasm
bun run build
```

Verify raw/gzip/Brotli sizes, disabled format absence, no stale dev artifact, and no unexpected >10% regression.

### Hot-path audit

Instrument counters and assert zero during an already-prepared Slide:

```text
section_parse
layout
display_compile
image_decode
scene_build
texture_create
target_recreate
```

Record at least 300 animation frames and report p50/p95/p99.

### Functional audit

Run the complete corpus on desktop Chrome and phone/tablet emulation. Confirm:

- real Vello-rendered content, not DOM preview;
- correct page breaks and spread pairing;
- images/tables/math/RTL/CJK;
- TOC and locator parity;
- selection/hit/source mapping;
- resize and rotation;
- touch drag commit/cancel;
- no blank page on large books;
- no main-thread long task caused by a polling loop;
- resources released after close/reopen.

### Native regression audit

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --no-deps
cargo fmt --all --check
```

Also run the native `engine-demo` window and offscreen render paths. Web work is not done if it breaks native reader behavior.

### Final evidence report

The final report must include:

1. exact commit list;
2. dependency graphs before/after;
3. bundle sizes before/after, raw/gzip/Brotli;
4. performance tables and reference hardware/browser;
5. screenshots for representative fixtures/viewports;
6. test command outputs;
7. known limitations;
8. confirmation that web uses real retained rendering;
9. confirmation that `apps/engine-web` owns only DOM/platform concerns;
10. confirmation that `engine-wasm` is a thin bridge.

## 45. Definition of Done — Web Engine v1

Web Engine v1 is complete only when all are true:

- EPUB bytes are opened by `rebook-engine` through the EPUB-only feature closure.
- A large image-heavy EPUB reaches first page without a blank screen or unbounded main-thread block.
- `PageDisplayList` is replayed through the shared Vello compositor into WebGPU.
- No production `<article>`/DOM text preview is used for page rendering.
- Canvas physical size tracks CSS size × DPR; layout uses the correct logical viewport.
- Resize and orientation changes preserve locator and reflow correctly.
- Previous/next, keyboard, TOC, and durable locator restore work.
- Pointer drag supports commit, cancel, and fling.
- None and Slide transitions work with source/destination pinning.
- Warm Slide moves cached textures only and passes forbidden-work assertions.
- Selection/hit testing returns source-backed context.
- Idle reader stops RAF.
- Bundle and performance reports meet the agreed budgets.
- Native workspace checks and native demo behavior remain healthy.
- An independent audit using Section 44 produces evidence for every item.

Anything less is a milestone or prototype, not a completed web reader.

## 46. Recommended Commit Sequence for AGY

Keep every commit buildable and reviewable:

1. `Restore web and native build baseline`
2. `Feature-gate ebook formats and add EPUB-only web profile`
3. `Trim engine-wasm to the engine facade boundary`
4. `Add engine cooperative work scheduler`
5. `Add prepared reader frame API`
6. `Promote shared Vello compositor after two-consumer verification`
7. `Render retained pages through WebGPU canvas`
8. `Make viewport and DPR reflow deterministic`
9. `Add navigation TOC and locator parity`
10. `Add pointer gesture and Slide transition`
11. `Add texture-only animation cache`
12. `Add hit testing selection and deep context`
13. `Add bundle performance and browser regression gates`
14. `Complete independent done audit`

Do not combine format gating, scheduler redesign, compositor extraction, and WebGPU rendering into one unreviewable commit.

# Part III — Android Platform Foundation

## 47. Architecture Boundary

Android must consume the same engine runtime as web and future native shells without importing desktop or browser APIs.

- `rebook-engine` is the single public product API and owns engine/book/reader lifetime, parsing, layout, logical viewport, normalized pointer events, navigation, selection and lifecycle-safe reader behavior.
- `rebook-android-host` uses the engine's full native format profile and is retained across Android surface recreation; web may keep an EPUB-only profile for bundle size.
- Each platform owns settings persistence, local library storage, content resolution, permissions, UI, clipboard and external intents.
- The Android render adapter owns the native surface and consumes `PreparedReaderFrame` through `rebook-vello-backend`.

## 48. Foundation Status

- [x] Separate logical layout size from physical render-target size with `ViewportMetrics`.
- [x] Migrate the web engine bridge to the shared viewport contract and correct DPR behavior.
- [x] Normalize pointer phase/type/coordinates in a platform-neutral `PointerEvent`.
- [x] Add foreground, suspension and surface-loss lifecycle contracts.
- [x] Add memory-pressure handling for reader and compositor caches.
- [x] Keep storage, settings and UI contracts outside engine; platforms use engine values such as locators and styles directly.
- [x] Add a full-format Android host crate with no desktop/web/service dependencies.
- [x] Add the Android ARM64 Rust target and verify the native host cross-compiles.
- [ ] Add the NDK toolchain and reproducible cargo-ndk build commands for a linked shared library.
- [ ] Add Gradle/Kotlin/JNI packaging.
- [ ] Add Android native wgpu/Vello surface creation and recovery.
- [ ] Add Storage Access Framework and Room-backed platform adapters.
- [ ] Add emulator/device lifecycle, touch, selection and memory-pressure tests.

## 49. Android Acceptance Gate

The first Android reader slice is complete only when it opens EPUB bytes from a content URI, renders a retained frame at the correct density, survives rotation/surface recreation without reopening the book, supports next/previous and interactive touch curl, restores a publication-keyed locator, and releases transient caches on Android low-memory callbacks.

## 50. Engine Binary Contract

Every platform-facing binary binding must wrap `EngineRuntime` as the single reader instance. Opening uses one `OpenReaderRequest` containing file bytes, viewport, style, locator, highlights and focus ranges so the engine paginates only once before first paint. Runtime bindings expose typed navigation, pointer, selection, style, search, TOC, locator, tick and frame operations. Platform code owns UI and durable storage and must not call lower engine implementation crates directly.
