    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use rebook_layout::{ReaderDefaultFont, ReaderTypesetting, SpreadMode};
    use rebook_publication::{
        Block, BlockStyle, FixedPageDimensions, ImageBlock, ImageStyle, Inline, Metadata,
        NOTE_SECTION_PROPERTY, PublicationId, PublicationUrl, RasterResource, Resource, Section,
        SectionAnchor, SourceAnchor, SourceRange, SpineItem, SpineItemId, TextBlock, TextBlockKind,
        TextRun, TextStyle, TocEntry,
    };

    use super::*;

    struct CountingSource {
        book: Book,
        sections: Vec<Section>,
        parse_counts: Vec<AtomicUsize>,
        background_delay: Duration,
    }

    struct LazyFixedSource {
        book: Book,
        parse_counts: Vec<AtomicUsize>,
        raster_counts: Vec<AtomicUsize>,
    }

    impl LazyFixedSource {
        fn new(section_count: usize) -> Arc<Self> {
            let sections = (0..section_count)
                .map(|index| SpineItem {
                    id: SpineItemId::new(format!("page-{index}")).unwrap(),
                    href: PublicationUrl::parse(&format!("page-{index}.xhtml")).unwrap(),
                    media_type: "application/xhtml+xml".into(),
                    linear: true,
                    properties: Vec::new(),
                })
                .collect::<Vec<_>>();
            let first = sections[0].href.clone();
            Arc::new(Self {
                book: Book {
                    id: PublicationId::new("lazy-fixed-reader-test").unwrap(),
                    metadata: Metadata {
                        title: "Lazy fixed pages".into(),
                        authors: Vec::new(),
                        languages: Vec::new(),
                        layout: RenditionLayout::PrePaginated,
                    },
                    sections,
                    table_of_contents: vec![TocEntry {
                        label: "Whole chapter".into(),
                        href: Some(first),
                        children: Vec::new(),
                    }],
                    cover: None,
                },
                parse_counts: (0..section_count).map(|_| AtomicUsize::new(0)).collect(),
                raster_counts: (0..section_count).map(|_| AtomicUsize::new(0)).collect(),
            })
        }

        fn href(index: usize) -> PublicationUrl {
            PublicationUrl::parse(&format!("images/page-{index}.rgba")).unwrap()
        }
    }

    impl BookSource for LazyFixedSource {
        fn book(&self) -> &Book {
            &self.book
        }

        fn parse_section(&self, index: usize) -> Result<Section, PublicationError> {
            self.parse_counts[index].fetch_add(1, Ordering::Relaxed);
            let descriptor = &self.book.sections[index];
            Ok(Section {
                id: descriptor.id.clone(),
                href: descriptor.href.clone(),
                blocks: vec![Block::Image(ImageBlock {
                    href: Self::href(index),
                    alt: format!("Page {}", index + 1),
                    style: ImageStyle::default(),
                    source: None,
                    text_layer: None,
                })],
                anchors: Vec::new(),
            })
        }

        fn resource(&self, href: &PublicationUrl) -> Result<Resource, PublicationError> {
            Err(PublicationError::ResourceNotFound(href.to_string()))
        }

        fn raster_resource(
            &self,
            href: &PublicationUrl,
        ) -> Result<Option<RasterResource>, PublicationError> {
            let index = href
                .path()
                .strip_prefix("images/page-")
                .and_then(|value| value.strip_suffix(".rgba"))
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or_else(|| PublicationError::ResourceNotFound(href.to_string()))?;
            self.raster_counts[index].fetch_add(1, Ordering::Relaxed);
            Ok(Some(RasterResource {
                width: 2,
                height: 3,
                pixels: vec![255; 2 * 3 * 4].into(),
            }))
        }

        fn fixed_page_dimensions(
            &self,
            section_index: usize,
        ) -> Result<Option<FixedPageDimensions>, PublicationError> {
            self.book
                .sections
                .get(section_index)
                .ok_or(PublicationError::ResourceNotFound(format!(
                    "section {section_index}"
                )))?;
            Ok(Some(FixedPageDimensions {
                width: 2,
                height: 3,
            }))
        }
    }

    impl CountingSource {
        fn new(texts: &[String]) -> Arc<Self> {
            Self::with_background_delay(texts, Duration::ZERO)
        }

        fn with_background_delay(texts: &[String], background_delay: Duration) -> Arc<Self> {
            let mut descriptors = Vec::with_capacity(texts.len());
            let mut sections = Vec::with_capacity(texts.len());
            for (index, text) in texts.iter().enumerate() {
                let id = SpineItemId::new(format!("section-{index}")).unwrap();
                let href = PublicationUrl::parse(&format!("section-{index}.xhtml")).unwrap();
                let text_len = u64::try_from(text.chars().count()).unwrap();
                descriptors.push(SpineItem {
                    id: id.clone(),
                    href: href.clone(),
                    media_type: "application/xhtml+xml".into(),
                    linear: true,
                    properties: Vec::new(),
                });
                sections.push(Section {
                    id: id.clone(),
                    href,
                    blocks: vec![Block::Text(TextBlock {
                        kind: TextBlockKind::Paragraph,
                        content: vec![Inline::Text(TextRun {
                            text: text.clone(),
                            style: TextStyle::default(),
                            link: None,
                        })],
                        style: BlockStyle::default(),
                        source: Some(SourceRange {
                            start: SourceAnchor {
                                spine: id.clone(),
                                node: "paragraph-0".into(),
                                text_offset: 0,
                            },
                            end: SourceAnchor {
                                spine: id.clone(),
                                node: "paragraph-0".into(),
                                text_offset: text_len,
                            },
                        }),
                    })],
                    anchors: Vec::new(),
                });
            }

            Arc::new(Self {
                book: Book {
                    id: PublicationId::new("reader-test").unwrap(),
                    metadata: Metadata::default(),
                    cover: None,
                    sections: descriptors,
                    table_of_contents: Vec::new(),
                },
                parse_counts: (0..sections.len()).map(|_| AtomicUsize::new(0)).collect(),
                sections,
                background_delay,
            })
        }

        fn parse_count(&self, index: usize) -> usize {
            self.parse_counts[index].load(Ordering::Relaxed)
        }
    }

    impl BookSource for CountingSource {
        fn book(&self) -> &Book {
            &self.book
        }

        fn parse_section(&self, index: usize) -> Result<Section, PublicationError> {
            if index > 0 {
                thread::sleep(self.background_delay);
            }
            let section =
                self.sections.get(index).cloned().ok_or_else(|| {
                    PublicationError::ResourceNotFound(format!("section {index}"))
                })?;
            self.parse_counts[index].fetch_add(1, Ordering::Relaxed);
            Ok(section)
        }

        fn resource(&self, href: &PublicationUrl) -> Result<Resource, PublicationError> {
            Err(PublicationError::ResourceNotFound(href.to_string()))
        }
    }

    struct SwitchingSource {
        original_book: Book,
        original_sections: Vec<Section>,
        derived_book: Book,
        derived_sections: Vec<Section>,
        derived: AtomicBool,
    }

    impl SwitchingSource {
        fn new(original: &Arc<CountingSource>, derived: &Arc<CountingSource>) -> Arc<Self> {
            Arc::new(Self {
                original_book: original.book.clone(),
                original_sections: original.sections.clone(),
                derived_book: derived.book.clone(),
                derived_sections: derived.sections.clone(),
                derived: AtomicBool::new(false),
            })
        }

        fn set_derived(&self, derived: bool) {
            self.derived.store(derived, Ordering::Release);
        }

        fn active(&self) -> (&Book, &[Section]) {
            if self.derived.load(Ordering::Acquire) {
                (&self.derived_book, &self.derived_sections)
            } else {
                (&self.original_book, &self.original_sections)
            }
        }
    }

    impl BookSource for SwitchingSource {
        fn book(&self) -> &Book {
            self.active().0
        }

        fn parse_section(&self, index: usize) -> Result<Section, PublicationError> {
            self.active()
                .1
                .get(index)
                .cloned()
                .ok_or_else(|| PublicationError::ResourceNotFound(format!("section {index}")))
        }

        fn resource(&self, href: &PublicationUrl) -> Result<Resource, PublicationError> {
            Err(PublicationError::ResourceNotFound(href.to_string()))
        }
    }

    fn viewport(width: u32, height: u32) -> LayoutViewport {
        LayoutViewport::new(width, height).unwrap()
    }

    fn image_page(image_x: f32) -> PageDisplayList {
        DisplayListCompiler.compile(&rebook_layout::PageLayout {
            viewport: viewport(1_200, 700),
            background: rebook_publication::Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![rebook_layout::PageItem::Image(
                rebook_layout::ImagePlacement {
                    image: rebook_layout::RasterImage {
                        width: 400,
                        height: 600,
                        pixels: vec![255; 400 * 600 * 4].into(),
                    },
                    x: image_x,
                    y: 0.0,
                    width: 400.0,
                    height: 600.0,
                    source: None,
                    text_layer: None,
                    replacement: None,
                },
            )],
        })
    }

    #[test]
    fn compact_image_spread_touches_and_centers_page_edges() {
        let primary = image_page(150.0);
        let secondary = image_page(150.0);
        let (primary_offset, secondary_offset) =
            resolve_spread_offsets(&primary, Some(&secondary), 600.0, true);
        let primary_bounds = primary.image_bounds().unwrap();
        let secondary_bounds = secondary.image_bounds().unwrap();
        let primary_left = primary_bounds.x0 + f64::from(primary_offset);
        let primary_right = primary_bounds.x1 + f64::from(primary_offset);
        let secondary_left = secondary_bounds.x0 + f64::from(secondary_offset);
        let secondary_right = secondary_bounds.x1 + f64::from(secondary_offset);

        assert!((primary_right - secondary_left).abs() < f64::EPSILON);
        assert!((f64::midpoint(primary_left, secondary_right) - 600.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cached_page_turns_and_boundaries_do_not_reparse() {
        let source = CountingSource::new(&["缓存翻页测试。".repeat(600)]);
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        assert!(reader.location().page_count > 2);
        assert_eq!(source.parse_count(0), 1);

        assert!(matches!(
            reader.turn_page(PageDirection::Previous).unwrap().outcome,
            NavigationOutcome::Boundary
        ));
        let mut moved = 0;
        loop {
            let result = reader.turn_page(PageDirection::Next).unwrap();
            if result.outcome == NavigationOutcome::Boundary {
                break;
            }
            moved += 1;
            assert!(moved < 10_000);
        }
        assert!(moved > 2);
        assert_eq!(source.parse_count(0), 1);
    }

    #[test]
    fn unified_typesetting_omits_whole_note_sections_from_toc_and_navigation() {
        let source = CountingSource::new(&[
            "chapter before notes".into(),
            "authored note definitions".into(),
            "back matter after notes".into(),
        ]);
        let mut source = Arc::try_unwrap(source)
            .ok()
            .expect("new counting source has one owner");
        source.book.sections[1]
            .properties
            .push(NOTE_SECTION_PROPERTY.into());
        source.book.table_of_contents = source
            .book
            .sections
            .iter()
            .enumerate()
            .map(|(index, section)| TocEntry {
                label: ["Chapter", "Notes", "Bibliography"][index].into(),
                href: Some(section.href.clone()),
                children: Vec::new(),
            })
            .collect();
        let notes_href = source.book.sections[1].href.clone();
        let source = Arc::new(source);

        let mut reader = ReaderSession::open(
            source,
            viewport(600, 400),
            ReaderStyle {
                typesetting: ReaderTypesetting::unified(),
                ..ReaderStyle::default()
            },
        )
        .unwrap();

        assert_eq!(
            reader
                .toc_items()
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            ["Chapter", "Bibliography"]
        );
        assert_eq!(
            reader
                .go_to_section(1)
                .unwrap()
                .snapshot
                .location
                .section_index,
            2
        );
        assert_eq!(
            reader
                .go_to_href(&notes_href)
                .unwrap()
                .snapshot
                .location
                .section_index,
            2
        );
        assert_eq!(
            reader
                .go_to_adjacent_reading_unit(PageDirection::Previous)
                .unwrap()
                .snapshot
                .location
                .section_index,
            0
        );
    }

    #[test]
    fn switching_to_unified_typesetting_relocates_an_open_note_section() {
        let source = CountingSource::new(&[
            "chapter before notes".into(),
            "authored note definitions".into(),
            "back matter after notes".into(),
        ]);
        let mut source = Arc::try_unwrap(source)
            .ok()
            .expect("new counting source has one owner");
        source.book.sections[1]
            .properties
            .push(NOTE_SECTION_PROPERTY.into());
        let mut reader =
            ReaderSession::open(Arc::new(source), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        reader.go_to_section(1).unwrap();

        let snapshot = reader
            .set_style(ReaderStyle {
                typesetting: ReaderTypesetting::unified(),
                ..ReaderStyle::default()
            })
            .unwrap();

        assert_eq!(snapshot.location.section_index, 2);
        assert!(reader.current_page().content_top().is_some());
    }

    #[test]
    fn continuous_section_pages_cover_every_segment_and_update_visible_position() {
        let source = CountingSource::new(&["连续章节滑动测试。".repeat(1_500)]);
        let mut reader = ReaderSession::open(
            source.clone(),
            viewport(600, 400),
            ReaderStyle {
                spread: SpreadMode::Scroll,
                ..ReaderStyle::default()
            },
        )
        .unwrap();
        let initial = reader.location();
        let pages = reader.current_section_pages().unwrap();

        assert!(pages.len() >= initial.page_count);
        assert!(pages.len() > 1);
        assert_eq!(
            pages
                .iter()
                .map(|entry| entry.position.segment_index)
                .collect::<HashSet<_>>()
                .len(),
            initial.segment_count,
        );
        assert!(pages.windows(2).all(|pair| {
            let left = pair[0].position;
            let right = pair[1].position;
            (left.section_index, left.segment_index, left.page_index)
                < (right.section_index, right.segment_index, right.page_index)
        }));
        assert_eq!(source.parse_count(0), 1);

        let last = pages.last().unwrap().position;
        let snapshot = reader.set_visible_position(last).unwrap();
        assert_eq!(snapshot.location.section_index, last.section_index);
        assert_eq!(snapshot.location.segment_index, last.segment_index);
        assert_eq!(snapshot.location.page_index, last.page_index);
    }

    #[test]
    fn native_selection_round_trips_to_source_ranges_and_geometry() {
        let source = CountingSource::new(&["选择文字行为".into()]);
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        let selected_source = SourceRange {
            start: SourceAnchor {
                spine: SpineItemId::new("section-0").unwrap(),
                node: "paragraph-0".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine: SpineItemId::new("section-0").unwrap(),
                node: "paragraph-0".into(),
                text_offset: 4,
            },
        };
        let rect = reader
            .current_page()
            .source_rects(std::slice::from_ref(&selected_source))[0];
        let first_character = SourceRange {
            start: selected_source.start.clone(),
            end: SourceAnchor {
                text_offset: 1,
                ..selected_source.start.clone()
            },
        };
        let last_character = SourceRange {
            start: SourceAnchor {
                text_offset: 3,
                ..selected_source.start.clone()
            },
            end: selected_source.end.clone(),
        };
        let first_rect = reader
            .current_page()
            .source_rects(std::slice::from_ref(&first_character))[0];
        let last_rect = reader
            .current_page()
            .source_rects(std::slice::from_ref(&last_character))[0];
        let y = logical_coordinate(rect.center().y);
        let anchor = reader
            .hit_test_current_spread(logical_coordinate(first_rect.x1) - 0.1, y, true)
            .unwrap()
            .unwrap();
        let focus = reader
            .hit_test_current_spread(logical_coordinate(rect.x1) - 0.1, y, true)
            .unwrap()
            .unwrap();
        let selection = reader.selection_between(&anchor, &focus).unwrap().unwrap();

        assert_eq!(selection.text, "选择文字");
        assert!(!selection.ranges.is_empty());
        assert!(!selection.rects.is_empty());
        assert!(
            reader
                .source_ranges_contain_point(
                    &selection.ranges,
                    selection.rects[0].x + selection.rects[0].width / 2.0,
                    selection.rects[0].y + selection.rects[0].height / 2.0,
                )
                .unwrap()
        );

        let reverse_anchor = reader
            .hit_test_current_spread(logical_coordinate(last_rect.x0) + 0.1, y, true)
            .unwrap()
            .unwrap();
        let reverse_focus = reader
            .hit_test_current_spread(logical_coordinate(rect.x0) + 0.1, y, true)
            .unwrap()
            .unwrap();
        let reverse_selection = reader
            .selection_between(&reverse_anchor, &reverse_focus)
            .unwrap()
            .unwrap();
        assert_eq!(reverse_selection.text, "选择文字");
    }

    #[test]
    fn semantic_selection_expands_to_word_sentence_and_paragraph_boundaries() {
        let text = "Hello, world! 下一句。";
        let word_start = text.find("world").unwrap();
        let hit = ReaderTextHit {
            position: ReaderPosition {
                section_index: 0,
                segment_index: 0,
                page_index: 0,
            },
            region_index: 0,
            byte_index: word_start + 2,
            cluster_start: word_start + 1,
            cluster_end: word_start + 2,
        };

        assert_eq!(
            semantic_byte_range(text, 0..text.len(), &hit, SelectionGranularity::Word, "en"),
            Some(word_start..word_start + "world".len())
        );
        assert_eq!(
            semantic_byte_range(
                text,
                0..text.len(),
                &hit,
                SelectionGranularity::Sentence,
                "en",
            ),
            Some(0.."Hello, world!".len())
        );
        assert_eq!(
            semantic_byte_range(
                text,
                0..text.len(),
                &hit,
                SelectionGranularity::Paragraph,
                "en",
            ),
            Some(0..text.len())
        );
    }

    #[test]
    fn sentence_selection_keeps_a_leading_quoted_term_with_the_next_sentence() {
        let text = "本书需要作一些说明。“现代”一词很简单。然后继续。";
        let modern = text.find("现代").unwrap();
        let hit = ReaderTextHit {
            position: ReaderPosition {
                section_index: 0,
                segment_index: 0,
                page_index: 0,
            },
            region_index: 0,
            byte_index: modern,
            cluster_start: modern,
            cluster_end: modern + "现".len(),
        };

        let range = semantic_byte_range(
            text,
            0..text.len(),
            &hit,
            SelectionGranularity::Sentence,
            "en",
        )
        .unwrap();

        assert_eq!(&text[range], "“现代”一词很简单。");
        assert_eq!(
            sentence_byte_ranges_with_language(text, "en")
                .into_iter()
                .map(|range| &text[range])
                .collect::<String>(),
            text
        );
    }

    #[test]
    fn word_boundary_extension_includes_only_terminal_punctuation() {
        let sentence = "Alpha beta! Gamma.";
        let beta = sentence.find("beta").unwrap();
        assert_eq!(
            extend_word_to_sentence_or_paragraph_end(
                sentence,
                0..sentence.len(),
                beta..beta + "beta".len(),
                "en",
            ),
            beta..beta + "beta!".len()
        );

        let middle = "Alpha beta, gamma.";
        let beta = middle.find("beta").unwrap();
        assert_eq!(
            extend_word_to_sentence_or_paragraph_end(
                middle,
                0..middle.len(),
                beta..beta + "beta".len(),
                "en",
            ),
            beta..beta + "beta".len()
        );

        let quoted = "内容。” 下一句。";
        assert_eq!(
            extend_word_to_sentence_or_paragraph_end(
                quoted,
                0..quoted.len(),
                0.."内容".len(),
                "zh",
            ),
            0.."内容。”".len()
        );
    }

    #[test]
    fn dragging_words_to_sentence_end_includes_punctuation_but_clicking_does_not() {
        let text = "Alpha beta! Gamma delta.";
        let source = CountingSource::new(&[text.into()]);
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        let source_range = |start, end| SourceRange {
            start: SourceAnchor {
                spine: SpineItemId::new("section-0").unwrap(),
                node: "paragraph-0".into(),
                text_offset: start,
            },
            end: SourceAnchor {
                spine: SpineItemId::new("section-0").unwrap(),
                node: "paragraph-0".into(),
                text_offset: end,
            },
        };
        let hit_for = |reader: &mut ReaderSession, range: SourceRange| {
            let rect = reader.current_page().source_rects(&[range])[0];
            reader
                .hit_test_current_spread(
                    logical_coordinate(rect.center().x),
                    logical_coordinate(rect.center().y),
                    true,
                )
                .unwrap()
                .unwrap()
        };
        let alpha = hit_for(&mut reader, source_range(0, 1));
        let beta = hit_for(&mut reader, source_range(7, 8));

        let dragged = reader
            .selection_between_with_granularity(&alpha, &beta, SelectionGranularity::Word)
            .unwrap()
            .unwrap();
        assert_eq!(dragged.text, "Alpha beta!");

        let clicked = reader
            .selection_between_with_granularity(&beta, &beta, SelectionGranularity::Word)
            .unwrap()
            .unwrap();
        assert_eq!(clicked.text, "beta");
    }

    #[test]
    fn semantic_selection_on_the_right_page_keeps_its_spread_offset() {
        let source = CountingSource::new(&["word ".repeat(FRAGMENT_TEXT_BUDGET)]);
        let mut reader = ReaderSession::open(
            source,
            viewport(1_200, 700),
            ReaderStyle {
                spread: SpreadMode::Double,
                ..ReaderStyle::default()
            },
        )
        .unwrap();
        let spread = reader.current_spread().unwrap();
        let secondary = spread.secondary.unwrap();
        let offset_x = spread.secondary_offset_x;
        let leading = secondary.leading_source_range().unwrap();
        let target = secondary
            .source_rects(std::slice::from_ref(&leading))
            .into_iter()
            .next()
            .unwrap();
        let hit = reader
            .hit_test_current_spread(
                logical_coordinate(target.center().x) + offset_x,
                logical_coordinate(target.center().y),
                true,
            )
            .unwrap()
            .unwrap();

        let selection = reader
            .selection_between_with_granularity(&hit, &hit, SelectionGranularity::Word)
            .unwrap()
            .unwrap();

        assert!(
            selection
                .rects
                .iter()
                .all(|rect| rect.position == hit.position)
        );
        assert!(selection.rects.iter().all(|rect| rect.x >= offset_x));

        let drag_anchor = reader
            .hit_test_current_spread(
                logical_coordinate(target.x0) + offset_x + 1.0,
                logical_coordinate(target.center().y),
                false,
            )
            .unwrap()
            .unwrap();
        let drag_focus = reader
            .hit_test_current_spread(
                logical_coordinate(target.x1) + offset_x - 1.0,
                logical_coordinate(target.center().y),
                false,
            )
            .unwrap()
            .unwrap();
        let dragged = reader
            .selection_between(&drag_anchor, &drag_focus)
            .unwrap()
            .unwrap();

        assert_eq!(drag_anchor.position, drag_focus.position);
        assert!(dragged.rects.iter().all(|rect| rect.x >= offset_x));
    }

    #[test]
    fn paragraph_selection_covers_continuations_across_logical_pages() {
        let text = "alpha beta gamma delta. ".repeat(800);
        let source = CountingSource::new(std::slice::from_ref(&text));
        let style = ReaderStyle {
            spread: SpreadMode::Single,
            ..ReaderStyle::default()
        };
        let mut reader = ReaderSession::open(source, viewport(320, 180), style).unwrap();
        let first_character = SourceRange {
            start: SourceAnchor {
                spine: SpineItemId::new("section-0").unwrap(),
                node: "paragraph-0".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine: SpineItemId::new("section-0").unwrap(),
                node: "paragraph-0".into(),
                text_offset: 1,
            },
        };
        let rect = reader
            .current_page()
            .source_rects(std::slice::from_ref(&first_character))[0];
        let hit = reader
            .hit_test_current_spread(
                logical_coordinate(rect.center().x),
                logical_coordinate(rect.center().y),
                true,
            )
            .unwrap()
            .unwrap();

        let selection = reader
            .selection_between_with_granularity(&hit, &hit, SelectionGranularity::Paragraph)
            .unwrap()
            .unwrap();

        assert_eq!(selection.ranges.first().unwrap().start.text_offset, 0);
        assert_eq!(
            selection.ranges.last().unwrap().end.text_offset,
            u64::try_from(text.chars().count()).unwrap()
        );
        assert_eq!(selection.text, text);
        assert_ne!(
            selection.rects.first().unwrap().position,
            selection.rects.last().unwrap().position
        );
    }

    #[test]
    fn visible_text_fragments_follow_the_current_page() {
        let source = CountingSource::new(&["visible page text ".repeat(1_200)]);
        let style = ReaderStyle {
            spread: SpreadMode::Single,
            ..ReaderStyle::default()
        };
        let mut reader = ReaderSession::open(source, viewport(600, 400), style).unwrap();

        let first = reader.current_visible_text_fragments().unwrap();
        assert!(!first.is_empty());
        assert!(first.iter().all(|fragment| {
            fragment.position
                == ReaderPosition {
                    section_index: reader.location().section_index,
                    segment_index: reader.location().segment_index,
                    page_index: reader.location().page_index,
                }
        }));
        let first_ranges = first
            .iter()
            .map(|fragment| fragment.range.clone())
            .collect::<Vec<_>>();
        let first_position = first[0].position;
        assert_eq!(
            reader
                .visible_text_fragments_for_pages(&[first_position])
                .unwrap(),
            first
        );

        assert_eq!(
            reader.turn_page(PageDirection::Next).unwrap().outcome,
            NavigationOutcome::Moved
        );
        let second = reader.current_visible_text_fragments().unwrap();
        assert!(!second.is_empty());
        assert!(second.iter().all(|fragment| {
            fragment.position
                == ReaderPosition {
                    section_index: reader.location().section_index,
                    segment_index: reader.location().segment_index,
                    page_index: reader.location().page_index,
                }
        }));
        assert_ne!(
            first_ranges,
            second
                .iter()
                .map(|fragment| fragment.range.clone())
                .collect::<Vec<_>>()
        );
        let combined = reader
            .visible_text_fragments_for_pages(&[first_position, second[0].position])
            .unwrap();
        assert!(
            combined
                .iter()
                .any(|fragment| fragment.position == first_position)
        );
        assert!(
            combined
                .iter()
                .any(|fragment| fragment.position == second[0].position)
        );
    }

    #[test]
    fn cached_visible_text_fragments_skip_pages_that_are_not_compiled_yet() {
        let source =
            CountingSource::new(&["visible page text".into(), "not compiled page text".into()]);
        let mut reader = ReaderSession::open(
            source,
            viewport(600, 400),
            ReaderStyle {
                spread: SpreadMode::Single,
                ..ReaderStyle::default()
            },
        )
        .unwrap();
        let visible = reader.current_visible_text_fragments().unwrap();
        assert!(!visible.is_empty());
        let visible_position = visible[0].position;
        let pending_position = ReaderPosition {
            section_index: 1,
            segment_index: 0,
            page_index: 0,
        };

        assert_eq!(
            reader.cached_visible_text_fragments_for_pages(&[visible_position, pending_position,]),
            visible
        );
        assert!(!reader.cache.contains_key(&SegmentKey {
            section_index: 1,
            segment_index: 0,
        }));
    }

    #[test]
    fn durable_source_navigation_resolves_the_page_after_pagination() {
        let source = CountingSource::new(&["navigation target ".repeat(900)]);
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        let anchor = SourceAnchor {
            spine: SpineItemId::new("section-0").unwrap(),
            node: "paragraph-0".into(),
            text_offset: 8_000,
        };

        reader.go_to_source(&anchor).unwrap();
        assert!(reader.location().page_index > 0);
        assert!(reader.current_page().contains_source_anchor(&anchor));
    }

    #[test]
    fn durable_locator_restores_after_viewport_repagination() {
        let source = CountingSource::new(&["durable locator ".repeat(1_200)]);
        let mut first =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        first
            .go_to_source(&SourceAnchor {
                spine: SpineItemId::new("section-0").unwrap(),
                node: "paragraph-0".into(),
                text_offset: 9_000,
            })
            .unwrap();
        let locator = first.current_locator();
        let source_anchor = locator.source.as_ref().unwrap().start.clone();
        let quote = locator.text.as_ref().expect("text locator quote");
        assert!(!quote.highlight.is_empty());
        assert!(quote.before.chars().count() <= LOCATOR_QUOTE_BEFORE_CHARS);
        assert!(quote.highlight.chars().count() <= LOCATOR_QUOTE_HIGHLIGHT_CHARS);
        assert!(quote.after.chars().count() <= LOCATOR_QUOTE_AFTER_CHARS);

        let mut restored =
            ReaderSession::open(source, viewport(820, 620), ReaderStyle::default()).unwrap();
        restored.restore_locator(&locator).unwrap();

        assert!(
            restored
                .current_page()
                .contains_source_anchor(&source_anchor)
        );
        assert_eq!(
            restored.current_locator().publication_id,
            locator.publication_id
        );
    }

    #[test]
    fn opening_at_a_locator_skips_the_unneeded_first_section() {
        let source =
            CountingSource::new(&["first section".repeat(200), "resumed section".repeat(200)]);
        let locator = LocatorV1 {
            version: LocatorV1::VERSION,
            publication_id: source.book.id.clone(),
            href: source.book.sections[1].href.clone(),
            progression: Some(0.0),
            total_progression: Some(0.5),
            position: None,
            source: None,
            partial_cfi: None,
            text: None,
        };

        let reader = ReaderSession::open_with_fonts_at_locator(
            source.clone(),
            viewport(600, 400),
            ReaderStyle::default(),
            Arc::default(),
            &locator,
        )
        .unwrap();

        assert_eq!(reader.location().section_index, 1);
        assert_eq!(source.parse_count(0), 0);
        assert_eq!(source.parse_count(1), 1);
    }

    #[test]
    fn oversized_text_block_is_split_into_stable_source_ranged_fragments() {
        let source = CountingSource::new(&["a".repeat(FRAGMENT_TEXT_BUDGET * 2 + 17)]);
        let mut section = source.sections[0].clone();
        let spine = section.id.clone();
        section.blocks[0] = Block::Text(TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: "a".repeat(FRAGMENT_TEXT_BUDGET * 2 + 17),
                style: TextStyle::default(),
                link: None,
            })],
            style: BlockStyle::default(),
            source: Some(SourceRange {
                start: SourceAnchor {
                    spine: spine.clone(),
                    node: "n0".into(),
                    text_offset: 0,
                },
                end: SourceAnchor {
                    spine,
                    node: "n0".into(),
                    text_offset: u64::try_from(FRAGMENT_TEXT_BUDGET * 2 + 17).unwrap(),
                },
            }),
        });

        let prepared = prepare_section(section, &HashSet::new(), &[]);

        assert_eq!(prepared.fragments.len(), 3);
        assert_eq!(prepared.segments.len(), 1);
        assert_eq!(block_text_len(&prepared.fragments[0].blocks[0]), 4_096);
        assert_eq!(block_text_len(&prepared.fragments[1].blocks[0]), 4_096);
        assert_eq!(block_text_len(&prepared.fragments[2].blocks[0]), 17);
        let ranges = prepared
            .fragments
            .iter()
            .map(|fragment| block_source(&fragment.blocks[0]).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ranges[0].start.text_offset, 0);
        assert_eq!(ranges[0].end.text_offset, 4_096);
        assert_eq!(ranges[1].start.text_offset, 4_096);
        assert_eq!(ranges[1].end.text_offset, 8_192);
        assert_eq!(ranges[2].start.text_offset, 8_192);
        assert_eq!(ranges[2].end.text_offset, 8_209);
    }

    #[test]
    fn content_fragment_boundaries_never_commit_partial_pages() {
        let text_len = FRAGMENT_TEXT_BUDGET * 3 + 100;
        let source = CountingSource::new(&["a".repeat(text_len)]);
        let mut section = source.sections[0].clone();
        let spine = section.id.clone();
        let Block::Text(block) = &mut section.blocks[0] else {
            unreachable!();
        };
        block.source = Some(SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "n0".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "n0".into(),
                text_offset: u64::try_from(text_len).unwrap(),
            },
        });
        let prepared = prepare_section(section, &HashSet::new(), &[]);
        let segment = &prepared.segments[0];
        let fragments = prepared.fragments[segment.fragment_range.clone()]
            .iter()
            .map(|fragment| fragment.blocks.as_slice())
            .collect::<Vec<_>>();

        let layout = LayoutEngine::new()
            .layout_fragments(
                source.as_ref(),
                &fragments,
                viewport(600, 50_000),
                &ReaderStyle::default(),
            )
            .unwrap();

        assert_eq!(prepared.fragments.len(), 4);
        assert_eq!(prepared.segments.len(), 1);
        assert_eq!(layout.pages.len(), 1);
        let ranges = layout.pages[0]
            .items
            .iter()
            .filter_map(|item| match item {
                PageItem::Text(placement) => placement.source.as_ref(),
                PageItem::Image(placement) => placement.source.as_ref(),
                PageItem::Quote(_) | PageItem::Table(_) | PageItem::Separator(_) => None,
            })
            .collect::<Vec<_>>();
        assert!(
            ranges
                .iter()
                .any(|range| range.end.text_offset == u64::try_from(FRAGMENT_TEXT_BUDGET).unwrap())
        );
        assert!(ranges.iter().any(|range| {
            range.start.text_offset == u64::try_from(FRAGMENT_TEXT_BUDGET).unwrap()
        }));
        assert!(
            ranges
                .iter()
                .any(|range| { range.end.text_offset == u64::try_from(text_len).unwrap() })
        );
    }

    #[test]
    fn continued_list_item_does_not_repeat_its_marker() {
        let parts = split_text_block(TextBlock {
            kind: TextBlockKind::ListItem {
                ordered: true,
                ordinal: 7,
                depth: 0,
                marker_visible: true,
            },
            content: vec![Inline::Text(TextRun {
                text: "item ".repeat(FRAGMENT_TEXT_BUDGET),
                style: TextStyle::default(),
                link: None,
            })],
            style: BlockStyle {
                indent: 24.0,
                ..BlockStyle::default()
            },
            source: None,
        });

        assert!(parts.len() > 1);
        assert!(matches!(
            parts[0].kind,
            TextBlockKind::ListItem {
                ordered: true,
                ordinal: 7,
                depth: 0,
                marker_visible: true,
            }
        ));
        assert!(
            parts[1..]
                .iter()
                .all(|part| part.kind == TextBlockKind::Paragraph)
        );
        assert!(parts[1..].iter().all(|part| part.style.indent == 0.0));
    }

    #[test]
    fn page_turns_across_content_fragments_do_not_reparse_the_authored_section() {
        let source = CountingSource::new(&["long text ".repeat(FRAGMENT_TEXT_BUDGET)]);
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        assert_eq!(reader.location().segment_count, 1);
        assert!(reader.location().page_count > 10);

        for _ in 0..10 {
            assert_eq!(
                reader.turn_page(PageDirection::Next).unwrap().outcome,
                NavigationOutcome::Moved
            );
        }
        assert_eq!(reader.location().segment_index, 0);
        assert_eq!(reader.location().page_index, 10);
        assert_eq!(source.parse_count(0), 1);

        assert_eq!(
            reader.turn_page(PageDirection::Previous).unwrap().outcome,
            NavigationOutcome::Moved
        );
        assert_eq!(reader.location().segment_index, 0);
        assert_eq!(reader.location().page_index, 9);
        assert_eq!(source.parse_count(0), 1);
    }

    #[test]
    fn double_spread_composes_adjacent_pages_from_one_continuous_section() {
        let source = CountingSource::new(&["long text ".repeat(FRAGMENT_TEXT_BUDGET)]);
        let mut reader = ReaderSession::open(
            source,
            viewport(1_200, 700),
            ReaderStyle {
                spread: rebook_layout::SpreadMode::Double,
                ..ReaderStyle::default()
            },
        )
        .unwrap();
        assert_eq!(reader.location().segment_count, 1);
        assert!(reader.current_page_count() > 2);
        reader.current_page = reader.current_page_count() - 2;

        let next = reader
            .next_position(reader.current_position())
            .unwrap()
            .expect("another logical page should follow");
        assert_eq!(next.section_index, 0);
        assert_eq!(next.segment_index, 0);
        let spread = reader.current_spread().unwrap();
        assert!(spread.secondary.is_some());
    }

    #[test]
    fn segment_window_prefetch_makes_short_section_switches_cache_only() {
        let source = CountingSource::new(&["第一章".into(), "第二章".into(), "第三章".into()]);
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();

        reader.prefetch_adjacent().unwrap();
        reader.wait_for_prefetch().unwrap();
        assert_eq!(reader.cached_segment_count(), 3);
        assert_eq!(source.parse_count(1), 1);
        assert_eq!(source.parse_count(2), 1);
        reader.turn_page(PageDirection::Next).unwrap();
        assert_eq!(reader.location().section_index, 1);
        assert_eq!(source.parse_count(1), 1);

        reader.prefetch_adjacent().unwrap();
        reader.wait_for_prefetch().unwrap();
        assert_eq!(reader.cached_segment_count(), 3);
        assert_eq!(source.parse_count(2), 1);
        reader.turn_page(PageDirection::Next).unwrap();
        assert_eq!(reader.location().section_index, 2);
        assert_eq!(source.parse_count(2), 1);
    }

    #[test]
    fn double_spread_composes_across_section_boundaries_without_repeating_pages() {
        let source = CountingSource::new(&[
            "left page".into(),
            "right page".into(),
            "next spread".into(),
        ]);
        let mut reader = ReaderSession::open(
            source.clone(),
            viewport(1_200, 700),
            ReaderStyle {
                spread: rebook_layout::SpreadMode::Double,
                ..ReaderStyle::default()
            },
        )
        .unwrap();

        let spread = reader.current_spread().unwrap();
        let secondary = spread.secondary.as_ref().unwrap();
        let secondary_offset_x = spread.secondary_offset_x;
        assert!(spread.primary.command_count() > 0);
        assert!(
            spread
                .secondary
                .as_ref()
                .is_some_and(|page| page.command_count() > 0)
        );
        assert_eq!(source.parse_count(1), 1);
        assert_eq!(reader.current_spread_section_indices().unwrap(), [0, 1]);

        let leading = secondary.leading_source_range().unwrap();
        let target = secondary
            .source_rects(std::slice::from_ref(&leading))
            .into_iter()
            .next()
            .unwrap();
        let hit = reader
            .hit_test_current_spread(
                logical_coordinate(target.x0) + secondary_offset_x + 1.0,
                logical_coordinate(target.center().y),
                true,
            )
            .unwrap()
            .unwrap();
        let selection = reader
            .selection_between_with_granularity(&hit, &hit, SelectionGranularity::Word)
            .unwrap()
            .unwrap();
        assert_eq!(selection.text, "right");
        assert!(
            selection
                .rects
                .iter()
                .all(|rect| rect.position.section_index == 1 && rect.x >= secondary_offset_x)
        );

        assert_eq!(
            reader.turn_page(PageDirection::Next).unwrap().outcome,
            NavigationOutcome::Moved
        );
        assert_eq!(reader.location().section_index, 2);
        assert_eq!(
            reader.turn_page(PageDirection::Previous).unwrap().outcome,
            NavigationOutcome::Moved
        );
        assert_eq!(reader.location().section_index, 0);
    }

    #[test]
    fn double_spread_prefetches_every_page_needed_by_the_next_spread() {
        let source = CountingSource::new(&[
            "page one".into(),
            "page two".into(),
            "page three".into(),
            "page four".into(),
            "page five".into(),
        ]);
        let mut reader = ReaderSession::open(
            source.clone(),
            viewport(1_200, 700),
            ReaderStyle {
                spread: rebook_layout::SpreadMode::Double,
                ..ReaderStyle::default()
            },
        )
        .unwrap();

        reader.prefetch_adjacent().unwrap();
        reader.wait_for_prefetch().unwrap();

        assert_eq!(source.parse_count(1), 1);
        assert_eq!(source.parse_count(2), 1);
        assert_eq!(source.parse_count(3), 1);
    }

    #[test]
    fn toc_href_navigation_resolves_segments_and_reuses_parsed_sections() {
        let mut source = CountingSource::new(&[
            "第一章".repeat(100),
            "第二章".repeat(100),
            "第三章".repeat(100),
        ]);
        let target_section = &mut Arc::get_mut(&mut source).unwrap().sections[1];
        let spine = target_section.id.clone();
        let source_range = |node: &str, length: u64| SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: node.to_owned(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine: spine.clone(),
                node: node.to_owned(),
                text_offset: length,
            },
        };
        target_section.blocks = vec![
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: "目标之前的长正文。".repeat(2_000),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: BlockStyle::default(),
                source: Some(source_range("n0", 18_000)),
            }),
            Block::Text(TextBlock {
                kind: TextBlockKind::Heading(2),
                content: vec![Inline::Text(TextRun {
                    text: "目录目标".to_owned(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: BlockStyle::default(),
                source: Some(source_range("n1", 4)),
            }),
        ];
        target_section.anchors = vec![SectionAnchor {
            fragment: "part-2".to_owned(),
            source: source_range("n1", 4).start,
        }];
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        reader.prefetch_adjacent().unwrap();
        reader.wait_for_prefetch().unwrap();

        let target = PublicationUrl::parse("section-1.xhtml#part-2").unwrap();
        assert_eq!(reader.section_index_for_href(&target), Some(1));
        let target_location = reader.position_for_href(&target).unwrap();
        assert_eq!(target_location.section_index, 1);
        assert_eq!(target_location.segment_index, 0);
        reader.go_to_href(&target).unwrap();
        let resolved_location = reader.position_for_href(&target).unwrap();

        assert_eq!(reader.location().section_index, 1);
        assert_eq!(
            reader.location().segment_index,
            target_location.segment_index
        );
        assert_eq!(reader.location().page_index, resolved_location.page_index);
        assert!(reader.location().page_index > 0);
        assert_eq!(source.parse_count(1), 1);
    }

    #[test]
    fn explicit_toc_navigation_keeps_the_clicked_item_active_on_shared_pages() {
        let mut source = CountingSource::new(&["Shared page content".into()]);
        let source_mut = Arc::get_mut(&mut source).unwrap();
        let spine = source_mut.sections[0].id.clone();
        let source_range = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "n0".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "n0".into(),
                text_offset: 19,
            },
        };
        source_mut.sections[0].anchors = vec![
            SectionAnchor {
                fragment: "first".into(),
                source: source_range.start.clone(),
            },
            SectionAnchor {
                fragment: "second".into(),
                source: SourceAnchor {
                    text_offset: 1,
                    ..source_range.start
                },
            },
        ];
        source_mut.book.table_of_contents = vec![
            TocEntry {
                label: "First".into(),
                href: Some(PublicationUrl::parse("section-0.xhtml#first").unwrap()),
                children: Vec::new(),
            },
            TocEntry {
                label: "Second".into(),
                href: Some(PublicationUrl::parse("section-0.xhtml#second").unwrap()),
                children: Vec::new(),
            },
        ];
        let second_anchor = source_mut.sections[0].anchors[1].source.clone();
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();

        assert_eq!(reader.snapshot().active_toc_id.as_deref(), Some("1"));
        let result = reader
            .go_to_href(&PublicationUrl::parse("section-0.xhtml#first").unwrap())
            .unwrap();

        assert_eq!(result.snapshot.active_toc_id.as_deref(), Some("0"));
        assert_eq!(
            reader
                .source_anchor_for_href(&PublicationUrl::parse("section-0.xhtml#second").unwrap())
                .as_ref(),
            Some(&second_anchor)
        );
    }

    #[test]
    fn distant_anchor_navigation_resolves_within_continuous_section_layout() {
        let mut source = CountingSource::new(&["placeholder".into()]);
        let source_mut = Arc::get_mut(&mut source).unwrap();
        let spine = source_mut.sections[0].id.clone();
        let preceding_text_len = FRAGMENT_TEXT_BUDGET * 6 + 100;
        let source_range = |node: &str, length: usize| SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: node.into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine: spine.clone(),
                node: node.into(),
                text_offset: u64::try_from(length).unwrap(),
            },
        };
        source_mut.sections[0].blocks = vec![
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: "a ".repeat(preceding_text_len / 2),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: BlockStyle::default(),
                source: Some(source_range("n0", preceding_text_len)),
            }),
            Block::Text(TextBlock {
                kind: TextBlockKind::Heading(2),
                content: vec![Inline::Text(TextRun {
                    text: "Target".into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: BlockStyle::default(),
                source: Some(source_range("n1", 6)),
            }),
        ];
        source_mut.sections[0].anchors = vec![SectionAnchor {
            fragment: "target".into(),
            source: source_range("n1", 6).start,
        }];
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        let target = PublicationUrl::parse("section-0.xhtml#target").unwrap();
        let target_position = reader.position_for_href(&target).unwrap();
        assert_eq!(target_position.segment_index, 0);
        assert!(target_position.page_index > 0);

        reader.go_to_href(&target).unwrap();

        assert_eq!(reader.location().segment_index, 0);
        assert_eq!(reader.location().page_index, target_position.page_index);
        assert!(reader.cache.contains_key(&SegmentKey {
            section_index: 0,
            segment_index: 0,
        }));
        assert_eq!(reader.cache.len(), 1);
        assert_eq!(source.parse_count(0), 1);
    }

    #[test]
    fn large_single_file_books_segment_at_top_level_toc_boundaries() {
        let mut source = CountingSource::new(&["placeholder".into()]);
        let source_mut = Arc::get_mut(&mut source).unwrap();
        let spine = source_mut.sections[0].id.clone();
        let chapter_text_len = LARGE_SECTION_TEXT_BUDGET / 2;
        let source_range = |node: &str| SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: node.into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine: spine.clone(),
                node: node.into(),
                text_offset: u64::try_from(chapter_text_len).unwrap(),
            },
        };
        source_mut.sections[0].blocks = ["chapter-1", "chapter-2", "chapter-3"]
            .into_iter()
            .map(|node| {
                Block::Text(TextBlock {
                    kind: TextBlockKind::Paragraph,
                    content: vec![Inline::Text(TextRun {
                        text: "a".repeat(chapter_text_len),
                        style: TextStyle::default(),
                        link: None,
                    })],
                    style: BlockStyle::default(),
                    source: Some(source_range(node)),
                })
            })
            .collect();
        source_mut.sections[0].anchors = ["chapter-2", "chapter-3"]
            .into_iter()
            .map(|fragment| SectionAnchor {
                fragment: fragment.into(),
                source: source_range(fragment).start,
            })
            .collect();
        source_mut.book.table_of_contents = vec![
            TocEntry {
                label: "Chapter 1".into(),
                href: Some(PublicationUrl::parse("section-0.xhtml").unwrap()),
                children: Vec::new(),
            },
            TocEntry {
                label: "Chapter 2".into(),
                href: Some(PublicationUrl::parse("section-0.xhtml#chapter-2").unwrap()),
                children: Vec::new(),
            },
            TocEntry {
                label: "Chapter 3".into(),
                href: Some(PublicationUrl::parse("section-0.xhtml#chapter-3").unwrap()),
                children: Vec::new(),
            },
        ];

        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        assert_eq!(reader.location().segment_count, 3);

        let target = PublicationUrl::parse("section-0.xhtml#chapter-3").unwrap();
        reader.go_to_href(&target).unwrap();
        assert_eq!(reader.location().segment_index, 2);
        assert_eq!(reader.location().page_index, 0);
    }

    #[test]
    fn toc_and_total_progression_advance_across_page_boundaries() {
        let mut source = CountingSource::new(&["placeholder".into()]);
        let source_mut = Arc::get_mut(&mut source).unwrap();
        let spine = source_mut.sections[0].id.clone();
        let preceding_text_len = FRAGMENT_TEXT_BUDGET * 4 + 100;
        let source_range = |node: &str, length: u64| SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: node.into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine: spine.clone(),
                node: node.into(),
                text_offset: length,
            },
        };
        source_mut.sections[0].blocks = vec![
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: "a ".repeat(preceding_text_len / 2),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: BlockStyle::default(),
                source: Some(source_range(
                    "n0",
                    u64::try_from(preceding_text_len).unwrap(),
                )),
            }),
            Block::Text(TextBlock {
                kind: TextBlockKind::Heading(2),
                content: vec![Inline::Text(TextRun {
                    text: "Later".into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: BlockStyle::default(),
                source: Some(source_range("n1", 5)),
            }),
        ];
        source_mut.sections[0].anchors = vec![SectionAnchor {
            fragment: "later".into(),
            source: source_range("n1", 5).start,
        }];
        source_mut.book.table_of_contents = vec![
            TocEntry {
                label: "Start".into(),
                href: Some(PublicationUrl::parse("section-0.xhtml").unwrap()),
                children: Vec::new(),
            },
            TocEntry {
                label: "Later".into(),
                href: Some(PublicationUrl::parse("section-0.xhtml#later").unwrap()),
                children: Vec::new(),
            },
        ];
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        assert_eq!(reader.snapshot().active_toc_id.as_deref(), Some("0"));
        let mut previous_progress = reader.snapshot().total_progression;

        for _ in 0..100 {
            let result = reader.turn_page(PageDirection::Next).unwrap();
            assert!(result.snapshot.total_progression > previous_progress);
            previous_progress = result.snapshot.total_progression;
            if result.snapshot.active_toc_id.as_deref() == Some("1") {
                assert_eq!(result.snapshot.location.segment_index, 1);
                assert_eq!(result.snapshot.location.page_index, 0);
                assert_eq!(source.parse_count(0), 1);
                return;
            }
        }
        panic!("reader did not reach the later fragment TOC anchor");
    }

    #[test]
    fn adjacent_prefetch_never_blocks_the_caller_thread() {
        let source = CountingSource::with_background_delay(
            &["第一章".into(), "第二章".into()],
            Duration::from_millis(300),
        );
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();

        let started = Instant::now();
        reader.prefetch_adjacent().unwrap();
        assert!(started.elapsed() < Duration::from_millis(100));

        reader.wait_for_prefetch().unwrap();
        assert_eq!(source.parse_count(1), 1);
    }

    #[test]
    fn interactive_page_turn_waits_in_background_instead_of_blocking() {
        let blocking_source = CountingSource::with_background_delay(
            &["first".into(), "second".into()],
            Duration::from_millis(300),
        );
        let mut blocking_reader =
            ReaderSession::open(blocking_source, viewport(600, 400), ReaderStyle::default())
                .unwrap();
        let blocking_started = Instant::now();
        let blocking_result = blocking_reader.turn_page(PageDirection::Next).unwrap();
        let blocking_elapsed = blocking_started.elapsed();
        assert_eq!(blocking_result.outcome, NavigationOutcome::Moved);
        assert!(blocking_elapsed >= Duration::from_millis(250));

        let source = CountingSource::with_background_delay(
            &["first".into(), "second".into()],
            Duration::from_millis(300),
        );
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        let nonblocking_started = Instant::now();
        let attempt = reader.try_turn_page(PageDirection::Next).unwrap();
        let nonblocking_elapsed = nonblocking_started.elapsed();

        assert_eq!(attempt, NavigationAttempt::Pending);
        assert!(nonblocking_elapsed < Duration::from_millis(100));
        assert!(blocking_elapsed > nonblocking_elapsed * 2);
        assert_eq!(reader.location().section_index, 0);

        reader.wait_for_prefetch().unwrap();
        let attempt = reader.try_turn_page(PageDirection::Next).unwrap();
        let NavigationAttempt::Ready(result) = attempt else {
            panic!("prefetched destination should be ready");
        };
        assert_eq!(result.outcome, NavigationOutcome::Moved);
        assert_eq!(result.snapshot.location.section_index, 1);
    }

    #[test]
    fn interactive_focus_chapter_turn_waits_in_background_in_both_directions() {
        let source = CountingSource::with_background_delay(
            &["first".into(), "large target".into(), "third".into()],
            Duration::from_millis(300),
        );
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();

        let started = Instant::now();
        let attempt = reader
            .try_go_to_adjacent_reading_unit(PageDirection::Next)
            .unwrap();
        assert_eq!(attempt, NavigationAttempt::Pending);
        assert!(started.elapsed() < Duration::from_millis(100));
        reader.wait_for_prefetch().unwrap();
        let NavigationAttempt::Ready(result) = reader
            .try_go_to_adjacent_reading_unit(PageDirection::Next)
            .unwrap()
        else {
            panic!("next chapter should be ready after prefetch");
        };
        assert_eq!(result.snapshot.location.section_index, 1);

        let source = CountingSource::with_background_delay(
            &["first".into(), "large target".into(), "third".into()],
            Duration::from_millis(300),
        );
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        reader.go_to_section(2).unwrap();
        let started = Instant::now();
        let attempt = reader
            .try_go_to_adjacent_reading_unit(PageDirection::Previous)
            .unwrap();
        assert_eq!(attempt, NavigationAttempt::Pending);
        assert!(started.elapsed() < Duration::from_millis(100));
        reader.wait_for_prefetch().unwrap();
        let NavigationAttempt::Ready(result) = reader
            .try_go_to_adjacent_reading_unit(PageDirection::Previous)
            .unwrap()
        else {
            panic!("previous chapter should be ready after prefetch");
        };
        assert_eq!(result.snapshot.location.section_index, 1);
    }

    #[test]
    fn interactive_toc_navigation_does_not_parse_the_target_on_the_caller() {
        let mut source = CountingSource::with_background_delay(
            &["first".into(), "large target".into()],
            Duration::from_millis(300),
        );
        Arc::get_mut(&mut source).unwrap().book.table_of_contents = vec![TocEntry {
            label: "Target".into(),
            href: Some(PublicationUrl::parse("section-1.xhtml#target").unwrap()),
            children: Vec::new(),
        }];
        let target = PublicationUrl::parse("section-1.xhtml#target").unwrap();
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();

        let started = Instant::now();
        let attempt = reader.try_go_to_href(&target).unwrap();
        assert_eq!(attempt, NavigationAttempt::Pending);
        assert!(started.elapsed() < Duration::from_millis(100));
        reader.wait_for_prefetch().unwrap();
        let NavigationAttempt::Ready(result) = reader.try_go_to_href(&target).unwrap() else {
            panic!("TOC target should be ready after prefetch");
        };
        assert_eq!(result.snapshot.location.section_index, 1);
    }

    #[test]
    fn background_section_parse_does_not_block_snapshot_updates() {
        let mut source = CountingSource::with_background_delay(
            &["first".into(), "second".into()],
            Duration::from_millis(300),
        );
        Arc::get_mut(&mut source).unwrap().book.table_of_contents = vec![TocEntry {
            label: "Second".into(),
            href: Some(PublicationUrl::parse("section-1.xhtml").unwrap()),
            children: Vec::new(),
        }];
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        reader.prefetch_adjacent().unwrap();
        thread::sleep(Duration::from_millis(30));

        let started = Instant::now();
        let _snapshot = reader.snapshot();

        assert!(started.elapsed() < Duration::from_millis(100));
        reader.wait_for_prefetch().unwrap();
    }

    #[test]
    fn resize_rebuilds_layout_and_preserves_approximate_progress() {
        let source = CountingSource::new(&["调整窗口后保持阅读进度。".repeat(600)]);
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        let old_count = reader.location().page_count;
        assert!(old_count > 4);
        for _ in 0..old_count / 2 {
            reader.turn_page(PageDirection::Next).unwrap();
        }
        let old_fraction = page_fraction(reader.location().page_index, old_count);

        reader.resize(viewport(500, 300)).unwrap();

        let location = reader.location();
        let new_fraction = page_fraction(location.page_index, location.page_count);
        let one_page = page_fraction(1, location.page_count);
        assert!((new_fraction - old_fraction).abs() <= one_page);
        assert_eq!(source.parse_count(0), 1);
        assert_eq!(reader.cached_segment_count(), 1);
    }

    #[test]
    fn font_family_change_rebuilds_layout_and_preserves_approximate_progress() {
        let source = CountingSource::new(&["字体切换后保持阅读进度。".repeat(600)]);
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        let old_count = reader.location().page_count;
        assert!(old_count > 4);
        for _ in 0..old_count / 2 {
            reader.turn_page(PageDirection::Next).unwrap();
        }
        let old_fraction = page_fraction(reader.location().page_index, old_count);

        let mut style = reader.style();
        style.typography.default_font = ReaderDefaultFont::SansSerif;
        reader.set_style(style).unwrap();

        let location = reader.location();
        let new_fraction = page_fraction(location.page_index, location.page_count);
        let one_page = page_fraction(1, location.page_count);
        assert!((new_fraction - old_fraction).abs() <= one_page);
        assert_eq!(
            reader.style().typography.default_font,
            ReaderDefaultFont::SansSerif
        );
        assert_eq!(source.parse_count(0), 1);
        assert_eq!(reader.cached_segment_count(), 1);
    }

    #[test]
    fn source_refresh_reparses_and_preserves_approximate_progress() {
        let source = CountingSource::new(&["派生正文刷新后保持阅读进度。".repeat(600)]);
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        let old_count = reader.location().page_count;
        for _ in 0..old_count / 2 {
            reader.turn_page(PageDirection::Next).unwrap();
        }
        let old_fraction = page_fraction(reader.location().page_index, old_count);

        reader.refresh_source().unwrap();

        let location = reader.location();
        let new_fraction = page_fraction(location.page_index, location.page_count);
        let one_page = page_fraction(1, location.page_count);
        assert!((new_fraction - old_fraction).abs() <= one_page);
        assert_eq!(source.parse_count(0), 2);
        assert_eq!(reader.cached_segment_count(), 1);
    }

    #[test]
    fn source_refresh_preserves_the_first_visible_source_anchor_after_repagination() {
        let original = CountingSource::new(&["stable anchor ".repeat(1_600)]);
        let derived = CountingSource::new(&["stable anchor ".repeat(2_400)]);
        let source = SwitchingSource::new(&original, &derived);
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        for _ in 0..reader.location().page_count / 2 {
            reader.turn_page(PageDirection::Next).unwrap();
        }
        let anchor = reader.current_page().leading_source_range().unwrap().start;

        source.set_derived(true);
        reader.refresh_source().unwrap();

        assert!(reader.current_page().contains_source_anchor(&anchor));
    }

    #[test]
    fn source_refresh_rebuilds_navigation_when_reading_order_changes() {
        let original = CountingSource::new(&[
            "Original page one".into(),
            "Original page two".into(),
            "Original page three".into(),
        ]);
        let derived = CountingSource::new(&["Continuous OCR section".into()]);
        let source = SwitchingSource::new(&original, &derived);
        let mut reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();

        source.set_derived(true);
        let derived_target = source.derived_book.sections[0].href.clone();
        let snapshot = reader
            .refresh_source_with_style_at_href(ReaderStyle::default(), Some(&derived_target))
            .unwrap();
        assert_eq!(snapshot.location.section_index, 0);
        assert_eq!(reader.section_index_for_href(&derived_target), Some(0));
        assert_eq!(reader.section_count(), 1);

        source.set_derived(false);
        let original_target = source.original_book.sections[2].href.clone();
        let snapshot = reader
            .refresh_source_with_style_at_href(ReaderStyle::default(), Some(&original_target))
            .unwrap();
        assert_eq!(snapshot.location.section_index, 2);
        assert_eq!(reader.section_index_for_href(&original_target), Some(2));
        assert_eq!(reader.section_count(), 3);
    }

    #[test]
    fn toc_items_preserve_reading_order_depth_and_ancestry() {
        let first_target = PublicationUrl::parse("text/chapter-1.xhtml").unwrap();
        let child_target = PublicationUrl::parse("text/chapter-1.xhtml#part-1").unwrap();
        let items = flatten_toc(&[
            TocEntry {
                label: "第一章".into(),
                href: Some(first_target.clone()),
                children: vec![TocEntry {
                    label: "第一节".into(),
                    href: Some(child_target.clone()),
                    children: Vec::new(),
                }],
            },
            TocEntry {
                label: "第二章".into(),
                href: None,
                children: Vec::new(),
            },
        ]);

        assert_eq!(items.len(), 3);
        assert_eq!((items[0].label.as_str(), items[0].depth), ("第一章", 0));
        assert_eq!(items[0].target.as_ref(), Some(&first_target));
        assert!(items[0].has_children);
        assert!(items[0].ancestors.is_empty());
        assert_eq!((items[1].label.as_str(), items[1].depth), ("第一节", 1));
        assert_eq!(items[1].target.as_ref(), Some(&child_target));
        assert_eq!(items[1].ancestors, ["0"]);
        assert_eq!((items[2].label.as_str(), items[2].depth), ("第二章", 0));
        assert!(items[2].target.is_none());
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "the fixture mirrors one complete TOC hierarchy"
    )]
    fn chapter_prelude_and_leaf_targets_are_independent_reading_units() {
        let mut source = CountingSource::new(&["placeholder".into()]);
        let source_mut = Arc::get_mut(&mut source).unwrap();
        let spine = source_mut.sections[0].id.clone();
        let block = |node: &str, text: &str| {
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: BlockStyle::default(),
                source: Some(SourceRange {
                    start: SourceAnchor {
                        spine: spine.clone(),
                        node: node.into(),
                        text_offset: 0,
                    },
                    end: SourceAnchor {
                        spine: spine.clone(),
                        node: node.into(),
                        text_offset: u64::try_from(text.chars().count()).unwrap(),
                    },
                }),
            })
        };
        source_mut.sections[0].blocks = vec![
            block("intro", "Introduction"),
            block("leaf-a", "First leaf"),
            block("leaf-b", "Second leaf"),
        ];
        source_mut.sections[0].anchors = vec![
            SectionAnchor {
                fragment: "chapter".into(),
                source: SourceAnchor {
                    spine: spine.clone(),
                    node: "intro".into(),
                    text_offset: 0,
                },
            },
            SectionAnchor {
                fragment: "leaf-a".into(),
                source: SourceAnchor {
                    spine: spine.clone(),
                    node: "leaf-a".into(),
                    text_offset: 0,
                },
            },
            SectionAnchor {
                fragment: "leaf-b".into(),
                source: SourceAnchor {
                    spine: spine.clone(),
                    node: "leaf-b".into(),
                    text_offset: 0,
                },
            },
        ];
        source_mut.book.table_of_contents = vec![TocEntry {
            label: "Chapter".into(),
            href: Some(PublicationUrl::parse("section-0.xhtml#chapter").unwrap()),
            children: vec![
                TocEntry {
                    label: "Leaf A".into(),
                    href: Some(PublicationUrl::parse("section-0.xhtml#leaf-a").unwrap()),
                    children: Vec::new(),
                },
                TocEntry {
                    label: "Leaf B".into(),
                    href: Some(PublicationUrl::parse("section-0.xhtml#leaf-b").unwrap()),
                    children: Vec::new(),
                },
            ],
        }];

        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        assert_eq!(
            reader.reading_unit_location(),
            ReadingUnitLocation { index: 0, count: 3 }
        );
        assert_eq!(reader.location().segment_count, 3);
        let prelude_ranges = reader.current_reading_unit_source_ranges().unwrap();
        assert_eq!(prelude_ranges.len(), 1);
        assert_eq!(prelude_ranges[0].start.node, "intro");

        let result = reader
            .go_to_adjacent_reading_unit(PageDirection::Next)
            .unwrap();
        assert_eq!(result.outcome, NavigationOutcome::Moved);
        assert_eq!(
            reader.reading_unit_location(),
            ReadingUnitLocation { index: 1, count: 3 }
        );
        let first_leaf_ranges = reader.current_reading_unit_source_ranges().unwrap();
        assert_eq!(first_leaf_ranges.len(), 1);
        assert_eq!(first_leaf_ranges[0].start.node, "leaf-a");
        assert_eq!(reader.location().segment_index, 1);

        reader
            .go_to_adjacent_reading_unit(PageDirection::Next)
            .unwrap();
        let second_leaf_ranges = reader.current_reading_unit_source_ranges().unwrap();
        assert_eq!(second_leaf_ranges.len(), 1);
        assert_eq!(second_leaf_ranges[0].start.node, "leaf-b");
        assert_eq!(reader.location().segment_index, 2);
    }

    #[test]
    fn heading_only_chapter_prelude_joins_the_first_subsection() {
        let spine = SpineItemId::new("chapter").unwrap();
        let block = |node: &str, kind: TextBlockKind, text: &str| {
            Block::Text(TextBlock {
                kind,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: BlockStyle::default(),
                source: Some(SourceRange {
                    start: SourceAnchor {
                        spine: spine.clone(),
                        node: node.into(),
                        text_offset: 0,
                    },
                    end: SourceAnchor {
                        spine: spine.clone(),
                        node: node.into(),
                        text_offset: u64::try_from(text.chars().count()).unwrap(),
                    },
                }),
            })
        };
        let anchor = |fragment: &str, node: &str| SectionAnchor {
            fragment: fragment.into(),
            source: SourceAnchor {
                spine: spine.clone(),
                node: node.into(),
                text_offset: 0,
            },
        };
        let fragments = vec![
            ContentFragment {
                blocks: vec![block("heading", TextBlockKind::Heading(1), "Prelude")],
                anchors: vec![anchor("chapter", "heading")],
            },
            ContentFragment {
                blocks: vec![block("body", TextBlockKind::Paragraph, "First subsection")],
                anchors: vec![anchor("leaf", "body")],
            },
        ];
        let units = build_reading_units(
            &[("chapter".into(), 0), ("leaf".into(), 1)],
            &[Some("chapter".into()), Some("leaf".into())],
            &fragments,
        );

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].fragment_range, 0..2);
        assert_eq!(units[0].start.as_ref().unwrap().node, "body");
    }

    #[test]
    fn fixed_layout_toc_units_span_complete_physical_pages() {
        let mut source = CountingSource::new(&[
            "Chapter title".into(),
            "Section one page one".into(),
            "Section one page two".into(),
            "Next chapter".into(),
            "Section two".into(),
        ]);
        let source_mut = Arc::get_mut(&mut source).unwrap();
        source_mut.book.metadata.layout = RenditionLayout::PrePaginated;
        let target = |index: usize| source_mut.book.sections[index].href.clone();
        source_mut.book.table_of_contents = vec![
            TocEntry {
                label: "Chapter 1".into(),
                href: Some(target(0)),
                children: vec![TocEntry {
                    label: "Section 1.1".into(),
                    href: Some(target(1)),
                    children: Vec::new(),
                }],
            },
            TocEntry {
                label: "Chapter 2".into(),
                href: Some(target(3)),
                children: vec![TocEntry {
                    label: "Section 2.1".into(),
                    href: Some(target(4)),
                    children: Vec::new(),
                }],
            },
        ];

        let mut reader = ReaderSession::open(
            source,
            viewport(600, 400),
            ReaderStyle {
                spread: SpreadMode::Scroll,
                ..ReaderStyle::default()
            },
        )
        .unwrap();
        assert_eq!(
            reader.reading_unit_location(),
            ReadingUnitLocation { index: 0, count: 4 }
        );
        assert_eq!(reader.current_reading_unit_pages().unwrap().len(), 1);

        reader
            .go_to_adjacent_reading_unit(PageDirection::Next)
            .unwrap();
        let pages = reader.current_reading_unit_pages().unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].position.section_index, 1);
        assert_eq!(pages[1].position.section_index, 2);
        let snapshot = reader.set_visible_position(pages[1].position).unwrap();
        assert_eq!(snapshot.location.section_index, 2);
        assert_eq!(
            reader.reading_unit_location(),
            ReadingUnitLocation { index: 1, count: 4 }
        );
    }

    #[test]
    fn fixed_layout_scroll_units_materialize_only_nearby_pages() {
        let source = LazyFixedSource::new(3);
        let mut reader = ReaderSession::open(
            Arc::clone(&source) as Arc<dyn BookSource>,
            viewport(600, 400),
            ReaderStyle {
                spread: SpreadMode::Scroll,
                ..ReaderStyle::default()
            },
        )
        .unwrap();

        let pages = reader.current_reading_unit_pages().unwrap();
        assert_eq!(pages.len(), 3);
        assert!(!pages[0].placeholder);
        assert!(pages[1].placeholder);
        assert!(pages[2].placeholder);
        assert_eq!(source.parse_counts[0].load(Ordering::Relaxed), 1);
        assert_eq!(source.parse_counts[1].load(Ordering::Relaxed), 0);
        assert_eq!(source.parse_counts[2].load(Ordering::Relaxed), 0);
        assert_eq!(source.raster_counts[0].load(Ordering::Relaxed), 1);
        assert_eq!(source.raster_counts[1].load(Ordering::Relaxed), 0);

        let target = pages[1].position;
        assert!(!reader.try_materialize_position(target).unwrap());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !reader.try_materialize_position(target).unwrap() {
            assert!(Instant::now() < deadline, "background page did not finish");
            std::thread::yield_now();
        }

        let pages = reader.current_reading_unit_pages().unwrap();
        assert!(!pages[1].placeholder);
        assert!(pages[2].placeholder);
        assert_eq!(source.parse_counts[1].load(Ordering::Relaxed), 1);
        assert_eq!(source.raster_counts[1].load(Ordering::Relaxed), 1);
        assert_eq!(source.parse_counts[2].load(Ordering::Relaxed), 0);
    }

    #[test]
    fn parent_only_toc_keeps_the_spine_as_one_reading_unit() {
        let mut source = CountingSource::new(&["content".into()]);
        Arc::get_mut(&mut source).unwrap().book.table_of_contents = vec![TocEntry {
            label: "Chapter".into(),
            href: Some(PublicationUrl::parse("section-0.xhtml#chapter").unwrap()),
            children: vec![TocEntry {
                label: "Non-navigable leaf".into(),
                href: None,
                children: Vec::new(),
            }],
        }];
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();

        assert_eq!(
            reader.reading_unit_location(),
            ReadingUnitLocation { index: 0, count: 1 }
        );
    }

    #[test]
    fn active_toc_follows_the_nearest_preceding_segment_page() {
        let items = vec![
            TocViewItem {
                id: "previous".into(),
                label: "Previous".into(),
                target: Some(PublicationUrl::parse("section-0.xhtml#previous").unwrap()),
                depth: 0,
                ancestors: Vec::new(),
                has_children: false,
            },
            TocViewItem {
                id: "chapter".into(),
                label: "Chapter".into(),
                target: Some(PublicationUrl::parse("section-1.xhtml#chapter").unwrap()),
                depth: 0,
                ancestors: Vec::new(),
                has_children: true,
            },
            TocViewItem {
                id: "subsection".into(),
                label: "Subsection".into(),
                target: Some(PublicationUrl::parse("section-1.xhtml#subsection").unwrap()),
                depth: 1,
                ancestors: vec!["chapter".into()],
                has_children: false,
            },
            TocViewItem {
                id: "future".into(),
                label: "Future".into(),
                target: Some(PublicationUrl::parse("section-2.xhtml#future").unwrap()),
                depth: 0,
                ancestors: Vec::new(),
                has_children: false,
            },
        ];
        let position = |section_index, segment_index, page_index| ReaderPosition {
            section_index,
            segment_index,
            page_index,
        };
        let resolve = |target: &PublicationUrl| match (target.path(), target.fragment()) {
            ("section-0.xhtml", _) => Some(position(0, 0, 4)),
            ("section-1.xhtml", Some("chapter")) => Some(position(1, 1, 2)),
            ("section-1.xhtml", Some("subsection")) => Some(position(1, 2, 0)),
            ("section-2.xhtml", _) => Some(position(2, 0, 0)),
            _ => None,
        };

        assert_eq!(
            active_toc_item_for_location(&items, &[1, 2], &[0], 1, 0, 1, resolve)
                .unwrap()
                .id,
            "previous"
        );
        assert_eq!(
            active_toc_item_for_location(&items, &[1, 2], &[0], 1, 1, 2, resolve)
                .unwrap()
                .id,
            "chapter"
        );
        assert_eq!(
            active_toc_item_for_location(&items, &[1, 2], &[0], 1, 2, 0, resolve)
                .unwrap()
                .id,
            "subsection"
        );
    }

    #[test]
    fn large_toc_snapshot_resolves_only_neighboring_sections() {
        let item_count = 2_034;
        let items = (0..item_count)
            .map(|index| TocViewItem {
                id: index.to_string(),
                label: format!("Chapter {index}"),
                target: Some(PublicationUrl::parse(&format!("section-{index}.xhtml")).unwrap()),
                depth: 0,
                ancestors: Vec::new(),
                has_children: false,
            })
            .collect::<Vec<_>>();
        let section_indices = items
            .iter()
            .enumerate()
            .map(|(index, item)| (item.target.as_ref().unwrap().path().to_owned(), index))
            .collect::<HashMap<_, _>>();
        let index = TocIndex::new(&items, &section_indices, item_count);
        let current_section = 1_500;
        let preceding_section = index.preceding_section_by_section[current_section].unwrap();
        let resolve_count = AtomicUsize::new(0);

        let active = active_toc_item_for_location(
            &items,
            &index.items_by_section[current_section],
            &index.items_by_section[preceding_section],
            current_section,
            0,
            0,
            |target| {
                resolve_count.fetch_add(1, Ordering::Relaxed);
                let section_index = section_indices.get(target.path()).copied()?;
                Some(ReaderPosition {
                    section_index,
                    segment_index: 0,
                    page_index: 0,
                })
            },
        )
        .unwrap();

        assert_eq!(active.id, current_section.to_string());
        assert_eq!(resolve_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn snapshot_owns_active_toc_state_and_progression() {
        let mut source = CountingSource::new(&["正文".repeat(600)]);
        Arc::get_mut(&mut source).unwrap().book.table_of_contents = vec![TocEntry {
            label: "Chapter".into(),
            href: Some(PublicationUrl::parse("section-0.xhtml").unwrap()),
            children: vec![TocEntry {
                label: "Child".into(),
                href: Some(PublicationUrl::parse("section-0.xhtml").unwrap()),
                children: Vec::new(),
            }],
        }];
        let reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();

        let snapshot = reader.snapshot();
        assert_eq!(snapshot.active_toc_id.as_deref(), Some("0/0"));
        assert_eq!(snapshot.active_toc_path, ["0"]);
        assert!(snapshot.total_progression > 0.0);
        assert!(snapshot.total_progression <= 1.0);
    }

    #[test]
    fn stale_prefetch_result_cannot_clear_current_generation_request() {
        let source = CountingSource::new(&["第一章".into(), "第二章".into()]);
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        let stale_generation = reader.prefetch_worker.generation();
        let current_generation = reader.prefetch_worker.invalidate();
        let segment = SegmentKey {
            section_index: 1,
            segment_index: 0,
        };
        let current_key = PrefetchKey {
            generation: current_generation,
            segment,
        };
        reader.prefetch_inflight.insert(current_key);
        let section = Arc::clone(reader.current_section_data());

        reader.install_prefetch(PrefetchResult {
            key: segment,
            generation: stale_generation,
            segment: Ok(Arc::new(CachedSegment {
                section,
                pages: Vec::new(),
                anchor_pages: HashMap::new(),
                visible_pages: 1,
                continuation_offset_x: 0.0,
            })),
        });

        assert!(reader.prefetch_inflight.contains(&current_key));
    }

    #[test]
    fn dropping_reader_joins_worker_and_releases_source() {
        let source = CountingSource::new(&["正文".into()]);
        let weak = Arc::downgrade(&source);
        let reader =
            ReaderSession::open(source.clone(), viewport(600, 400), ReaderStyle::default())
                .unwrap();
        drop(source);

        assert!(weak.upgrade().is_some());
        drop(reader);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn prepared_navigation_leaves_state_unchanged_and_commits_cleanly() {
        let source = CountingSource::new(&["第一章内容".into(), "第二章内容".into()]);
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        reader.wait_for_prefetch().unwrap();

        let initial_position = reader.current_position();
        let initial_locator = reader.current_locator();
        let initial_snapshot = reader.snapshot();

        // 1. Boundary at beginning
        match reader.prepare_navigation(PageDirection::Previous).unwrap() {
            NavigationPreparation::Boundary => {}
            _ => panic!("expected boundary at start"),
        }
        assert_eq!(reader.current_position(), initial_position);

        // 2. Prepare next: initially destination section is not cached yet, so preparation is Pending
        let prep = reader.prepare_navigation(PageDirection::Next).unwrap();
        let token = match prep {
            NavigationPreparation::Pending(tok) => tok,
            NavigationPreparation::Ready(_) => panic!("expected pending before prefetch"),
            NavigationPreparation::Boundary => panic!("expected pending, not boundary"),
        };
        assert_eq!(reader.current_position(), initial_position);
        assert_eq!(reader.current_locator(), initial_locator);
        assert_eq!(reader.snapshot(), initial_snapshot);

        // Wait for prefetch worker to compile the destination
        reader.wait_for_prefetch().unwrap();

        // Polling the token now yields Ready
        let NavigationPreparation::Ready(prepared) = reader.poll_navigation(token).unwrap() else {
            panic!("expected ready after wait_for_prefetch");
        };
        assert_eq!(prepared.source(), initial_position);
        assert_ne!(prepared.destination(), initial_position);
        assert_eq!(reader.current_position(), initial_position);
        assert_eq!(reader.current_locator(), initial_locator);
        assert_eq!(reader.snapshot(), initial_snapshot);

        // 3. Cancel prepared navigation
        let token = prepared.token();
        assert!(reader.cancel_navigation(token));
        assert_eq!(reader.current_position(), initial_position);
        assert_eq!(reader.current_locator(), initial_locator);

        // 4. Reprepare (now cached, returns Ready immediately) and commit
        let NavigationPreparation::Ready(prepared) =
            reader.prepare_navigation(PageDirection::Next).unwrap()
        else {
            panic!("expected ready prepared navigation");
        };
        let expected_destination = prepared.destination();
        let result = reader.commit_navigation(prepared).unwrap();
        assert_eq!(result.outcome, NavigationOutcome::Moved);
        assert_eq!(reader.current_position(), expected_destination);

        // 5. Double commit with same token fails
        reader.wait_for_prefetch().unwrap();
        let NavigationPreparation::Ready(stale_prepared) =
            reader.prepare_navigation(PageDirection::Previous).unwrap()
        else {
            panic!("expected ready prepared navigation");
        };
        let token = stale_prepared.token();
        reader.cancel_navigation(token);
        assert!(matches!(
            reader.commit_navigation(stale_prepared),
            Err(ReaderError::StaleNavigationToken)
        ));
    }

    #[test]
    fn navigation_tokens_are_invalidated_by_resize_and_style() {
        let source = CountingSource::new(&["第一章内容".into(), "第二章内容".into()]);
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        reader.wait_for_prefetch().unwrap();

        // First prepare triggers prefetch
        let NavigationPreparation::Pending(token) =
            reader.prepare_navigation(PageDirection::Next).unwrap()
        else {
            panic!("expected pending");
        };
        reader.wait_for_prefetch().unwrap();

        let NavigationPreparation::Ready(prepared) = reader.poll_navigation(token).unwrap() else {
            panic!("expected ready prepared navigation");
        };
        let token = prepared.token();

        // Resize invalidates token
        reader.resize(viewport(800, 600)).unwrap();
        assert!(!reader.cancel_navigation(token));
        assert!(matches!(
            reader.poll_navigation(token),
            Err(ReaderError::StaleNavigationToken)
        ));
        assert!(matches!(
            reader.commit_navigation(prepared),
            Err(ReaderError::StaleNavigationToken)
        ));

        // Wait for prefetch after resize
        reader.wait_for_prefetch().unwrap();
        let NavigationPreparation::Pending(token2) =
            reader.prepare_navigation(PageDirection::Next).unwrap()
        else {
            panic!("expected pending");
        };
        reader.wait_for_prefetch().unwrap();
        let NavigationPreparation::Ready(prepared2) = reader.poll_navigation(token2).unwrap()
        else {
            panic!("expected ready prepared navigation");
        };
        let token2 = prepared2.token();

        let mut style = reader.style().clone();
        style.column_gap += 10.0;
        reader.set_style(style).unwrap();
        assert!(matches!(
            reader.commit_navigation(prepared2),
            Err(ReaderError::StaleNavigationToken)
        ));
        assert!(!reader.cancel_navigation(token2));
    }

    #[test]
    fn scheduler_pending_navigation_becomes_ready_over_ticks() {
        let source =
            CountingSource::new(&["Chương 1".into(), "Chương 2".into(), "Chương 3".into()]);
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();

        // 1. Prepare navigation forwards
        let prep = reader.prepare_navigation(PageDirection::Next).unwrap();
        let token = match prep {
            NavigationPreparation::Pending(tok) => tok,
            NavigationPreparation::Ready(_) => panic!("expected pending before compilation"),
            NavigationPreparation::Boundary => panic!("expected pending, not boundary"),
        };

        // 2. Advancing work via tick
        let _ = reader.tick(Duration::from_millis(50)).unwrap();
        reader.wait_for_prefetch().unwrap();

        // 3. Polling navigation now returns Ready
        let prep_ready = reader.poll_navigation(token).unwrap();
        let NavigationPreparation::Ready(prepared) = prep_ready else {
            panic!("navigation transaction should be ready after tick/prefetch");
        };

        // 4. Commit moves the reader
        let result = reader.commit_navigation(prepared).unwrap();
        assert_eq!(result.outcome, NavigationOutcome::Moved);
        assert_eq!(result.snapshot.location.section_index, 1);
    }

    #[test]
    fn scheduler_opposite_direction_cancels_and_replaces_pending_navigation() {
        let source = CountingSource::new(&["Một".into(), "Hai".into(), "Ba".into()]);
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        reader.wait_for_prefetch().unwrap();

        // Go to section 1 first
        reader.go_to_section(1).unwrap();
        let section_1_pos = reader.current_position();

        // Prepare Next
        let NavigationPreparation::Pending(token_next) =
            reader.prepare_navigation(PageDirection::Next).unwrap()
        else {
            panic!("expected pending for next");
        };

        // Opposite direction (Previous) replaces the active navigation transaction
        let prep_prev = reader.prepare_navigation(PageDirection::Previous).unwrap();
        match prep_prev {
            NavigationPreparation::Pending(token_prev) => {
                assert_ne!(token_prev, token_next);
                // Old token is now stale/rejected
                assert!(matches!(
                    reader.poll_navigation(token_next),
                    Err(ReaderError::StaleNavigationToken)
                ));
            }
            NavigationPreparation::Ready(prepared_prev) => {
                assert!(matches!(
                    reader.poll_navigation(token_next),
                    Err(ReaderError::StaleNavigationToken)
                ));
                let res = reader.commit_navigation(prepared_prev).unwrap();
                assert_eq!(res.outcome, NavigationOutcome::Moved);
                assert_eq!(res.snapshot.location.section_index, 0);
            }
            NavigationPreparation::Boundary => panic!("expected pending or ready, not boundary"),
        }
        assert_eq!(
            reader.current_position().section_index,
            if prep_prev_was_ready(&reader) {
                0
            } else {
                section_1_pos.section_index
            }
        );
    }

    fn prep_prev_was_ready(reader: &ReaderSession) -> bool {
        reader.current_position().section_index == 0
    }

    #[test]
    fn scheduler_boundary_navigation_never_mutates_location() {
        let source = CountingSource::new(&["Single Page Section".into()]);
        let mut reader =
            ReaderSession::open(source, viewport(600, 400), ReaderStyle::default()).unwrap();
        reader.wait_for_prefetch().unwrap();

        let initial_location = reader.location();

        // Previous at beginning
        let prep_prev = reader.prepare_navigation(PageDirection::Previous).unwrap();
        assert!(matches!(prep_prev, NavigationPreparation::Boundary));
        assert_eq!(reader.location(), initial_location);

        // Next at end
        let prep_next = reader.prepare_navigation(PageDirection::Next).unwrap();
        assert!(matches!(prep_next, NavigationPreparation::Boundary));
        assert_eq!(reader.location(), initial_location);
    }
