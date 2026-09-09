use rebook_layout::LayoutViewport;
use rebook_publication::SourceRange;
use rebook_reader::{ReaderPosition, ReaderSpread};

use crate::transition::TransitionKind;

/// Semantic ranges dynamically highlighted or focused over a page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlaySet {
    pub highlights: Vec<SourceRange>,
    pub selection: Vec<SourceRange>,
    pub focus: Vec<SourceRange>,
}

/// Unique identifier for a rendered page in a specific layout generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageFrameKey {
    pub position: ReaderPosition,
    pub layout_generation: u64,
}

/// Unique identifier for a visual spread composed of one or two pages.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpreadFrameKey {
    pub primary: PageFrameKey,
    pub secondary: Option<PageFrameKey>,
    pub width: u32,
    pub height: u32,
}

/// State of transition animation between spreads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameTransition {
    None,
    Slide {
        progress: f32,
        primary_offset_x: f32,
        destination_offset_x: f32,
    },
    Curl {
        direction: rebook_reader::PageDirection,
        progress: f32,
        start_x_ratio: f32,
        start_y_ratio: f32,
        current_x_ratio: f32,
        current_y_ratio: f32,
    },
}

/// Platform-independent prepared reader frame constructed by the engine.
///
/// Contains stable identifiers, viewport geometry, retained spread display lists,
/// and transition state. Backends use this to determine if textures or static scenes
/// need rebuilding without inspecting DOM or platform state.
#[derive(Clone)]
pub struct PreparedReaderFrame {
    pub key: SpreadFrameKey,
    pub destination_key: Option<SpreadFrameKey>,
    pub viewport: LayoutViewport,
    pub layout_generation: u64,
    pub content_revision: u64,
    pub overlay_revision: u64,
    pub overlays: OverlaySet,
    pub current_spread: ReaderSpread,
    pub destination_spread: Option<ReaderSpread>,
    pub transition_kind: TransitionKind,
    pub transition: FrameTransition,
    pub requires_next_frame: bool,
}

impl PreparedReaderFrame {
    pub fn is_transitioning(&self) -> bool {
        !matches!(self.transition, FrameTransition::None)
    }
}
