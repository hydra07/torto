use std::sync::Arc;

use rebook_publication::{PublicationId, PublicationUrl, SourceAnchor};

use crate::{
    AppLifecycleEvent, Engine, EngineAnimationState, EngineBook, EngineConfig, EngineError,
    EngineNavigationState, EngineReader, MemoryPressure, NavigationResult, OpenReaderRequest,
    PageDirection, PointerEvent, PointerGestureResult, PreparedReaderFrame, ReaderSelection,
    ReaderSnapshot, ReaderStyle, SourceRange, TickResult, TocViewItem, ViewportMetrics,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenBookSummary {
    pub publication_id: PublicationId,
    pub title: String,
    pub section_count: usize,
}

/// Platform work requested by the engine after a lifecycle or memory event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformDirective {
    None,
    RequestFrame,
    ReleaseTransientRenderResources,
}

/// Complete platform-neutral reader runtime.
///
/// Platforms provide book bytes, viewport/input events, persistence and a
/// render surface. The runtime owns parsing, layout, navigation, selection,
/// pagination, reader state and engine lifecycle.
pub struct EngineRuntime {
    engine: Engine,
    book: Option<EngineBook>,
    reader: Option<EngineReader>,
    viewport: ViewportMetrics,
    suspended: bool,
}

impl EngineRuntime {
    pub fn new(config: EngineConfig, viewport: ViewportMetrics) -> Self {
        Self {
            engine: Engine::new(config),
            book: None,
            reader: None,
            viewport,
            suspended: false,
        }
    }

    pub fn open_bytes(
        &mut self,
        bytes: impl Into<Arc<[u8]>>,
        file_name: &str,
    ) -> Result<OpenBookSummary, EngineError> {
        self.open(OpenReaderRequest::new(bytes, file_name, self.viewport))
    }

    /// Opens and configures a book in one transaction. Style, locator and
    /// overlays are installed before the first platform-visible frame.
    pub fn open(&mut self, request: OpenReaderRequest) -> Result<OpenBookSummary, EngineError> {
        self.close();
        self.viewport = request.viewport;
        let book = self
            .engine
            .open_bytes(Arc::clone(&request.bytes), &request.file_name)?;
        let summary = OpenBookSummary {
            publication_id: book.book().id.clone(),
            title: book.book().metadata.title.clone(),
            section_count: book.book().sections.len(),
        };
        let mut reader = self.engine.create_reader(&book, request.reader_config())?;
        reader.set_highlights(request.highlights);
        reader.set_focus_ranges(request.focus);
        self.book = Some(book);
        self.reader = Some(reader);
        Ok(summary)
    }

    pub fn close(&mut self) {
        self.reader = None;
        self.book = None;
    }

    pub fn is_open(&self) -> bool {
        self.reader.is_some()
    }

    pub fn viewport(&self) -> ViewportMetrics {
        self.viewport
    }

    pub fn resize(&mut self, viewport: ViewportMetrics) -> Result<(), EngineError> {
        let layout_changed = self.viewport.layout != viewport.layout;
        self.viewport = viewport;
        if layout_changed && let Some(reader) = self.reader.as_mut() {
            reader.resize(viewport.layout)?;
        }
        Ok(())
    }

    pub fn reader(&self) -> Option<&EngineReader> {
        self.reader.as_ref()
    }

    pub fn reader_mut(&mut self) -> Option<&mut EngineReader> {
        self.reader.as_mut()
    }

    pub fn snapshot(&self) -> Result<ReaderSnapshot, EngineError> {
        Ok(self.require_reader()?.snapshot())
    }

    pub fn locator(&self) -> Result<crate::LocatorV1, EngineError> {
        Ok(self.require_reader()?.current_locator())
    }

    pub fn restore_locator(
        &mut self,
        locator: &crate::LocatorV1,
    ) -> Result<NavigationResult, EngineError> {
        Ok(self.require_reader_mut()?.restore_locator(locator)?)
    }

    pub fn go_to_toc_item(&mut self, id: &str) -> Result<NavigationResult, EngineError> {
        Ok(self.require_reader_mut()?.go_to_toc_item(id)?)
    }

    pub fn go_to_href(&mut self, href: &PublicationUrl) -> Result<NavigationResult, EngineError> {
        Ok(self.require_reader_mut()?.go_to_href(href)?)
    }

    pub fn go_to_source(&mut self, anchor: &SourceAnchor) -> Result<NavigationResult, EngineError> {
        Ok(self.require_reader_mut()?.go_to_source(anchor)?)
    }

    pub fn toc(&self) -> Result<&[TocViewItem], EngineError> {
        Ok(self.require_reader()?.toc_items())
    }

    pub fn navigate(
        &mut self,
        direction: PageDirection,
    ) -> Result<EngineNavigationState, EngineError> {
        Ok(self.require_reader_mut()?.navigation_step(direction)?)
    }

    pub fn pointer(&mut self, event: PointerEvent) -> Result<PointerGestureResult, EngineError> {
        Ok(self.require_reader_mut()?.handle_pointer(event)?)
    }

    pub fn animation_step(
        &mut self,
        timestamp_ms: f64,
    ) -> Result<EngineAnimationState, EngineError> {
        Ok(self.require_reader_mut()?.animation_step(timestamp_ms)?)
    }

    pub fn begin_selection(&mut self, x: f32, y: f32) -> Result<bool, EngineError> {
        Ok(self.require_reader_mut()?.begin_text_selection(x, y)?)
    }

    pub fn update_selection(&mut self, x: f32, y: f32) -> Result<bool, EngineError> {
        Ok(self.require_reader_mut()?.update_text_selection(x, y)?)
    }

    pub fn end_selection(&mut self) -> Result<Option<String>, EngineError> {
        Ok(self
            .require_reader_mut()?
            .end_text_selection()
            .map(str::to_owned))
    }

    pub fn clear_selection(&mut self) -> Result<bool, EngineError> {
        Ok(self.require_reader_mut()?.clear_text_selection())
    }

    pub fn selection(&self) -> Result<Option<&ReaderSelection>, EngineError> {
        Ok(self.require_reader()?.selection())
    }

    pub fn set_style(&mut self, style: ReaderStyle) -> Result<(), EngineError> {
        self.require_reader_mut()?.set_style(style)?;
        Ok(())
    }

    pub fn set_highlights(&mut self, ranges: Vec<SourceRange>) -> Result<(), EngineError> {
        self.require_reader_mut()?.set_highlights(ranges);
        Ok(())
    }

    pub fn set_focus(&mut self, ranges: Vec<SourceRange>) -> Result<(), EngineError> {
        self.require_reader_mut()?.set_focus_ranges(ranges);
        Ok(())
    }

    pub fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<crate::SearchResult>, EngineError> {
        self.require_reader()?
            .search(query, max_results)
            .map_err(EngineError::Search)
    }

    pub fn tick(&mut self, budget: std::time::Duration) -> Result<TickResult, EngineError> {
        Ok(self.require_reader_mut()?.tick(budget)?)
    }

    pub fn frame(&mut self) -> Result<PreparedReaderFrame, EngineError> {
        Ok(self.require_reader_mut()?.frame()?)
    }

    pub fn lifecycle(&mut self, event: AppLifecycleEvent, timestamp_ms: f64) -> PlatformDirective {
        self.suspended = matches!(event, AppLifecycleEvent::Suspended);
        if let Some(reader) = self.reader.as_mut() {
            reader.handle_lifecycle(event, timestamp_ms);
        }
        match event {
            AppLifecycleEvent::Resumed | AppLifecycleEvent::SurfaceRestored => {
                PlatformDirective::RequestFrame
            }
            AppLifecycleEvent::Suspended | AppLifecycleEvent::SurfaceLost => {
                PlatformDirective::ReleaseTransientRenderResources
            }
        }
    }

    pub fn memory_pressure(&mut self, level: MemoryPressure) -> PlatformDirective {
        if let Some(reader) = self.reader.as_mut() {
            reader.handle_memory_pressure(level);
        }
        PlatformDirective::ReleaseTransientRenderResources
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    fn require_reader(&self) -> Result<&EngineReader, EngineError> {
        self.reader.as_ref().ok_or(EngineError::NoBookOpen)
    }

    fn require_reader_mut(&mut self) -> Result<&mut EngineReader, EngineError> {
        self.reader.as_mut().ok_or(EngineError::NoBookOpen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_resize_does_not_reflow_logical_layout() {
        let initial = ViewportMetrics::new(400, 800, 800, 1600, 2.0);
        let mut runtime = EngineRuntime::new(EngineConfig::default(), initial);
        runtime
            .resize(ViewportMetrics::new(400, 800, 1200, 2400, 3.0))
            .unwrap();
        assert_eq!(runtime.viewport().layout, initial.layout);
        assert_eq!(runtime.viewport().surface_width, 1200);
    }

    #[test]
    fn lifecycle_is_safe_without_an_open_book() {
        let viewport = ViewportMetrics::from_logical_size(400, 800, 2.0);
        let mut runtime = EngineRuntime::new(EngineConfig::default(), viewport);
        assert_eq!(
            runtime.lifecycle(AppLifecycleEvent::Suspended, 10.0),
            PlatformDirective::ReleaseTransientRenderResources
        );
        assert!(runtime.is_suspended());
    }
}
