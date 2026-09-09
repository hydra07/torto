use std::sync::Arc;

use rebook_layout::{LayoutViewport, ReaderFontBlob, ReaderStyle};
use rebook_publication::{LocatorV1, SourceRange};

use crate::ViewportMetrics;

#[derive(Clone, Default)]
pub struct EngineConfig {
    pub fonts: Arc<[ReaderFontBlob]>,
}

#[derive(Clone)]
pub struct ReaderConfig {
    pub viewport: LayoutViewport,
    pub style: ReaderStyle,
    pub locator: Option<LocatorV1>,
}

/// Complete input required to open a reader. Platform settings and persisted
/// state can be applied in one engine call without intermediate reflows.
#[derive(Clone)]
pub struct OpenReaderRequest {
    pub bytes: Arc<[u8]>,
    pub file_name: String,
    pub viewport: ViewportMetrics,
    pub style: ReaderStyle,
    pub locator: Option<LocatorV1>,
    pub highlights: Vec<SourceRange>,
    pub focus: Vec<SourceRange>,
}

impl OpenReaderRequest {
    pub fn new(
        bytes: impl Into<Arc<[u8]>>,
        file_name: impl Into<String>,
        viewport: ViewportMetrics,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            file_name: file_name.into(),
            viewport,
            style: ReaderStyle::default(),
            locator: None,
            highlights: Vec::new(),
            focus: Vec::new(),
        }
    }

    pub fn reader_config(&self) -> ReaderConfig {
        ReaderConfig {
            viewport: self.viewport.layout,
            style: self.style.clone(),
            locator: self.locator.clone(),
        }
    }
}

impl ReaderConfig {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            viewport: LayoutViewport { width, height },
            style: ReaderStyle::default(),
            locator: None,
        }
    }

    pub fn from_viewport(viewport: ViewportMetrics) -> Self {
        Self {
            viewport: viewport.layout,
            style: ReaderStyle::default(),
            locator: None,
        }
    }
}
