# Torto Architecture Audit

This audit describes the checkout as inspected on 2026-09-08. Paths and symbol names below refer to the source in this repository; crate names are not treated as proof of architectural ownership.

## 1. Executive Summary

Torto already has a native, pagination-first reader pipeline: format adapters expose a lazy `BookSource`; `rebook-html` normalizes reflowable markup into the `rebook-publication` Reading IR; `LayoutEngine` shapes and paginates it into `PageLayout`; `DisplayListCompiler::compile` produces retained, source-aware `PageDisplayList` values; `ReaderSession` adds navigation, segmentation, caching, prefetch, spreads, locators, hit testing, and selection; desktop code converts retained pages into Vello scenes and renders them through wgpu into a texture displayed by egui.

The largest structural problem is physical, not conceptual: `crates/layout/src/lib.rs`, `crates/reader/src/lib.rs`, `crates/html/src/lib.rs`, and `crates/renderer/src/lib.rs` each contain several distinct subsystems, while `apps/desktop/src/reader/mod.rs` and `egui_view.rs` mix reusable reader presentation/compositing with egui, AI, persistence, and application policy. Inline tests make the headline sizes look worse, but layout and reader still contain roughly 4.1k and 4.3k production lines respectively.

Preserve the existing `publication`, `html`, `formats`, `layout`, `renderer`, and `reader` crates and their data flow. First perform mechanical internal module splits and isolate the Vello page-scene compositor from `DesktopReader`; do not redesign the IR or create many crates before those seams are visible.

## 2. Workspace Map

```text
apps/
  desktop/                    Native desktop application, egui UI, reader presentation,
                              Vello/wgpu integration, library, persistence, sync and AI features.
  inspect/                    Headless format/publication diagnostic CLI.
crates/
  publication/                Format-neutral book, semantic block, resource and locator contracts.
  html/                       HTML/CSS/XML-to-Reading-IR parser.
  formats/                    EPUB, MOBI/AZW/AZW3, FB2/FBZ, CBZ, CHM and PDF adapters.
  layout/                     Font discovery, shaping, line breaking, media layout and pagination.
  renderer/                   Backend-neutral retained page commands plus source hit/selection maps.
  reader/                     Stateful reader session, lazy section preparation, navigation and caches.
  math/                       LaTeX-to-SVG support used by layout and desktop chat.
  macos-open-file/            macOS open-document platform hook.
  windows-window-background/  Windows native window background helper.
third_party/
  egui*/                      Workspace-patched UI crates; excluded as workspace members.
```

The executable entry is `apps/desktop/src/main.rs`; window/event-loop ownership is in `apps/desktop/src/platform/application.rs`, GPU ownership is in `apps/desktop/src/platform/gpu.rs`, and top-level app routing is in `apps/desktop/src/app/mod.rs`. `apps/desktop/build.rs` only supplies Windows resources and is not part of the reader engine.

## 3. Cargo Dependency Graph

Internal normal dependencies, verified from manifests and `cargo metadata`, are:

```text
rebook-publication
  ↑             ↑                 ↑
rebook-html     rebook-math       platform helper crates (independent)
  ↑                ↑
rebook-formats     rebook-layout ─────────────┐
  ↑                ↑                         │
  │                └── rebook-renderer ──────┤
  │                              ↑            │
  └────────────── rebook-reader ─┘            │
                    ↑                         │
             rebook-desktop ──────────────────┘

rebook-inspect -> rebook-formats + rebook-publication
```

More exactly, `rebook-reader` depends on publication, layout, and renderer; renderer depends on layout and publication; layout depends on publication and math; formats depends on HTML and publication; HTML depends only on publication. The desktop binary directly depends on all engine crates because it also implements source wrappers, settings, diagnostics, and rendering bridges.

| Package | Role verified from source | Important third-party dependencies | Coupling profile |
|---|---|---|---|
| `rebook-publication` | Pure model/resource boundary | Serde, percent-encoding, thiserror | No UI/GPU/FS/OS/runtime/parser/typography |
| `rebook-html` | DOM/CSS normalization into IR | roxmltree | Parser only; no UI/GPU/FS/runtime |
| `rebook-formats` | File/byte import and lazy resources | zip, flate2, quick-xml, scraper, libchm, hayro, image | Filesystem in `open_file`; parser/codec heavy; PDF has raster cache |
| `rebook-math` | Formula SVG generation | ratex parser/layout/SVG | Renderer utility, otherwise platform independent |
| `rebook-layout` | Typography, layout and pagination | Parley, ICU4X segmenter, hyphenation, image, resvg, read-fonts | No UI/GPU/OS; resource access through `BookSource`; stateful font caches |
| `rebook-renderer` | Retained drawing and semantic geometry | anyrender, Parley, kurbo, peniko | No wgpu/Vello/egui/FS/OS/runtime; backend-neutral paint trait, but carries Peniko/Parley types |
| `rebook-reader` | Session/navigation/cache/prefetch | sentencex, unicode-segmentation | No UI/GPU/FS/OS or async runtime; uses `std::thread`, channels, mutex/condvar |
| `rebook-desktop` | UI/platform shell plus compositor and optional product features | egui, Vello, wgpu, winit, tokio, reqwest, rusqlite, keyring, rfd | UI/GPU/FS/OS/network/runtime coupled |
| `rebook-inspect` | Headless parser/IR validation | serde_json | CLI filesystem only; no renderer/layout/UI |
| platform helper crates | Native window integration | objc2 or windows-sys | OS-specific |

The intended typography stack is already present. `rebook-layout` calls Parley directly; `cargo tree -p rebook-layout` shows Fontique, HarfRust, ICU normalizer/properties/segmenter, and Skrifa beneath Parley. ICU4X line segmentation is also used directly in `crates/layout/src/linebreak/parley.rs`. There is no direct HarfRust API use in Torto source.

## 4. Runtime Reader Pipeline

The exact ordinary open-to-pixels path is:

1. The shelf initiates `reader::open_reader` from `apps/desktop/src/shelf/mod.rs`; `apps/desktop/src/reader/mod.rs::open_reader` calls `rebook_formats::open_file_for_reading`, obtains `OpenedPublication::{source,book,cover_bytes}`, applies desktop source wrappers, and constructs `ReaderSession::open_with_fonts_at_locator` (or the corresponding open path).
2. `rebook-formats::open_file_with_options` in `crates/formats/src/lib.rs` detects `BookFormat`, reads local bytes (CHM has a path-specialized route), and returns an `Arc<dyn BookSource>`. The `Book` descriptor is eager; section parsing and most resources remain lazy.
3. `ReaderSession::new_unpositioned` in `crates/reader/src/lib.rs` builds TOC indexes, hidden-section policy, `SectionRepository`, `LayoutEngine`, `DisplayListCompiler`, caches and `PrefetchWorker`. Opening/restoring then ensures the starting segment.
4. `SectionRepository::load` calls `BookSource::parse_section(index)`. EPUB and direct converted formats eventually call `rebook_html::parse_section`/`parse_section_with_hints_and_image_classifier`; PDF returns a fixed-page `Section` with an `ImageBlock` and text layer.
5. `prepare_section` turns a parsed `Section` into private `PreparedSection { fragments, segments, anchor_segments, reading_units }`. It uses `fragment_section_blocks`, `resolve_fragment_anchors`, `build_reading_units`, and `build_layout_segments`; this bounds pagination work without creating fake authored sections.
6. `ReaderSession::ensure_segment` or the `PrefetchWorker` calls private `compile_segment`. The input is a `PreparedSection`, `SegmentKey`, viewport/style, `LayoutEngine`, and `DisplayListCompiler`; the output is `CachedSegment` holding shared retained pages and anchor/page metadata.
7. `LayoutEngine::layout_fragments` in `crates/layout/src/lib.rs` resolves page/spread geometry, flattens visible note policy, loads/rasterizes media through `BookSource`, shapes blocks, and drives `Paginator`. `Paginator::finish` returns `SectionLayout { pages: Vec<PageLayout>, visible_pages, continuation_offset_x }`.
8. `compile_segment` maps every `PageLayout` through `DisplayListCompiler::compile`, yielding `Arc<PageDisplayList>` values and an authored-fragment-to-page index. `ReaderSession` owns these in its segment `HashMap`; its explicit `VecDeque` is the LRU order.
9. `ReaderSession::current_spread` returns `ReaderSpread { primary, secondary, primary_offset_x, secondary_offset_x }`. The secondary page can cross a segment or section boundary. `current_section_pages`/`current_reading_unit_pages` expose larger page sets for continuous/focus presentation.
10. `DesktopReader::page_scene` in `apps/desktop/src/reader/render/scene.rs` obtains the spread, replays page background/images and non-image content into cached `PageSceneLayers`, injects current highlight/selection/focus overlays, and returns `ReaderScene`. `PageDisplayList` replay targets `VelloScene`, the local `anyrender::PaintScene` adapter in `render/vello.rs`.
11. `DesktopReader::ui` in `reader/egui_view.rs` returns `ReaderFramePlan { rect, scene_id, scene_revision, background }` and asks egui to show the last/current registered page texture.
12. `GpuState::render` in `platform/gpu.rs` runs the egui pass to get that plan, calls `ensure_page_target`, asks `DesktopApp::reader_scene`, and invokes `GpuState::render_reader_scene` when `(scene_id, scene_revision)` is stale. That method scales the Vello `Scene`, refreshes referenced atlas images when required, and calls `vello::Renderer::render_to_texture` into a wgpu storage/texture-binding target.
13. The target is registered with `egui_wgpu::Renderer::register_native_texture`; egui paints it into the wgpu surface render pass, `queue.submit` executes both Vello and egui work, and `SurfaceTexture::present` displays it.

The pipeline is platform independent through `PageDisplayList`. Vello scene construction is mostly portable backend work but is physically and type-coupled to `DesktopReader`; target creation, surface acquisition, texture registration, and presentation are desktop shell work.

## 5. Canonical Reading IR

`crates/publication/src/lib.rs` is the canonical model today:

- `Book` owns `PublicationId`, `Metadata`, optional cover `PublicationUrl`, ordered `Vec<SpineItem>`, and hierarchical `Vec<TocEntry>`.
- `Metadata` preserves title, ordered authors, BCP-47 language tags, and `RenditionLayout`; `Metadata::writing_system` derives a coarse `WritingSystem` hint without mutating metadata.
- `SpineItem` preserves `SpineItemId`, canonical href, media type, linearity and properties. `NOTE_SECTION_PROPERTY` adds a normalized whole-section semantic hint.
- `Section` owns its spine identity/href, source-ordered blocks, and `SectionAnchor` mappings from authored fragments to `SourceAnchor`.
- `Block` variants are `Text`, `Quote`, `Table`, `Image`, `Figure`, `Note`, `Separator`, `LineBreak`, and `PageBreak`. `TextBlockKind` retains paragraphs, heading levels/ordinals, blockquotes/attributions, preformatted text, captions, footnote definitions, nested list metadata, and definition-list roles.
- `Inline` variants are styled `Text(TextRun)`, semantic `Math(MathRun)`, inline `Image`, and forced `Break`. `TextRun` retains normalized text, a portable `TextStyle`, and resolved link; text style includes font family/scale/weight/style/decoration/color/language/hyphenation/role/baseline fields. Block and image styles retain a deliberate portable subset, not arbitrary CSS.
- `TableBlock` retains rows, cells, spans, header state/alignment, and source-backed cell text. `QuoteBlock`, `FigureBlock`, `NoteBlock`, and `SeparatorBlock` retain higher-level grouping useful beyond rendering.
- Fixed layout is represented by an image plus `FixedPageTextLayer { width, height, text, spans, replacement }`; spans map Unicode scalar ranges to `FixedPageTextRect`, and replacement segments support translated overlays.
- `PublicationUrl` canonicalizes internal decoded `/` paths, strips queries for lookup, rejects external schemes/root escapes, and keeps an optional decoded fragment. It deliberately cannot represent filesystem or network URLs.
- `SourceAnchor { spine, node, text_offset }` uses a deterministic parser-assigned node string and Unicode scalar offset. `SourceRange` is half-open. `LocatorV1` combines publication/href, section and total progression, optional position, precise range, partial CFI, and `TextQuote` recovery context.

This is strong enough to remain the canonical Reading IR: it is serializable, renderer-independent, semantically richer than flat text, lazily accessible via `BookSource`, and source ranges are already consumed by navigation, selection, highlights and desktop search/AI source wrappers. Search/indexing, annotations, knowledge indexing, and AI retrieval can traverse `BookSource::book` plus `parse_section` without layout or renderer state. The current desktop plugins do not belong in this IR path.

Concrete gaps, without redesigning it:

- Parser node IDs are deterministic within the parser output but are synthetic strings allocated by `ReadingIrParser::allocate_node`; there is no preserved DOM path, raw byte range, or standardized CFI generation (`LocatorV1::partial_cfi` exists but this audit found no engine producer).
- Source offsets are attached at block/text-run aggregation boundaries. A `TextRun` itself has no source range; `TextBlock.source` is the durable mapping unit. Styling/link runs survive, but exact source positions inside transformed/collapsed HTML are recovered by proportional text/source offset mapping later, not a per-character provenance table.
- CSS is intentionally lossy: `StyleSheet` maps supported selectors/properties into `TextStyle`, `BlockStyle`, and `ImageStyle`; unsupported DOM semantics, arbitrary attributes, cascade information, and markup structure are discarded.
- `SourceAnchor` reaches spine → parser node → scalar offset, but it has no explicit block ID, section/chapter ID, or immutable semantic-parent chain. Chapter is inferred through TOC/section indexes, and block identity is currently a range comparison.
- Fixed-page extracted text has geometric spans but not font/style, semantic block hierarchy, or reliable logical structure beyond extractor order.
- The `Book` model has limited bibliographic metadata and no explicit landmark/page-list model beyond TOC origin/properties.

Current mapping is: source element → `ReadingIrParser::source_range` → `TextBlock`/image/table/quote ranges → `TextPlacement.source` and retained text → `ShapedTextRegion` → `PageTextHit`/`PageSelectionFragment` → `ReaderTextHit`/`ReaderSelectionRect`. `PageDisplayList::{text_region_source_range,text_region_byte_range_for_source,source_rects}` provides both directions between retained text/page geometry and durable ranges. The chain supports page → shaped cluster/text → source range → parser node/spine, but block/section/chapter context must still be resolved by scanning IR and TOC rather than returned as one deep-context object.

## 6. Formats and Parsing

`BookFormat` supports EPUB, MOBI, AZW, AZW3, FB2, FBZ, CBZ, CHM, and PDF (`crates/formats/src/lib.rs`). `open_file`, `open_file_for_reading`, and `open_bytes` normalize them behind `OpenedPublication` and `Arc<dyn BookSource>`.

- EPUB: `EpubPublication::open_bytes` in `formats/src/epub.rs` validates and reads the ZIP/container/package, builds metadata, manifest, spine, cover, and EPUB nav/NCX TOC. `BookSource::parse_section` decodes XHTML, supplies CSS/image hints, then calls `rebook_html` lazily. Resources remain archive-backed.
- MOBI/AZW/AZW3: `mobi::open` chooses KF8 or MOBI6 using `kf8::{is_kf8,parse,parse_mobi6}`. KF8 reconstructs skeletons/fragments, remaps embedded resources and TOC targets; MOBI6 normalizes legacy HTML/file-position anchors. Both become `DirectBookSource`, whose lazy `parse_section` routes stored HTML through `rebook-html`.
- FB2/FBZ: `fb2::open` decodes XML or zipped XML, extracts metadata/binary images/sections, renders normalized HTML fragments, then uses `DirectBookSource` and the common HTML parser.
- CBZ: `cbz::open` orders image archive entries and creates a pre-paginated book of image sections through `DirectBookSource`.
- CHM: `chm::{open_path,open_bytes}` reads the CHM directory/resources and TOC and implements `BookSource` directly; its `parse_section` loads a content document and invokes `rebook_html::parse_section`.
- PDF: `pdf::{open,open_with_id,open_shared}` uses Hayro, creates one pre-paginated spine item per page, extracts catalog outlines through `pdf/catalog.rs`, and implements `BookSource` directly. `PdfPublication::parse_section` provides a page image block plus `FixedPageTextLayer`; rasterization and page resources are lazy and protected by `PdfResourceCache`/LRU.

`rebook-html` is itself a mixed parser subsystem. `ReadingIrParser` handles structural block recovery, figures, notes, quotes, lists/tables, anchors and inline collection; `InlineCollector` normalizes whitespace and merges runs; `StyleSheet` implements the supported CSS cascade; helper families classify note links, captions, quotes, separators and navigation suppression. `parse_section_with_hints_and_image_classifier` is the full entry point. Production ends around line 3703; the remaining ~2.45k lines are dense parser tests.

## 7. Layout and Typography

`crates/layout/src/lib.rs` contains these conceptual subsystems:

- Configuration: `LayoutViewport`, `ReaderStyle`, `ReaderTypesetting`, `ReaderTypography`, `ReaderFontChoice`, `ReaderDefaultFont`, `TypesettingMode`, `LineBreakStrategy`, `ParagraphIndentMode`, and `SpreadMode`.
- Font management: `LayoutEngine::{new,with_fonts,available_font_families,available_reader_font_families}`, `ReaderFontFamilies::repair_typography`, Fontique collection access through Parley, OpenType inspection through `read-fonts`, and resvg font configuration.
- Semantic style resolution: `resolve_text_block`, `resolve_semantic_inline_presentation`, `semantic_script_spans`, writing-system defaults, list/heading/quote rules and authored-vs-unified typesetting policy.
- Shaping: `shape_text*`, `build_text_layout`, `prepare_inline_content`, Parley `Layout<TextBrush>`, inline boxes and synthetic list/footnote content. Parley supplies Fontique/HarfRust/ICU integration indirectly.
- Line breaking: `linebreak/knuth_plass.rs` is the independent optimizer; `linebreak/parley.rs` adapts shaped clusters, maps ICU4X UAX #14 boundaries, mixed-script spacing and justification; `linebreak/hyphenation.rs` supplies cached en-US/en-GB dictionary breaks. Unsupported/RTL cases fall back to Parley wrapping rather than the custom optimized path.
- Tables/media/math: `shape_table`, adaptive column fitting and safe row breaks; `load_raster_image`, inline raster preparation, SVG via resvg, and formulas via `rebook-math` rasterization.
- Fixed layout: `layout_fixed_page_placeholder`, page image sizing, fixed text-layer replacement shaping and placement.
- Pagination: `resolve_page_geometry`, `PageGeometry`, `Paginator`, quote continuation, keep-together policy, columns/spreads, page commits, and final `SectionLayout` generation.

`LayoutEngine::layout_fragments` is the orchestration seam. It takes publication-backed block slices, viewport and style; it emits renderer-independent `SectionLayout`. `PageLayout` contains viewport/background/leading gap and `PageItem`s. Text items retain an `Arc<parley::Layout<TextBrush>>`, complete shaped UTF-8 text, synthetic-prefix length, visible line range, position, width, source range and inline rasters. That is deliberately more than geometry: it preserves shaping state so the retained renderer can draw and map hits without reshaping.

Spread geometry is chosen in layout: `resolve_page_geometry` sets `visible_pages` and `continuation_offset_x`, and `Paginator` may paginate columns accordingly. Fixed pages use exact aspect geometry and can first be represented by a one-pixel placeholder. This supports lazy continuous PDF materialization through reader APIs.

## 8. Retained Renderer

`DisplayListCompiler::compile` in `crates/renderer/src/lib.rs` converts one immutable `PageLayout` into `PageDisplayList`. It creates private retained `DisplayCommand` variants (`Glyphs`, `Image`, `FillRect`, `FillRoundedRect`, `Rule`) and parallel semantic regions: shaped/fixed `TextRegion`, inline content, table, quote, and footnote regions.

Responsibilities are cleanly distinguishable even though they share one file:

- Compilation: `DisplayListCompiler::compile`, `compile_text_commands`, `compile_table_commands`, `text_region`, and `fixed_text_region` translate positioned layout into commands and indexes.
- Retained page data/replay: `PageDisplayList::{paint,paint_scaled,paint_scaled_at,paint_background,paint_content_at,paint_images_at,paint_non_image_content_at}` replay through `anyrender::PaintScene`; no Vello/wgpu/egui types occur here.
- Semantic maps: `hit_test_text`, `selection_fragment`, source/text byte conversion, `leading_source_range`, `source_range_nearest_y`, source/table/quote/image bounds, footnote lookup, and source-range painting.
- Backend drawing: only the `PaintScene` trait boundary and portable Kurbo/Peniko/anyrender paint data live here. The Vello implementation is `apps/desktop/src/reader/render/vello.rs::VelloScene`.

`PageDisplayList` is sufficiently backend-independent to remain the retained page representation for desktop and Android. Its coupling is to Parley layout/font blobs and Peniko/anyrender paint vocabulary, not a window system or GPU API. That is acceptable for a shared Vello-oriented engine. A truly unrelated backend would need to implement `PaintScene` and understand the same glyph/image data; the private `DisplayCommand` prevents external backends from inspecting commands directly, but replay APIs are adequate for drawing. Android-specific wgpu surface ownership should sit after this boundary.

One architectural caveat is that semantic indexes are stored beside paint commands. This is useful and avoids duplicate page structures, but search/index/AI must continue to use publication IR rather than requiring `PageDisplayList`; its semantic maps are for viewport interactions.

## 9. Reader Session

`rebook-reader` is platform independent but internally mixed. It imports no egui, Vello, wgpu, filesystem, or OS APIs. Its only concurrency mechanism is standard-library threads/channels/synchronization.

Subsystem classification:

| Subsystem and exact symbols | Current responsibility | Classification |
|---|---|---|
| Session state: `ReaderSession`, `ReaderSnapshot`, `ReaderLocation`, `ReaderPosition` | Own source/layout/compiler, active position, TOC indexes and cache generation | CORE BUT SPLIT INTERNALLY |
| Section preparation: `SectionRepository`, `SectionSlotState`, `PreparedSection`, `ContentFragment`, `LayoutSegment`, `prepare_section`, `fragment_section_blocks`, `build_layout_segments` | Lazy parse and bounded compilation units | CORE |
| Pagination compile/cache: `SegmentKey`, `CachedSegment`, `compile_segment`, `ensure_segment`, `touch`, `evict` | Layout-to-display-list orchestration and LRU | CORE |
| Prefetch: `PrefetchWorker`, request/result/key, queue/poll/wait/install methods | Background section load/layout/compile with generations | CORE policy/caching |
| Navigation: `turn_page`, `try_turn_page`, `go_to_*`, `try_go_to_*`, next/previous position methods, `NavigationAttempt` | Blocking and non-blocking movement, cross-segment/section spreads | CORE |
| Spread/page access: `ReaderSpread`, `ReaderSectionPage`, `current_spread`, `current_section_pages`, `current_reading_unit_pages`, `resolve_spread_offsets` | Presentation-neutral page assembly | CORE |
| Locators: `current_locator`, `restore_locator`, `position_for_source_anchor`, href/anchor methods | Durable location ↔ pagination mapping | CORE |
| Selection/hits: `ReaderTextHit`, `ReaderSelection`, `ReaderSelectionRect`, `ReaderImage`, hit/source/image methods and selection helpers | Converts retained page interaction into durable ranges | CORE |
| Reading units/TOC: `ReadingUnit`, `FixedReadingUnit`, `ReadingUnitLocation`, `TocIndex`, `TocViewItem`, flatten/active helpers | Semantic navigation and presentation-ready TOC flattening | CORE, with some policy |
| Segmentation: `sentence_byte_ranges*`, `sentence_char_ranges`, semantic selection helpers | Sentence/word/paragraph expansion | CORE semantic utility |

`SectionRepository` prevents duplicate parses with a mutex/condvar and stores weak prepared-section references; cached segments retain the active prepared section. `PrefetchWorker::spawn` owns a separate `LayoutEngine` and `DisplayListCompiler`, receives generation-tagged requests, and compiles off the caller thread. `try_ensure_navigation_segment` cancels stale speculative generations so direct navigation is prioritized. `NavigationAttempt::Pending` is therefore a real non-blocking contract, not a UI fiction.

The segment cache is a bounded `HashMap<SegmentKey, Arc<CachedSegment>>` plus `VecDeque` LRU. `invalidate_layout` clears compiled segments on viewport/style changes while preserving fractional progress; source refresh rebuilds source/repository/TOC state. Fixed pages have placeholder/materialization APIs (`ReaderSectionPage::placeholder`, `try_materialize_position`) so continuous PDF does not rasterize every page eagerly.

The main mixing is policy density: selection semantics, sentence segmentation, TOC flattening, note hiding, reading units, cache mechanics, navigation, locators and worker lifecycle all sit in `lib.rs`. None is desktop-specific, but they should become internal modules.

## 10. Desktop Reader Architecture

`DesktopReader` in `apps/desktop/src/reader/mod.rs` is not one architectural layer. Its fields simultaneously own `ReaderSession`, publication wrappers, snapshot/navigation state, highlights and progress stores, selections/images, reading/focus/scroll modes, search/chat/translation/PDF OCR/TOC features, egui textures and panels, Vello scene caches, gestures, timers and reopen/exit state.

Conceptual separation from source evidence:

- Reusable compositor/backend: `reader/render/scene.rs` (`PageSceneKey`, `PageSceneLayers`, `ReaderScene`, scene LRU, `page_scene`, `page_scene_layers`, `scroll_page_scene`, overlay painting, scene invalidation) and `reader/render/vello.rs` (`VelloScene` implementing `anyrender::PaintScene`). These depend on Vello/Peniko and retained page APIs. The basic spread replay, layering, transforms, image list and cache policy are candidates for Android reuse.
- Desktop GPU surface: `platform/gpu.rs::GpuState`, `PageTarget`, surface acquisition/configuration, wgpu texture allocation, egui texture registration, `render_reader_scene`, surface render pass and presentation. The Vello `render_to_texture` core is reusable in concept, but current ownership and return type are egui/winit-specific.
- egui shell/presentation: `reader/egui_view.rs`, `settings/egui_view.rs`, shelf/sidebar/toolbars/dialogs, `ReaderFramePlan`/`ReaderPageTexture`, and app routing. These should remain desktop shell.
- Desktop input/navigation bridge: `reader/interaction.rs`, `reader/navigation.rs`, and parts of `egui_view.rs`; these translate egui keyboard/pointer/wheel/long-press input into `ReaderSession` commands and retry `Pending` requests. Gesture policy is not reusable unchanged on Android, although command semantics are.
- Application features: `assistant.rs`, chat modules, `plugins/*`, generated TOC/metadata, translation/OCR wrappers, highlights/persistence/sync/statistics. These are optional product services and must stay outside the hot render path.

`reader/mod.rs` is misleadingly measured at 4,899 LOC: the first module-level `#[cfg(test)]` begins at 3,342, so production is about 3,341 LOC, not 687; an earlier function-local `#[cfg(test)]` made a naive first-marker count wrong. It remains a god file because its production portion combines all categories above. `egui_view.rs` is ~5,485 production LOC plus ~766 tests and is a separate UI god file.

Code reusable on Android with little/no algorithm change is `VelloScene`, the retained-page-to-scene layer assembly, page scene key/LRU, spread transforms, overlay replay, and image-atlas reference collection. Before reuse, those operations must accept reader pages/overlay descriptors as inputs rather than methods that inspect the enormous `DesktopReader` state.

## 11. GPU / Vello / wgpu Path

The Vello/wgpu path is exact and two-stage:

```text
PageDisplayList::paint_* --anyrender::PaintScene--> VelloScene --mutates--> vello::Scene
    -> ReaderScene/PageSceneLayers cache
    -> GpuState::render_reader_scene
    -> vello::Renderer::render_to_texture
    -> wgpu PageTarget texture/view
    -> egui-wgpu registered TextureId
    -> egui surface pass
    -> queue.submit + SurfaceTexture::present
```

`PageSceneLayers` separates underlay (background/images) from non-image content so dynamic source overlays can be inserted between/around them without recompiling the static page. `ReaderScene` keeps `Arc<Scene>`, referenced `ImageData`, and `refresh_image_atlas`; `GpuState::render_reader_scene` explicitly dirties images to work around persistent-atlas omission before rendering.

Invalidation is explicit. `DesktopReader` has a stable `scene_id` and wrapping `scene_revision`; `bump_scene_revision`, `invalidate_page_scene(s)`, and `apply_snapshot` control changes. `GpuState::reader_scene_needs_render` compares `(scene_id, scene_revision)` against the target. Target recreation is separated from scene invalidation and requests another redraw so an old egui texture is not stretched during resize.

This is already a compositor boundary in behavior, but not in ownership: scene construction reaches directly into focus/highlight/selection/scroll state and is implemented on `DesktopReader`. The surface path is correctly desktop-specific.

## 12. Inspect / Diagnostic Tooling

`apps/inspect/src/main.rs` is a 78-line headless CLI. `inspect` calls `rebook_formats::open_file`, serializes format, publication ID, metadata, cover, reading order and TOC, and eagerly invokes `BookSource::parse_section` for every section to report block and anchor counts. It validates that a supported file can cross the format/publication boundary and that every section parses; it does not validate layout, pagination, resources, source-range consistency, retained rendering, or pixel output.

It should remain a permanent DEV TOOL because it provides a renderer-free smoke test and machine-readable IR summary. It should not be counted in runtime kernel size. Future diagnostics could deepen validation, but that is outside this audit.

## 13. Testing Structure

Tests are overwhelmingly inline `#[cfg(test)] mod tests` blocks rather than `tests/` integration suites. No `.snap` files or dedicated snapshot/golden directories were found. Fixtures are predominantly constructed in test code; layout tests embed repository font assets, and format tests synthesize ZIP/PDF/MOBI data.

- Publication tests cover URL normalization/security, locator validation and writing-system inference.
- HTML has ~60 tests covering CSS normalization, whitespace, notes, anchors, figures/captions, tables/lists, quotes, math/images and navigation suppression.
- Formats test detection, EPUB archive safety/navigation/encoding, MOBI/KF8 decompression/reconstruction, FB2/CBZ/CHM conversion, PDF outlines/text/rasterization and direct-source heading promotion.
- Layout has ~82 tests around font repair, script styling, Parley shaping, optimized/Knuth-Plass breaks, hyphenation, tables, images/math, fixed pages, spread geometry, pagination and quote continuation.
- Renderer has ~18 tests for compilation, glyph/image commands, hit testing, selection/source mapping, fixed text, tables/quotes/footnotes and replay behavior.
- Reader has ~52 tests for locators, navigation, cross-section spreads, selection granularity, source mapping, reading units, cache eviction, prefetch generations/non-blocking behavior and fixed-page placeholders.
- Desktop reader tests cover focus/continuous presentation, navigation effects, selection/input, scene caching/atlas invalidation and GPU revision checks. GPU tests do not constitute end-to-end rendered-image goldens.

Approximate production/test LOC uses the module-level test boundary when present. Files with multiple conditional blocks were manually corrected; numbers are for architectural scale, not a lexical code metric.

## 14. Large / Mixed Files

| path | total LOC | prod LOC | test LOC | responsibilities | severity |
|---|---:|---:|---:|---|---|
| `crates/layout/src/lib.rs` | 8,189 | ~4,148 | ~4,041 | style/fonts, shaping, script policy, tables/media/math, fixed layout, pagination | GOD FILE |
| `crates/reader/src/lib.rs` | 6,759 | ~4,293 | ~2,466 | session, repository, segments, navigation, cache/prefetch, spreads, locators, selection, TOC/units | GOD FILE |
| `crates/html/src/lib.rs` | 6,156 | ~3,703 | ~2,453 | structural HTML parser, inline collector, CSS cascade, semantic recovery | GOD FILE |
| `apps/desktop/src/reader/egui_view.rs` | 6,251 | ~5,485 | ~766 | entire reader UI, panels, shortcuts, gestures, assistant/search/TOC, texture presentation | GOD FILE |
| `apps/desktop/src/reader/mod.rs` | 4,899 | ~3,341 | ~1,558 | desktop reader aggregate, source wrappers, focus/scroll state, optional product features | GOD FILE |
| `apps/desktop/src/plugins/pdf_ocr.rs` | 4,089 | ~2,121 | ~1,968 | PDF OCR workflow/source overlay/tests; optional, outside kernel | SHOULD SPLIT |
| `apps/desktop/src/plugins/ai.rs` | 3,698 | ~2,506 | ~1,192 | LLM provider/request orchestration and tests; optional | SHOULD SPLIT |
| `crates/renderer/src/lib.rs` | 3,156 | ~2,090 | ~1,066 | compilation, retained commands, hit maps, selections, replay | SHOULD SPLIT |
| `apps/desktop/src/plugins/translation.rs` | 3,001 | ~1,800 | ~1,201 | translation workflow/source overlays; optional | SHOULD SPLIT |
| `apps/desktop/src/settings/egui_view.rs` | 2,725 | ~2,494 | ~231 | settings UI | SHOULD SPLIT |
| `apps/desktop/src/reader/chat_markdown.rs` | 2,614 | ~1,725 | ~889 | chat Markdown layout/render/cache | SHOULD SPLIT |
| `apps/desktop/src/reader/assistant.rs` | 2,405 | ~2,209 | ~196 | reader assistant policy/state | SHOULD SPLIT |
| `crates/formats/src/epub.rs` | 2,117 | ~1,491 | ~626 | archive safety, package/nav parsing, lazy source | SHOULD SPLIT |
| `crates/formats/src/kf8.rs` | 1,808 | ~1,774 | ~34 | PalmDB/KF8/MOBI6 parsing, decompression, resources/TOC | SHOULD SPLIT |
| `crates/publication/src/lib.rs` | 1,406 | ~1,317 | ~89 | coherent public model and contracts | OK |
| `apps/desktop/src/reader/render/scene.rs` | 501 | ~437 | ~64 | Vello composition/layers/cache/overlays | OK |
| `apps/desktop/src/platform/gpu.rs` | 503 | ~483 | ~20 | wgpu/Vello target and desktop surface | OK |
| `apps/desktop/src/reader/render/vello.rs` | 155 | 155 | 0 | anyrender-to-Vello adapter | OK |
| `apps/inspect/src/main.rs` | 78 | 78 | 0 | headless inspection CLI | OK |

Other desktop 2k+ files are product/UI concerns and were inventoried but are not reader-kernel boundaries. Third-party patched egui sources are vendored dependencies, not Torto architecture, and are excluded from this table.

## 15. Current Architectural Boundaries

The real boundaries today are:

1. `BookSource` is the import/resource boundary. Formats own decoding and lazy bytes; consumers see `Book`, `Section`, `Resource`, raster resource and fixed dimensions.
2. Publication IR is the semantic boundary. It is independent of layout, rendering and applications.
3. `PageLayout` is the pagination/shaping output boundary, but it intentionally contains Parley layouts and decoded rasters rather than being a pure serializable geometry model.
4. `PageDisplayList` is the retained page and viewport-semantic boundary. It exposes replay and semantic queries without GPU/UI types.
5. `ReaderSession` is the reusable behavioral kernel boundary. It owns pagination generations and returns retained pages/spreads plus durable semantic results.
6. `VelloScene`/`ReaderScene` is an implicit reusable compositor boundary currently embedded under desktop.
7. `GpuState` plus application/event-loop code is the actual desktop platform boundary.
8. AI, search UI, translation, OCR, sync and persistence are desktop services/source wrappers, not part of the render kernel, although some currently feed overlays back through `DesktopReader`.

Crate boundaries mostly align with these stages; the exceptions are internal cohesion and the misplaced compositor. There is no evidence supporting a rewrite or replacement of the six core crates.

## 16. Target Reader Kernel Boundary

The smallest sensible target retains the existing pipeline:

```text
formats -> html -> publication
                       ├── search/index/annotation/knowledge/AI retrieval (cold consumers)
                       v
                    layout -> PageLayout -> renderer -> PageDisplayList
                                                       v
                                                    reader
                                          navigation/cache/prefetch/spreads
                                                       v
                                             Vello compositor boundary
                                                       v
                                              platform wgpu surface
```

| Component | Classification | Reason |
|---|---|---|
| `rebook-publication` | CORE | Canonical semantic/source model |
| `rebook-html` | CORE BUT SPLIT INTERNALLY | Shared reflow parser; large physical module |
| `rebook-formats` | CORE BUT SPLIT INTERNALLY | Required import adapters; format-specific internals remain isolated |
| `rebook-layout` | CORE BUT SPLIT INTERNALLY | Pagination and typography kernel |
| `rebook-renderer` | CORE BUT SPLIT INTERNALLY | Retained backend-neutral page representation |
| `rebook-reader` | CORE BUT SPLIT INTERNALLY | Platform-independent reader/session kernel |
| Vello bridge and page compositor now under desktop | CORE candidate after dependency inversion | Android can reuse retained-page scene assembly |
| wgpu target/surface adapters | Platform shell | Separate desktop and future Android ownership |
| `apps/inspect` | DEV TOOL | Headless parser/IR validation |
| `rebook-math` | OPTIONAL core support | Needed for formula-bearing books; also used by chat |
| macOS/Windows helper crates | DESKTOP SHELL | Native desktop integration only |
| egui views, shelf, settings | DESKTOP SHELL | Desktop application/UI |
| AI/chat/search UI/translation/OCR/generated metadata/TOC | OPTIONAL | Keep outside hot rendering path and kernel baseline |
| sync, updater, statistics, WebDAV, keyring | REMOVE FROM MINIMAL APP | Product services unrelated to minimal reading |

A new compositor crate is not yet justified. First make scene composition a self-contained desktop module with explicit inputs. Extract a crate only when Android integration demonstrates a second consumer and its dependency surface is known.

## 17. Mechanical Modularization Plan

These are file moves and visibility adjustments, not behavior/API changes.

For `rebook-reader`, keep public re-exports in `lib.rs` and split:

- `position.rs`: `ReaderLocation`, `ReaderPosition`, `ReaderSnapshot`, `ReaderVisibleTextFragment`, progression helpers.
- `navigation.rs`: `PageDirection`, navigation outcomes/attempts/results, `turn_page`, `try_turn_page`, go-to/next/previous/position resolution methods.
- `section.rs`: `SectionRepository`, slots/state, `PreparedSection`, `ContentFragment`, `LayoutSegment`, `prepare_section`, fragment/anchor/segment builders.
- `cache.rs`: `SegmentKey`, `CachedSegment`, `compile_segment`, ensure/touch/evict/invalidation methods.
- `prefetch.rs`: worker/request/result/key, generation queue/poll/wait/install logic.
- `spread.rs`: `ReaderSpread`, `ReaderSectionPage`, current spread/page-set methods and `resolve_spread_offsets`.
- `locator.rs`: current/restore locator, href/source-anchor mapping and range containment helpers.
- `selection.rs`: hit/image/selection public types, page hit conversion, source geometry and selection construction/granularity helpers.
- `reading_units.rs`: `ReadingUnit`, `FixedReadingUnit`, location and unit building/navigation.
- `toc.rs`: `TocIndex`, `TocViewItem`, flattening/hidden-section/active-item helpers.
- `segmentation.rs`: sentence range functions and language/terminal helpers.
- `session.rs`: `ReaderSession` fields, constructors, style/resize/source-refresh orchestration and delegating public methods.

For `rebook-layout`:

- `style.rs`: reader style/typesetting/typography enums and normalization.
- `fonts.rs`: font blobs/families, discovery/classification/repair and optical-size helpers.
- `model.rs`: `SectionLayout`, `PageLayout`, `PageItem` and all placement/raster types.
- `engine.rs`: `LayoutEngine` constructors and `layout_section`/`layout_blocks`/`layout_fragments` orchestration.
- `text.rs`: `TextBrush`, prepared/range types, semantic style resolution and shaping/build-layout helpers.
- `tables.rs`: prepared table/cell metrics, shaping, adaptive widths and safe breaks.
- `media.rs`: image loading/sizing, inline images, SVG/math rasterization and figure layout.
- `fixed.rs`: placeholders and fixed-page replacement requests/shaping.
- `pagination.rs`: geometry helpers, `Paginator`, quote state and page commits.
- retain existing `linebreak/{mod,knuth_plass,parley,hyphenation}.rs`.

For `rebook-renderer`:

- `display_list.rs`: `PageDisplayList`, retained metadata and replay entry points.
- `commands.rs`: private command structs/enum and painting.
- `compiler.rs`: `DisplayListCompiler`, compile helpers and item bounds.
- `text.rs`: shaped/fixed `TextRegion`, hit tests and byte/source conversion.
- `selection.rs`: public hit/fragment types, rectangle/path and source painting helpers.
- `regions.rs`: inline/table/quote/footnote/image semantic regions and lookup.

Tests should move into each module's local `tests` module so behavior remains near its implementation. Preserve root re-exports to avoid downstream API churn. HTML deserves the same mechanical treatment (`parser`, `inline`, `css`, `notes`, `figures`, `quotes`, `tables_lists`, `anchors`), although the requested first extraction work should prioritize reader/layout/renderer.

For desktop, split `DesktopReader` behavior before moving ownership: keep the aggregate initially, but move scene composition to a `ReaderCompositor` that receives `ReaderSpread`/page sets plus a compact overlay descriptor. Keep egui texture handles, gestures, panels and app features out of that type. This makes the reuse evidence testable without prematurely adding a crate.

## 18. Page Transition Readiness

The correct insertion point is between destination-page readiness in `ReaderSession` and final scene submission in `GpuState`, centered on desktop `ReaderScene` composition. `PageDisplayList` already supports arbitrary replay offsets (`paint_scaled_at`, `paint_*_at`), and Vello `Scene::append` accepts `Affine`; no layout or renderer change is required for None/Slide/Cover transforms.

- None: already implemented as the immediate replacement path through `try_turn_page` → `apply_snapshot` → scene revision.
- Slide: close to ready. The shell needs simultaneous source and destination spreads, animation progress/direction, and composition of two cached scene layer sets with translated transforms. Current `try_turn_page` is non-blocking but commits immediately when `Ready`; there is no public peek/non-committing destination-spread API, so the UI cannot retain old session state while asking the session for the destination. `DesktopReader` currently stores only `pending_page_turn`, not transition endpoints.
- Cover: uses the same dual-spread readiness and scene pair, but transforms/clips the moving destination or source over the stationary page. A compositor-owned clip/ordering policy is required; `VelloScene::push_clip_layer` exists.
- Curl/3D: Vello affine scene replay is insufficient for a true perspective/deformed page. It will likely require rendering each spread to textures and a dedicated wgpu mesh/shader/compositing pass. Do not force this into `PageDisplayList` now.

Relevant readiness details:

- Current and adjacent content: `ReaderSession::current_spread`, internal `try_next_position`/`try_previous_position`, and cross-segment/section compilation already exist.
- Pending/ready: `NavigationAttempt::{Pending,Ready}` and prioritized generation cancellation avoid blocking the event loop; `DesktopReader::retry_pending_page_turn` is polled from `ui_controller.rs` with 16 ms repaint scheduling.
- Cache: retained segment pages and desktop `PageSceneLayers` LRU already reduce transition cost. Cache keys identify one primary `ReaderPosition`; spread membership and overlay revision are not explicit in the key.
- Revision: `scene_id`/`scene_revision` only describe a single final scene. Transition progress would otherwise bump/re-render each frame; that is acceptable for slide initially but should be explicit rather than disguised as overlay invalidation.
- Gestures: input is spread across `egui_view.rs` and `interaction.rs`; wheel, click, selection, image long press, focus navigation and pending turns already arbitrate. Drag progress/velocity/cancel and interaction locking are absent.
- Double pages: layout reports `visible_pages` and `ReaderSpread` offsets, and a turn advances by a spread-aware destination. Transition semantics must animate a spread as a unit and preserve reading-direction/page-side rules; do not animate two independently inferred pages.

Minimal cleanup before Slide:

1. Extract reader scene/layer composition from `DesktopReader` behind explicit spread and overlay inputs.
2. Add a reader-level non-committing destination query/handle (or a prepare-then-commit transaction) that reuses `try_*` prefetch behavior; avoid cloning `ReaderSession`.
3. Represent transition state explicitly: source/destination positions and spreads, direction, progress, readiness and commit/cancel.
4. Centralize page-turn gesture arbitration and repaint scheduling.
5. Make compositor cache identity include all static spread inputs while keeping overlays/animation transforms separate.

## 19. Android Readiness

The six core crates are largely Android-ready at the source architecture level: no egui/winit/wgpu/OS APIs leak into them, `ReaderSession` uses portable standard threads, and Cargo already enables wgpu Vulkan/GLES for `target_os = "android"` in the desktop manifest. That manifest flag does not itself provide an Android application.

Concrete coupling to address:

- Font discovery: `LayoutEngine::new` calls system-font loading through resvg/Fontique-related contexts; Android will need explicit bundled/system font provisioning and lifecycle validation. `LayoutEngine::with_fonts` already supplies an injection route.
- File access: `formats::open_file` assumes a path and `std::fs`; Android content URIs should read bytes/platform streams and call `open_bytes`. CHM's path-specialized route must be checked for large-file memory behavior.
- GPU surface: `GpuState` is winit/egui desktop code. Android needs its own surface/event/lifecycle adapter while reusing Vello scene construction and `render_to_texture` concepts.
- Input: keyboard/mouse/wheel/hover and egui gesture logic cannot be reused. Android requires touch gesture arbitration, density/insets and lifecycle-aware repaint scheduling.
- Compositor ownership: reusable Vello scene/cache logic is tied to `DesktopReader`, egui rectangles/colors, highlights and focus UI. Introduce plain compositor inputs before sharing it.
- Thread/lifecycle: `PrefetchWorker` owns a long-lived thread and joins on drop. Android pause/resume, memory pressure, cancellation latency and background restrictions need explicit shell coordination, though the worker has generation invalidation already.
- Memory: fixed-page raster, segment cache, Vello scene cache and GPU target are separately bounded/managed. Android needs one memory-pressure policy and smaller defaults; avoid eagerly collecting `current_section_pages` for long PDF sections.
- Optional services: tokio, reqwest, SQLite, keyring, updater, sync and AI are desktop/application dependencies. They should not enter an Android minimal-reader kernel build.
- Licensing: workspace packages are MIT, but a complete Android dependency/license review is still required. This audit did not verify every transitive dependency's license, so permissive compatibility must not be assumed from Cargo names.

## 20. Recommended Next Steps

1. Mechanically split `rebook-reader`, `rebook-layout`, and `rebook-renderer` into the internal modules listed above, preserving root exports and behavior.
2. Refactor desktop scene construction into an explicit-input `ReaderCompositor` module while leaving it inside `apps/desktop` until a second platform proves the crate boundary.
3. Add source-integrity tests that traverse source range → layout → display list → hit/selection and recover block/section/TOC context, especially across normalized HTML and fixed-page text.
4. Define and test a non-committing destination-spread preparation/commit contract in `ReaderSession`; then implement Slide against two retained spreads and compositor transforms.
5. Build a minimal Android spike using `open_bytes`, explicit fonts, the existing core crates and an Android wgpu surface, excluding egui and all optional AI/sync/persistence features.
