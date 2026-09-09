/// Reader session with section, layout, and display-list caches.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::TryRecvError;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, Weak};
#[cfg(not(target_arch = "wasm32"))]
use std::thread::{self, JoinHandle};

use rebook_layout::{
    LayoutEngine, LayoutError, LayoutViewport, PageItem, ReaderFontBlob, ReaderStyle,
    TypesettingMode,
};
use rebook_publication::{
    Block, Book, BookSource, Inline, LocatorV1, PublicationError, PublicationUrl, RenditionLayout,
    Section, SectionAnchor, SourceAnchor, SourceRange, TableOfContentsOrigin, TextBlock, TextRun,
    TocEntry,
};
use rebook_renderer::{DisplayListCompiler, PageDisplayList, PageImageHit, PageTextHit};
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;

const PREFETCH_DISTANCE: usize = 2;
const DEFAULT_SEGMENT_CACHE_CAPACITY: usize = PREFETCH_DISTANCE * 2 + 3;
const FRAGMENT_TEXT_BUDGET: usize = 4_096;
const LARGE_SECTION_TEXT_BUDGET: usize = FRAGMENT_TEXT_BUDGET * 8;
const FRAGMENT_BLOCK_BUDGET: usize = 64;

/// Direction requested by keyboard, pointer, or command navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDirection {
    Next,
    Previous,
}

/// Semantic unit used to expand pointer-driven text selections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectionGranularity {
    #[default]
    Free,
    Word,
    Sentence,
    Paragraph,
}

/// Returns contiguous sentence ranges in `text` using the shared multilingual
/// reader segmenter and a generic language fallback.
pub fn sentence_byte_ranges(text: &str) -> Vec<Range<usize>> {
    sentence_byte_ranges_with_language(text, "en")
}

/// Returns sentence ranges as UTF-8 byte offsets while preserving every source
/// character, including boundary whitespace and punctuation.
pub fn sentence_byte_ranges_with_language(text: &str, language_hint: &str) -> Vec<Range<usize>> {
    let language = sentence_language_for_text(text, language_hint);
    sentencex::get_sentence_boundaries(&language, text)
        .into_iter()
        .map(|boundary| boundary.start_byte..boundary.end_byte)
        .collect()
}

/// Returns sentence ranges as Unicode scalar indices. This is used by derived
/// Inline views whose diagnostic text keeps the same character count as the
/// original source but may intentionally mask formulas or footnotes.
pub fn sentence_char_ranges(text: &str, language_hint: &str) -> Vec<Range<usize>> {
    let language = sentence_language_for_text(text, language_hint);
    sentencex::get_sentence_boundaries(&language, text)
        .into_iter()
        .map(|boundary| boundary.start_index..boundary.end_index)
        .collect()
}

fn sentence_language_for_text(text: &str, language_hint: &str) -> String {
    if text
        .chars()
        .any(|character| matches!(character, '\u{3040}'..='\u{30ff}'))
    {
        return "ja".to_owned();
    }
    if text.chars().any(|character| {
        matches!(
            character,
            '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '。' | '！' | '？'
        )
    }) {
        return "zh".to_owned();
    }
    language_hint
        .split(['-', '_'])
        .next()
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .unwrap_or("en")
        .to_ascii_lowercase()
}

/// Stable current position exposed to the application shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderLocation {
    pub section_index: usize,
    pub segment_index: usize,
    pub segment_count: usize,
    pub page_index: usize,
    pub page_count: usize,
}

/// Resolved random-access destination in the current pagination generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReaderPosition {
    pub section_index: usize,
    pub segment_index: usize,
    pub page_index: usize,
}

impl From<ReaderLocation> for ReaderPosition {
    fn from(loc: ReaderLocation) -> Self {
        Self {
            section_index: loc.section_index,
            segment_index: loc.segment_index,
            page_index: loc.page_index,
        }
    }
}

/// A pointer-resolved text position tied to the current pagination generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderTextHit {
    position: ReaderPosition,
    region_index: usize,
    byte_index: usize,
    cluster_start: usize,
    cluster_end: usize,
}

/// Page-coordinate rectangle used to paint a native selection and anchor its
/// floating action toolbar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReaderSelectionRect {
    pub position: ReaderPosition,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Durable source ranges plus transient geometry for the active native text
/// selection. Each range belongs to one source-backed text block.
#[derive(Debug, Clone, PartialEq)]
pub struct ReaderSelection {
    pub ranges: Vec<SourceRange>,
    pub text: String,
    pub rects: Vec<ReaderSelectionRect>,
}

/// Original image pixels resolved from a point in the visible reader spread.
#[derive(Clone)]
pub struct ReaderImage {
    pub position: ReaderPosition,
    /// Left edge in the coordinate space used for the image query.
    pub x: f32,
    /// Top edge in the coordinate space used for the image query.
    pub y: f32,
    pub display_width: f32,
    pub display_height: f32,
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>,
}

/// One source-backed text fragment retained on a logical page in the current
/// visible spread. The source range remains stable while `position` identifies
/// the page that supplied the visible quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderVisibleTextFragment {
    pub position: ReaderPosition,
    pub range: SourceRange,
    pub text: String,
}

/// One visual reader surface assembled from adjacent logical pages. In double
/// mode the secondary page may come from the next layout segment or authored
/// spine section.
#[derive(Clone)]
pub struct ReaderSpread {
    pub primary: Arc<PageDisplayList>,
    pub secondary: Option<Arc<PageDisplayList>>,
    pub primary_offset_x: f32,
    pub secondary_offset_x: f32,
}

/// One compiled logical page in the active authored section.
#[derive(Clone)]
pub struct ReaderSectionPage {
    pub position: ReaderPosition,
    pub page: Arc<PageDisplayList>,
    /// Whether this page only reserves fixed-page geometry and still needs its
    /// real raster/display list to be materialized near the viewport.
    pub placeholder: bool,
    /// Optional top crop in logical page coordinates for semantic reading views.
    pub visible_top: Option<f32>,
    /// Optional bottom crop in logical page coordinates for semantic reading views.
    pub visible_bottom: Option<f32>,
}

/// Position inside the semantic table-of-contents units of the current view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadingUnitLocation {
    pub index: usize,
    pub count: usize,
}

/// Flattened, presentation-ready table-of-contents item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocViewItem {
    pub id: String,
    pub label: String,
    pub target: Option<PublicationUrl>,
    pub depth: usize,
    pub ancestors: Vec<String>,
    pub has_children: bool,
}

/// Complete reader state after a command has been applied.
#[derive(Debug, Clone, PartialEq)]
pub struct ReaderSnapshot {
    pub location: ReaderLocation,
    pub total_progression: f64,
    pub active_toc_id: Option<String>,
    pub active_toc_path: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationOutcome {
    Moved,
    Boundary,
}

/// Navigation always returns the resulting state, including at book boundaries.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationResult {
    pub outcome: NavigationOutcome,
    pub snapshot: ReaderSnapshot,
}

/// Result of an interactive navigation attempt. A pending result means the
/// destination is being prepared by the background pagination worker and the
/// caller should retry without blocking its event loop.
#[derive(Debug, Clone, PartialEq)]
pub enum NavigationAttempt {
    Ready(NavigationResult),
    Pending,
}

/// Opaque identity of a prepared or pending navigation transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NavigationToken {
    id: u64,
    generation: u64,
}

/// State of a non-committing navigation preparation.
pub enum NavigationPreparation {
    Ready(PreparedNavigation),
    Pending(NavigationToken),
    Boundary,
}

/// Result of a cooperative work quantum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickResult {
    /// Work queue is empty or no background tasks are pending.
    Idle,
    /// Bounded budget expired while more work remains in the queue.
    MoreWorkRemaining,
}

/// A prepared navigation transaction holding source and destination spreads.
pub struct PreparedNavigation {
    token: NavigationToken,
    direction: PageDirection,
    source: ReaderPosition,
    destination: ReaderPosition,
    destination_spread: ReaderSpread,
}

impl PreparedNavigation {
    pub const fn token(&self) -> NavigationToken {
        self.token
    }

    pub const fn direction(&self) -> PageDirection {
        self.direction
    }

    pub const fn source(&self) -> ReaderPosition {
        self.source
    }

    pub const fn destination(&self) -> ReaderPosition {
        self.destination
    }

    pub const fn destination_spread(&self) -> &ReaderSpread {
        &self.destination_spread
    }
}

enum PositionAttempt {
    Ready(Option<ReaderPosition>),
    Pending,
}

struct CachedSegment {
    section: Arc<PreparedSection>,
    pages: Vec<Arc<PageDisplayList>>,
    anchor_pages: HashMap<String, usize>,
    visible_pages: usize,
    continuation_offset_x: f32,
}

struct PreparedSection {
    fragments: Vec<ContentFragment>,
    segments: Vec<LayoutSegment>,
    anchor_segments: HashMap<String, usize>,
    reading_units: Vec<ReadingUnit>,
}

struct ContentFragment {
    blocks: Vec<Block>,
    anchors: Vec<rebook_publication::SectionAnchor>,
}

struct LayoutSegment {
    fragment_range: Range<usize>,
}

struct ReadingUnit {
    fragment_range: Range<usize>,
    start: Option<SourceAnchor>,
}

/// Semantic reading units for fixed-layout publications. A PDF physical page
/// is represented by one spine section, so a TOC leaf unit can span several
/// authored sections without cropping any of those pages.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FixedReadingUnit {
    section_range: Range<usize>,
}

struct SectionRepository {
    source: Arc<dyn BookSource>,
    sections: Vec<SectionSlot>,
}

struct SectionSlot {
    state: Mutex<SectionSlotState>,
    ready: Condvar,
}

enum SectionSlotState {
    Empty,
    Loading,
    Ready(Weak<PreparedSection>),
}

impl SectionRepository {
    fn new(source: Arc<dyn BookSource>) -> Self {
        let section_count = source.book().sections.len();
        Self {
            source,
            sections: (0..section_count)
                .map(|_| SectionSlot {
                    state: Mutex::new(SectionSlotState::Empty),
                    ready: Condvar::new(),
                })
                .collect(),
        }
    }

    fn get(&self, index: usize) -> Option<Arc<PreparedSection>> {
        let slot = self.sections.get(index)?;
        let state = slot.state.lock().ok()?;
        match &*state {
            SectionSlotState::Ready(section) => section.upgrade(),
            SectionSlotState::Empty | SectionSlotState::Loading => None,
        }
    }

    fn load(&self, index: usize) -> Result<Arc<PreparedSection>, ReaderError> {
        let slot = self
            .sections
            .get(index)
            .ok_or(ReaderError::SectionOutOfBounds(index))?;
        loop {
            let mut state = slot
                .state
                .lock()
                .map_err(|_| ReaderError::SectionRepositoryPoisoned)?;
            match &*state {
                SectionSlotState::Ready(section) => {
                    if let Some(section) = section.upgrade() {
                        return Ok(section);
                    }
                    *state = SectionSlotState::Loading;
                }
                SectionSlotState::Empty => *state = SectionSlotState::Loading,
                SectionSlotState::Loading => {
                    drop(
                        slot.ready
                            .wait(state)
                            .map_err(|_| ReaderError::SectionRepositoryPoisoned)?,
                    );
                    continue;
                }
            }
            drop(state);

            let layout_boundaries = top_level_toc_fragments_for_section(self.source.book(), index);
            let reading_boundaries = semantic_toc_boundaries_for_section(self.source.book(), index);
            let parsed = self
                .source
                .parse_section(index)
                .map(|section| prepare_section(section, &layout_boundaries, &reading_boundaries));
            let mut state = slot
                .state
                .lock()
                .map_err(|_| ReaderError::SectionRepositoryPoisoned)?;
            match parsed {
                Ok(section) => {
                    let section = Arc::new(section);
                    *state = SectionSlotState::Ready(Arc::downgrade(&section));
                    slot.ready.notify_all();
                    return Ok(section);
                }
                Err(error) => {
                    *state = SectionSlotState::Empty;
                    slot.ready.notify_all();
                    return Err(error.into());
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SegmentKey {
    section_index: usize,
    segment_index: usize,
}

struct PrefetchRequest {
    key: SegmentKey,
    viewport: LayoutViewport,
    style: ReaderStyle,
    generation: u64,
}

struct PrefetchResult {
    key: SegmentKey,
    generation: u64,
    segment: Result<Arc<CachedSegment>, ReaderError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PrefetchKey {
    generation: u64,
    segment: SegmentKey,
}

#[cfg(not(target_arch = "wasm32"))]
struct PrefetchWorker {
    requests: Option<Sender<PrefetchRequest>>,
    results: Mutex<Receiver<PrefetchResult>>,
    active_generation: Arc<AtomicU64>,
    active_request: Arc<Mutex<Option<PrefetchKey>>>,
    cancelled: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PrefetchWorker {
    fn spawn(
        source: Arc<dyn BookSource>,
        repository: Arc<SectionRepository>,
        fonts: Arc<[ReaderFontBlob]>,
    ) -> Result<Self, ReaderError> {
        let (request_sender, request_receiver) = mpsc::channel::<PrefetchRequest>();
        let (result_sender, result_receiver) = mpsc::channel::<PrefetchResult>();
        let active_generation = Arc::new(AtomicU64::new(0));
        let active_request = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_generation = Arc::clone(&active_generation);
        let worker_active_request = Arc::clone(&active_request);
        let worker_cancelled = Arc::clone(&cancelled);
        let handle = thread::Builder::new()
            .name("rebook-prefetch".into())
            .spawn(move || {
                let mut layout_engine = LayoutEngine::with_fonts(fonts.iter().cloned());
                let display_compiler = DisplayListCompiler;
                while let Ok(request) = request_receiver.recv() {
                    if worker_cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    if worker_generation.load(Ordering::Acquire) != request.generation {
                        continue;
                    }
                    let request_key = PrefetchKey {
                        generation: request.generation,
                        segment: request.key,
                    };
                    if let Ok(mut active) = worker_active_request.lock() {
                        *active = Some(request_key);
                    }
                    let segment = repository
                        .load(request.key.section_index)
                        .and_then(|section| {
                            compile_segment(
                                source.as_ref(),
                                section,
                                request.key,
                                request.viewport,
                                &request.style,
                                &mut layout_engine,
                                &display_compiler,
                            )
                            .map(Arc::new)
                        });
                    if let Ok(mut active) = worker_active_request.lock()
                        && *active == Some(request_key)
                    {
                        *active = None;
                    }
                    if worker_cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    if worker_generation.load(Ordering::Acquire) != request.generation {
                        continue;
                    }
                    if result_sender
                        .send(PrefetchResult {
                            key: request.key,
                            generation: request.generation,
                            segment,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(ReaderError::PrefetchWorkerStart)?;
        Ok(Self {
            requests: Some(request_sender),
            results: Mutex::new(result_receiver),
            active_generation,
            active_request,
            cancelled,
            handle: Some(handle),
        })
    }

    fn generation(&self) -> u64 {
        self.active_generation.load(Ordering::Acquire)
    }

    fn invalidate(&self) -> u64 {
        self.active_generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn active_key(&self) -> Option<PrefetchKey> {
        self.active_request.lock().ok().and_then(|active| *active)
    }

    fn send(&self, request: PrefetchRequest) -> Result<(), ReaderError> {
        self.requests
            .as_ref()
            .ok_or(ReaderError::PrefetchWorkerStopped)?
            .send(request)
            .map_err(|_| ReaderError::PrefetchWorkerStopped)
    }

    fn recv(&self) -> Result<PrefetchResult, ReaderError> {
        self.results
            .lock()
            .map_err(|_| ReaderError::PrefetchWorkerStopped)?
            .recv()
            .map_err(|_| ReaderError::PrefetchWorkerStopped)
    }

    fn try_recv(&self) -> Result<PrefetchResult, TryRecvError> {
        self.results
            .lock()
            .map_or(Err(TryRecvError::Disconnected), |results| {
                results.try_recv()
            })
    }

    #[allow(clippy::unused_self)]
    fn tick(&self, _budget: std::time::Duration) -> TickResult {
        TickResult::Idle
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for PrefetchWorker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.requests.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct WasmPrefetchState {
    source: Arc<dyn BookSource>,
    repository: Arc<SectionRepository>,
    layout_engine: LayoutEngine,
    pending: VecDeque<PrefetchRequest>,
    completed: VecDeque<PrefetchResult>,
}

#[cfg(target_arch = "wasm32")]
struct PrefetchWorker {
    state: Mutex<WasmPrefetchState>,
    active_generation: Arc<AtomicU64>,
}

#[cfg(target_arch = "wasm32")]
impl PrefetchWorker {
    fn spawn(
        source: Arc<dyn BookSource>,
        repository: Arc<SectionRepository>,
        fonts: Arc<[ReaderFontBlob]>,
    ) -> Result<Self, ReaderError> {
        let layout_engine = LayoutEngine::with_fonts(fonts.iter().cloned());
        Ok(Self {
            state: Mutex::new(WasmPrefetchState {
                source,
                repository,
                layout_engine,
                pending: VecDeque::new(),
                completed: VecDeque::new(),
            }),
            active_generation: Arc::new(AtomicU64::new(0)),
        })
    }

    fn generation(&self) -> u64 {
        self.active_generation.load(Ordering::Acquire)
    }

    fn invalidate(&self) -> u64 {
        let next_generation = self.active_generation.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut state) = self.state.lock() {
            state.pending.clear();
            state.completed.clear();
        }
        next_generation
    }

    fn active_key(&self) -> Option<PrefetchKey> {
        None
    }

    fn send(&self, request: PrefetchRequest) -> Result<(), ReaderError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReaderError::PrefetchWorkerStopped)?;
        state.pending.push_back(request);
        Ok(())
    }

    fn step_one(state: &mut WasmPrefetchState, active_gen: u64) -> Option<PrefetchResult> {
        let display_compiler = DisplayListCompiler;
        while let Some(request) = state.pending.pop_front() {
            if request.generation != active_gen {
                continue;
            }
            let segment = state
                .repository
                .load(request.key.section_index)
                .and_then(|section| {
                    compile_segment(
                        state.source.as_ref(),
                        section,
                        request.key,
                        request.viewport,
                        &request.style,
                        &mut state.layout_engine,
                        &display_compiler,
                    )
                    .map(Arc::new)
                });
            return Some(PrefetchResult {
                key: request.key,
                generation: request.generation,
                segment,
            });
        }
        None
    }

    fn recv(&self) -> Result<PrefetchResult, ReaderError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReaderError::PrefetchWorkerStopped)?;
        let active_gen = self.active_generation.load(Ordering::Acquire);
        if let Some(res) = state.completed.pop_front() {
            return Ok(res);
        }
        if let Some(res) = Self::step_one(&mut state, active_gen) {
            return Ok(res);
        }
        Err(ReaderError::PrefetchWorkerStopped)
    }

    fn try_recv(&self) -> Result<PrefetchResult, TryRecvError> {
        let mut state = self.state.lock().map_err(|_| TryRecvError::Disconnected)?;
        if let Some(res) = state.completed.pop_front() {
            return Ok(res);
        }
        Err(TryRecvError::Empty)
    }

    fn tick(&self, budget: std::time::Duration) -> TickResult {
        let Ok(mut state) = self.state.lock() else {
            return TickResult::Idle;
        };
        let active_gen = self.active_generation.load(Ordering::Acquire);
        let start = web_time::Instant::now();
        loop {
            if state.pending.is_empty() {
                return TickResult::Idle;
            }
            if start.elapsed() >= budget {
                return TickResult::MoreWorkRemaining;
            }
            if let Some(res) = Self::step_one(&mut state, active_gen) {
                state.completed.push_back(res);
            }
        }
    }
}

struct TocIndex {
    items_by_section: Vec<Vec<usize>>,
    preceding_section_by_section: Vec<Option<usize>>,
}

impl TocIndex {
    fn new(
        items: &[TocViewItem],
        section_indices_by_path: &HashMap<String, usize>,
        section_count: usize,
    ) -> Self {
        let mut items_by_section = vec![Vec::new(); section_count];
        for (item_index, item) in items.iter().enumerate() {
            let Some(section_index) = item
                .target
                .as_ref()
                .and_then(|target| section_indices_by_path.get(target.path()))
                .copied()
            else {
                continue;
            };
            items_by_section[section_index].push(item_index);
        }

        let mut preceding_section_by_section = Vec::with_capacity(section_count);
        let mut preceding_section = None;
        for (section_index, section_items) in items_by_section.iter().enumerate() {
            preceding_section_by_section.push(preceding_section);
            if !section_items.is_empty() {
                preceding_section = Some(section_index);
            }
        }

        Self {
            items_by_section,
            preceding_section_by_section,
        }
    }
}
