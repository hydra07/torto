mod book;
mod config;
mod error;
pub mod features;
pub mod frame;
pub mod input;
pub mod platform;
mod reader;
mod runtime;
pub mod transition;

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::sync::Arc;

use rebook_formats::open_bytes;
use rebook_layout::ReaderFontBlob;

pub use book::EngineBook;
pub use config::{EngineConfig, OpenReaderRequest, ReaderConfig};
pub use error::EngineError;
pub use features::{SearchResult, search_book};
pub use frame::{FrameTransition, OverlaySet, PageFrameKey, PreparedReaderFrame, SpreadFrameKey};
pub use input::{PointerEvent, PointerKind, PointerPhase};
pub use platform::{AppLifecycleEvent, MemoryPressure, ViewportMetrics};
pub use reader::{EngineAnimationState, EngineNavigationState, EngineReader};
pub use rebook_layout::{
    LayoutViewport, ParagraphIndentMode, ReaderDefaultFont, ReaderFontChoice, ReaderStyle,
    ReaderTypesetting, ReaderTypography, SpreadMode, TypesettingMode,
};
pub use rebook_publication::{LocatorV1, Rgba, SourceRange};
pub use rebook_reader::{
    NavigationAttempt, NavigationOutcome, NavigationPreparation, NavigationResult, NavigationToken,
    PageDirection, PreparedNavigation, ReaderError, ReaderPosition, ReaderSelection,
    ReaderSelectionRect, ReaderSession, ReaderSnapshot, ReaderSpread, SelectionGranularity,
    TickResult, TocViewItem,
};
pub use runtime::{EngineRuntime, OpenBookSummary, PlatformDirective};
pub use transition::PointerGestureResult;

pub struct Engine {
    fonts: Arc<[ReaderFontBlob]>,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Self {
        Self {
            fonts: config.fonts,
        }
    }

    pub fn fonts(&self) -> Arc<[ReaderFontBlob]> {
        Arc::clone(&self.fonts)
    }

    pub fn open_bytes(
        &self,
        bytes: impl Into<Arc<[u8]>>,
        file_name: &str,
    ) -> Result<EngineBook, EngineError> {
        let opened = open_bytes(bytes, file_name)?;
        Ok(EngineBook::new(opened))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_file(&self, path: impl AsRef<Path>) -> Result<EngineBook, EngineError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("book");
        self.open_bytes(bytes, file_name)
    }

    pub fn create_reader(
        &self,
        book: &EngineBook,
        config: ReaderConfig,
    ) -> Result<EngineReader, EngineError> {
        let session = if let Some(locator) = &config.locator {
            ReaderSession::open_with_fonts_at_locator(
                book.source(),
                config.viewport,
                config.style,
                Arc::clone(&self.fonts),
                locator,
            )?
        } else {
            ReaderSession::open_with_fonts(
                book.source(),
                config.viewport,
                config.style,
                Arc::clone(&self.fonts),
            )?
        };
        let mut reader = EngineReader::new(session);
        // Warm the adjacent reading units as an engine policy. Native builds
        // execute this on the worker; single-threaded WASM advances it through
        // `EngineReader::tick` when the platform scheduler grants a time slice.
        reader.prefetch_adjacent()?;
        Ok(reader)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(EngineConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rebook_layout::{LayoutViewport, ReaderStyle};
    use rebook_publication::{
        Block, BlockStyle, Book, BookSource, Inline, Metadata, PublicationError, PublicationId,
        PublicationUrl, Resource, Section, SourceAnchor, SpineItem, SpineItemId, TextBlock,
        TextBlockKind, TextRun, TextStyle,
    };
    use std::sync::Arc;

    struct InMemorySource {
        book: Book,
        section: Section,
    }

    impl InMemorySource {
        fn new() -> Self {
            let id = SpineItemId::new("chapter1").unwrap();
            let href = PublicationUrl::parse("chapter1.xhtml").unwrap();
            let section = Section {
                id: id.clone(),
                href: href.clone(),
                blocks: vec![Block::Text(TextBlock {
                    kind: TextBlockKind::Paragraph,
                    content: vec![Inline::Text(TextRun {
                        text: "Hello world from engine facade! ".repeat(200),
                        style: TextStyle::default(),
                        link: None,
                    })],
                    style: BlockStyle::default(),
                    source: None,
                })],
                anchors: Vec::new(),
            };
            let book = Book {
                id: PublicationId::new("test-book").unwrap(),
                metadata: Metadata {
                    title: "Test Book".into(),
                    authors: vec!["Author".into()],
                    ..Default::default()
                },
                table_of_contents: Vec::new(),
                sections: vec![SpineItem {
                    id,
                    href,
                    media_type: "application/xhtml+xml".into(),
                    linear: true,
                    properties: Vec::new(),
                }],
                cover: None,
            };
            Self { book, section }
        }
    }

    impl BookSource for InMemorySource {
        fn book(&self) -> &Book {
            &self.book
        }

        fn parse_section(&self, _index: usize) -> Result<Section, PublicationError> {
            Ok(self.section.clone())
        }

        fn resource(&self, href: &PublicationUrl) -> Result<Resource, PublicationError> {
            Err(PublicationError::ResourceNotFound(href.to_string()))
        }
    }

    #[test]
    fn unsupported_file_name_produces_typed_error() {
        let engine = Engine::default();
        let result = engine.open_bytes(vec![0, 1, 2, 3], "unknown.xyz");
        assert!(result.is_err());
        match result {
            Err(EngineError::Format(rebook_formats::FormatError::UnsupportedFormat(name))) => {
                assert_eq!(name, "unknown.xyz");
            }
            _ => panic!("expected UnsupportedFormat"),
        }
    }

    #[test]
    fn reader_creation_and_spread_works_with_custom_source() {
        let source: Arc<dyn BookSource> = Arc::new(InMemorySource::new());
        let session = ReaderSession::open_with_fonts(
            source,
            LayoutViewport {
                width: 800,
                height: 1000,
            },
            ReaderStyle::default(),
            Arc::default(),
        )
        .unwrap();
        let mut reader = EngineReader::new(session);

        assert_eq!(reader.book().metadata.title, "Test Book");
        let spread = reader.current_spread().unwrap();
        assert!(spread.primary.width() > 0);
        let locator = reader.current_locator();
        assert_eq!(locator.publication_id.as_str(), "test-book");

        // Test navigation preparation on facade
        let boundary = reader.prepare_navigation(PageDirection::Previous).unwrap();
        assert!(matches!(boundary, NavigationPreparation::Boundary));
    }

    #[test]
    fn facade_exposes_internal_href_and_source_navigation() {
        let source: Arc<dyn BookSource> = Arc::new(InMemorySource::new());
        let session = ReaderSession::open_with_fonts(
            source,
            LayoutViewport {
                width: 800,
                height: 1000,
            },
            ReaderStyle::default(),
            Arc::default(),
        )
        .unwrap();
        let mut reader = EngineReader::new(session);

        let href = PublicationUrl::parse("chapter1.xhtml#missing").unwrap();
        assert!(reader.go_to_href(&href).is_ok());
        let anchor = SourceAnchor {
            spine: SpineItemId::new("chapter1").unwrap(),
            node: "missing-node".into(),
            text_offset: 0,
        };
        assert!(matches!(
            reader.go_to_source(&anchor),
            Err(ReaderError::NavigationTargetNotFound(_))
        ));
    }
    #[test]
    fn test_book_source_and_reader_lifetime() {
        let source = Arc::new(InMemorySource::new());
        let weak_source = Arc::downgrade(&source);

        {
            let session = ReaderSession::open_with_fonts(
                source,
                LayoutViewport {
                    width: 800,
                    height: 1000,
                },
                ReaderStyle::default(),
                Arc::default(),
            )
            .unwrap();
            let reader = EngineReader::new(session);
            assert_eq!(reader.book().metadata.title, "Test Book");
            assert!(weak_source.upgrade().is_some());
        }

        // Reader and session dropped, weak source count should be zero
        assert!(weak_source.upgrade().is_none());
    }

    #[test]
    fn test_prepared_reader_frame() {
        let source: Arc<dyn BookSource> = Arc::new(InMemorySource::new());
        let session = ReaderSession::open_with_fonts(
            source,
            LayoutViewport {
                width: 800,
                height: 1000,
            },
            ReaderStyle::default(),
            Arc::default(),
        )
        .unwrap();
        let mut reader = EngineReader::new(session);
        let frame = reader.frame().unwrap();

        assert_eq!(frame.viewport.width, 800);
        assert_eq!(frame.viewport.height, 1000);
        assert_eq!(frame.key.width, 800);
        assert_eq!(frame.key.height, 1000);
        assert_eq!(frame.transition, FrameTransition::None);
        assert_eq!(frame.transition_kind, transition::TransitionKind::Curl);
        assert!(!frame.is_transitioning());
        assert!(!frame.requires_next_frame);
    }

    #[test]
    fn interactive_curl_commits_only_after_settle_finishes() {
        let source: Arc<dyn BookSource> = Arc::new(InMemorySource::new());
        let session = ReaderSession::open_with_fonts(
            source,
            LayoutViewport {
                width: 320,
                height: 240,
            },
            ReaderStyle::default(),
            Arc::default(),
        )
        .unwrap();
        let mut reader = EngineReader::new(session);
        let initial = reader.current_position();

        reader.pointer_down(1, 290.0, 100.0, 0.0);
        assert_eq!(
            reader.pointer_move(1, 80.0, 102.0, 120.0).unwrap(),
            PointerGestureResult::Claimed
        );
        let drag_frame = reader.frame().unwrap();
        assert!(matches!(
            drag_frame.transition,
            FrameTransition::Curl {
                direction: PageDirection::Next,
                progress,
                ..
            } if progress > 0.5
        ));
        assert_eq!(reader.current_position(), initial);

        assert_eq!(
            reader.pointer_up(1, 40.0, 102.0, 160.0).unwrap(),
            PointerGestureResult::Claimed
        );
        let mut moved = false;
        for step in 1..=80 {
            let _ = reader.tick(std::time::Duration::from_millis(10));
            if reader.animation_step(160.0 + f64::from(step * 16)).unwrap()
                == EngineAnimationState::Moved
            {
                moved = true;
                break;
            }
        }
        assert!(moved);
        assert_ne!(reader.current_position(), initial);
        assert!(matches!(
            reader.frame().unwrap().transition,
            FrameTransition::None
        ));
    }

    #[test]
    fn test_search_and_highlights() {
        let mut source_inner = InMemorySource::new();
        let spine_id = source_inner.book.sections[0].id.clone();
        source_inner.section.blocks = vec![Block::Text(TextBlock {
            kind: TextBlockKind::Paragraph,
            source: Some(SourceRange {
                start: SourceAnchor {
                    spine: spine_id.clone(),
                    node: String::new(),
                    text_offset: 0,
                },
                end: SourceAnchor {
                    spine: spine_id,
                    node: String::new(),
                    text_offset: 44,
                },
            }),
            content: vec![Inline::Text(TextRun {
                text: "The quick brown fox jumps over the lazy dog.".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: BlockStyle::default(),
        })];

        let source = Arc::new(source_inner);
        let session = ReaderSession::open_with_fonts(
            source,
            LayoutViewport {
                width: 300,
                height: 400,
            },
            ReaderStyle::default(),
            Arc::default(),
        )
        .unwrap();
        let mut reader = EngineReader::new(session);

        let results = reader.search("brown fox", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_text, "brown fox");
        assert_eq!(results[0].section_index, 0);

        let range = results[0].range.clone();
        reader.set_highlights(vec![range.clone()]);
        assert_eq!(reader.highlights(), &[range.clone()]);

        let frame = reader.frame().unwrap();
        assert_eq!(frame.overlays.highlights, vec![range]);

        reader.clear_highlights();
        assert!(reader.highlights().is_empty());
    }
}
