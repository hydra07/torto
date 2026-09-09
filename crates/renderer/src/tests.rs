    use super::*;
    use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext, StyleProperty};
    use rebook_layout::{
        FixedPageTextReplacementPlacement, FixedPageTextReplacementSegmentPlacement,
        ImagePlacement, InlineImage, LayoutViewport, PageItem, PageLayout, QuotePlacement,
        RasterImage, SeparatorPlacement, TextBrush, TextPlacement,
    };

    #[test]
    fn synthetic_bold_uses_two_and_a_half_percent_em_outline_expansion() {
        let regular = synthetic_embolden(false, 20.0);
        let bold = synthetic_embolden(true, 20.0);

        assert_eq!(regular, Vec2::ZERO);
        assert!((bold.x - 0.5).abs() < f64::EPSILON);
        assert!((bold.y - 0.5).abs() < f64::EPSILON);
        assert_eq!(synthetic_embolden(true, 0.0), Vec2::ZERO);
        assert_eq!(synthetic_embolden(true, f32::NAN), Vec2::ZERO);
    }

    #[test]
    fn rounded_quote_decorations_are_painted_below_source_overlays() {
        let page = PageLayout {
            viewport: LayoutViewport::new(200, 200).unwrap(),
            background: Rgba {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            },
            leading_gap: 0.0,
            items: vec![PageItem::Quote(QuotePlacement {
                x: 20.0,
                y: 30.0,
                width: 160.0,
                height: 80.0,
                continued_before: false,
                continued_after: false,
                fill: Rgba {
                    alpha: 0,
                    ..Rgba::BLACK
                },
                accent: Rgba {
                    red: 0xD1,
                    green: 0xD7,
                    blue: 0xDE,
                    alpha: 255,
                },
                sources: Vec::new(),
            })],
        };

        let list = DisplayListCompiler.compile(&page);
        let quote_decorations = list
            .commands
            .iter()
            .filter(|command| matches!(command, DisplayCommand::FillRoundedRect(_)))
            .collect::<Vec<_>>();
        assert_eq!(quote_decorations.len(), 1);
        assert!(
            quote_decorations
                .iter()
                .all(|command| command.paints_below_source_overlays())
        );
    }

    #[test]
    fn continued_quote_segments_trim_page_padding_and_join_the_accent() {
        let page = PageLayout {
            viewport: LayoutViewport::new(200, 200).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![
                PageItem::Quote(QuotePlacement {
                    x: 20.0,
                    y: 10.0,
                    width: 160.0,
                    height: 180.0,
                    continued_before: true,
                    continued_after: true,
                    fill: Rgba {
                        alpha: 0,
                        ..Rgba::BLACK
                    },
                    accent: Rgba::BLACK,
                    sources: Vec::new(),
                }),
                PageItem::Separator(SeparatorPlacement {
                    x: 40.0,
                    y: 50.0,
                    width: 80.0,
                }),
            ],
        };

        let list = DisplayListCompiler.compile(&page);
        assert_eq!(list.content_top(), Some(50.0));
        assert_eq!(list.content_bottom(), Some(51.0));
        let accent = list
            .commands
            .iter()
            .find_map(|command| match command {
                DisplayCommand::FillRoundedRect(command) => Some(command.rect.bounding_box()),
                _ => None,
            })
            .expect("continued quote should paint an accent");
        assert_eq!(accent.y0, 10.0);
        assert_eq!(accent.y1, 190.0);
    }

    #[test]
    fn matching_quote_continuations_expose_a_scroll_bridge() {
        let spine = SpineItemId::new("chapter").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "quote".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "quote".into(),
                text_offset: 20,
            },
        };
        let page = |continued_before, continued_after| {
            DisplayListCompiler.compile(&PageLayout {
                viewport: LayoutViewport::new(200, 200).unwrap(),
                background: Rgba::BLACK,
                leading_gap: 0.0,
                items: vec![PageItem::Quote(QuotePlacement {
                    x: 20.0,
                    y: 10.0,
                    width: 160.0,
                    height: 180.0,
                    continued_before,
                    continued_after,
                    fill: Rgba {
                        alpha: 0,
                        ..Rgba::BLACK
                    },
                    accent: Rgba::BLACK,
                    sources: vec![source.clone()],
                })],
            })
        };

        let first = page(false, true);
        let second = page(true, false);
        assert_eq!(
            first.source_block_bounds(std::slice::from_ref(&source)),
            Some(Rect::new(20.0, 10.0, 180.0, 190.0))
        );
        let bridge = first
            .quote_bridge_to(&second)
            .expect("matching continuation slices should expose their accent style");
        assert_eq!(bridge.x, 26.0);
        assert_eq!(bridge.width, 4.0);
    }
    use rebook_publication::{
        FixedPageTextLayer, FixedPageTextRect, FixedPageTextSpan, SourceAnchor, SourceRange,
        SpineItemId,
    };

    #[test]
    fn multiline_highlight_geometry_overlaps_adjacent_antialiased_edges() {
        let path = source_range_highlight_path([
            Rect::new(10.0, 10.0, 80.0, 25.0),
            Rect::new(10.0, 25.0, 60.0, 40.0),
        ]);

        assert_eq!(path.bounding_box(), Rect::new(10.0, 9.5, 80.0, 40.5));
        assert_eq!(
            path.elements()
                .iter()
                .filter(|element| matches!(element, kurbo::PathEl::MoveTo(_)))
                .count(),
            2
        );
    }

    #[test]
    fn empty_page_still_has_a_background() {
        let page = PageLayout {
            viewport: LayoutViewport::new(320, 240).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: Vec::new(),
        };
        let list = DisplayListCompiler.compile(&page);
        assert_eq!(list.width(), 320);
        assert_eq!(list.height(), 240);
        assert_eq!(list.content_top(), None);
        assert_eq!(list.content_bottom(), None);
        assert_eq!(list.command_count(), 0);
    }

    #[test]
    fn multi_run_footnote_reference_compiles_to_one_icon_region() {
        let text: Arc<str> = "A【3】".into();
        let mut font_context = FontContext::new();
        let mut layout_context = LayoutContext::new();
        let mut builder =
            layout_context.ranged_builder(&mut font_context, text.as_ref(), 1.0, false);
        builder.push_default(StyleProperty::FontSize(18.0));
        builder.push_default(StyleProperty::Brush(TextBrush {
            color: Rgba::BLACK,
            underline: false,
            baseline: TextBaseline::Normal,
            footnote_reference: false,
            footnote_reference_group: 0,
        }));
        builder.push(
            StyleProperty::Brush(TextBrush {
                color: Rgba::BLACK,
                underline: false,
                baseline: TextBaseline::Superscript,
                footnote_reference: true,
                footnote_reference_group: 1,
            }),
            1..8,
        );
        // Force the semantic marker into several glyph runs independently of the
        // fonts installed on the test machine. All runs must still become one icon.
        builder.push(StyleProperty::FontSize(17.0), 4..5);
        let mut layout = builder.build(text.as_ref());
        layout.break_all_lines(Some(240.0));
        layout.align(Alignment::Start, AlignmentOptions::default());
        let source = SourceRange {
            start: SourceAnchor {
                spine: SpineItemId::new("chapter-1").unwrap(),
                node: "paragraph-1".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine: SpineItemId::new("chapter-1").unwrap(),
                node: "paragraph-1".into(),
                text_offset: 8,
            },
        };
        let page = PageLayout {
            viewport: LayoutViewport::new(320, 240).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Text(TextPlacement {
                layout: Arc::new(layout),
                text,
                source_text_start: 0,
                lines: 0..1,
                origin_x: 24.0,
                origin_y: 24.0,
                available_width: 240.0,
                source: Some(source.clone()),
                inline_images: Arc::from([]),
            })],
        };

        let list = DisplayListCompiler.compile(&page);
        assert_eq!(list.footnote_regions.len(), 1);
        assert_eq!(list.footnote_regions[0].source, source);
        let icon_bounds = list.footnote_regions[0].bounds;
        assert_eq!(
            list.footnote_source_at(icon_bounds.center().x as f32, icon_bounds.center().y as f32),
            Some(source.clone())
        );
        assert_eq!(list.footnote_source_at(0.0, 0.0), None);
        let painted_glyphs = list
            .commands
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::Glyphs(command) => Some(command.glyphs.len()),
                _ => None,
            })
            .sum::<usize>();
        assert_eq!(painted_glyphs, 1);
    }

    #[test]
    fn footnote_icon_uses_a_stable_optical_superscript_position() {
        let baseline = 40.0;
        let bounds = footnote_icon_bounds(24.0, baseline, 20.0);

        assert!((bounds.width() - 12.0).abs() < f64::EPSILON);
        assert!((bounds.height() - 12.0).abs() < f64::EPSILON);
        assert!((bounds.center().x - 24.0).abs() < f64::EPSILON);
        assert!((bounds.center().y - 31.84).abs() < 0.001);
        assert!(bounds.y1 < f64::from(baseline));
    }

    #[test]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the test uses small, bounded logical page coordinates"
    )]
    fn text_hits_and_source_ranges_round_trip_through_retained_geometry() {
        let text: Arc<str> = "hello world".into();
        let mut font_context = FontContext::new();
        let mut layout_context = LayoutContext::new();
        let mut builder =
            layout_context.ranged_builder(&mut font_context, text.as_ref(), 1.0, false);
        builder.push_default(StyleProperty::FontSize(18.0));
        builder.push_default(StyleProperty::Brush(TextBrush {
            color: Rgba::BLACK,
            underline: false,
            baseline: TextBaseline::Normal,
            footnote_reference: false,
            footnote_reference_group: 0,
        }));
        let mut layout = builder.build(text.as_ref());
        layout.break_all_lines(Some(240.0));
        layout.align(Alignment::Start, AlignmentOptions::default());
        let spine = SpineItemId::new("chapter-1").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "paragraph-1".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "paragraph-1".into(),
                text_offset: 11,
            },
        };
        let page = PageLayout {
            viewport: LayoutViewport::new(320, 240).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Text(TextPlacement {
                layout: Arc::new(layout),
                text,
                source_text_start: 0,
                lines: 0..1,
                origin_x: 24.0,
                origin_y: 24.0,
                available_width: 240.0,
                source: Some(source.clone()),
                inline_images: Arc::from([]),
            })],
        };
        let list = DisplayListCompiler.compile(&page);
        assert!(
            list.content_bottom()
                .is_some_and(|bottom| bottom > 24.0 && bottom < 80.0),
            "text content should end near its shaped line rather than the 240px page boundary"
        );
        let text_bounds = list
            .source_rects(std::slice::from_ref(&source))
            .into_iter()
            .reduce(|bounds, next| bounds.union(next))
            .unwrap();
        assert_eq!(
            list.source_block_bounds(std::slice::from_ref(&source)),
            Some(Rect::new(
                16.0,
                text_bounds.y0 - 6.0,
                272.0,
                text_bounds.y1 + 6.0,
            ))
        );
        let selected_source = SourceRange {
            start: SourceAnchor {
                spine: SpineItemId::new("chapter-1").unwrap(),
                node: "paragraph-1".into(),
                text_offset: 1,
            },
            end: SourceAnchor {
                spine: SpineItemId::new("chapter-1").unwrap(),
                node: "paragraph-1".into(),
                text_offset: 5,
            },
        };
        let rects = list.source_rects(std::slice::from_ref(&selected_source));
        assert!(!rects.is_empty());
        let point = rects[0].center();
        assert!(
            list.hit_test_text(point.x as f32, point.y as f32, true)
                .is_some()
        );

        let fragment = list.selection_fragment(0, 1..5).unwrap();
        assert_eq!(fragment.quote, "ello");
        assert_eq!(fragment.range, selected_source);
        assert!(!fragment.rects.is_empty());
    }

    #[test]
    fn translated_text_maps_selection_offsets_to_the_original_source_span() {
        let text: Arc<str> = "这是较短的完整译文".into();
        let mut font_context = FontContext::new();
        let mut layout_context = LayoutContext::new();
        let mut builder =
            layout_context.ranged_builder(&mut font_context, text.as_ref(), 1.0, false);
        builder.push_default(StyleProperty::FontSize(18.0));
        builder.push_default(StyleProperty::Brush(TextBrush {
            color: Rgba::BLACK,
            underline: false,
            baseline: TextBaseline::Normal,
            footnote_reference: false,
            footnote_reference_group: 0,
        }));
        let mut layout = builder.build(text.as_ref());
        layout.break_all_lines(Some(240.0));
        layout.align(Alignment::Start, AlignmentOptions::default());
        let spine = SpineItemId::new("chapter-1").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "paragraph-1".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "paragraph-1".into(),
                text_offset: 224,
            },
        };
        let line_count = layout.len();
        let page = PageLayout {
            viewport: LayoutViewport::new(320, 240).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Text(TextPlacement {
                layout: Arc::new(layout),
                text: Arc::clone(&text),
                source_text_start: 0,
                lines: 0..line_count,
                origin_x: 24.0,
                origin_y: 24.0,
                available_width: 240.0,
                source: Some(source.clone()),
                inline_images: Arc::from([]),
            })],
        };
        let list = DisplayListCompiler.compile(&page);

        let selection = list.selection_fragment(0, 0..text.len()).unwrap();
        assert_eq!(selection.range, source);
        assert!(!list.source_rects(std::slice::from_ref(&source)).is_empty());
    }

    #[test]
    fn trailing_inline_only_line_does_not_hide_preceding_source_text() {
        let text: Arc<str> =
            "Of course it is necessary that the letters be beautiful. Given a sequence".into();
        let mut font_context = FontContext::new();
        let mut layout_context = LayoutContext::new();
        let mut builder =
            layout_context.ranged_builder(&mut font_context, text.as_ref(), 1.0, false);
        builder.push_default(StyleProperty::FontSize(18.0));
        builder.push_default(StyleProperty::Brush(TextBrush {
            color: Rgba::BLACK,
            underline: false,
            baseline: TextBaseline::Normal,
            footnote_reference: false,
            footnote_reference_group: 0,
        }));
        builder.push_inline_box(parley::InlineBox {
            id: 1,
            kind: parley::InlineBoxKind::InFlow,
            index: text.len(),
            width: 220.0,
            height: 60.0,
        });
        let mut layout = builder.build(text.as_ref());
        layout.break_all_lines(Some(160.0));
        layout.align(Alignment::Start, AlignmentOptions::default());
        let line_count = layout.len();
        let trailing_range = layout
            .get(line_count - 1)
            .expect("inline formula line")
            .text_range();
        assert!(
            trailing_range.end <= trailing_range.start,
            "the regression needs an inline-only trailing line"
        );

        let spine = SpineItemId::new("chapter-1").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "paragraph-with-formula".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "paragraph-with-formula".into(),
                text_offset: u64::try_from(text.chars().count()).unwrap(),
            },
        };
        let page = PageLayout {
            viewport: LayoutViewport::new(320, 360).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Text(TextPlacement {
                layout: Arc::new(layout),
                text,
                source_text_start: 0,
                lines: 0..line_count,
                origin_x: 24.0,
                origin_y: 24.0,
                available_width: 160.0,
                source: Some(source.clone()),
                inline_images: Arc::from([InlineImage {
                    id: 1,
                    image: RasterImage {
                        width: 1,
                        height: 1,
                        pixels: Arc::from([0_u8, 0, 0, 255]),
                    },
                    width: 220.0,
                    height: 60.0,
                    offset_y: 0.0,
                }]),
            })],
        };
        let list = DisplayListCompiler.compile(&page);

        let rects = list.source_rects(std::slice::from_ref(&source));
        assert!(!rects.is_empty());
        assert!(
            rects
                .iter()
                .any(|rect| rect.width() == 220.0 && rect.height() == 60.0),
            "the complete semantic block should include its trailing formula"
        );
    }

    #[test]
    fn pure_formula_with_empty_source_span_has_selectable_geometry() {
        let text: Arc<str> = "".into();
        let mut font_context = FontContext::new();
        let mut layout_context = LayoutContext::new();
        let mut builder =
            layout_context.ranged_builder(&mut font_context, text.as_ref(), 1.0, false);
        builder.push_default(StyleProperty::FontSize(18.0));
        builder.push_default(StyleProperty::Brush(TextBrush {
            color: Rgba::BLACK,
            underline: false,
            baseline: TextBaseline::Normal,
            footnote_reference: false,
            footnote_reference_group: 0,
        }));
        builder.push_inline_box(parley::InlineBox {
            id: 7,
            kind: parley::InlineBoxKind::InFlow,
            index: 0,
            width: 180.0,
            height: 44.0,
        });
        let mut layout = builder.build(text.as_ref());
        layout.break_all_lines(Some(240.0));
        layout.align(Alignment::Start, AlignmentOptions::default());
        let line_count = layout.len();
        let spine = SpineItemId::new("chapter-1").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "formula-only".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "formula-only".into(),
                text_offset: 0,
            },
        };
        let page = PageLayout {
            viewport: LayoutViewport::new(320, 180).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Text(TextPlacement {
                layout: Arc::new(layout),
                text,
                source_text_start: 0,
                lines: 0..line_count,
                origin_x: 24.0,
                origin_y: 24.0,
                available_width: 240.0,
                source: Some(source.clone()),
                inline_images: Arc::from([InlineImage {
                    id: 7,
                    image: RasterImage {
                        width: 1,
                        height: 1,
                        pixels: Arc::from([0_u8, 0, 0, 255]),
                    },
                    width: 180.0,
                    height: 44.0,
                    offset_y: 0.0,
                }]),
            })],
        };
        let list = DisplayListCompiler.compile(&page);

        let rects = list.source_rects(std::slice::from_ref(&source));
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].width(), 180.0);
        assert_eq!(rects[0].height(), 44.0);
    }

    #[test]
    fn selection_covers_the_visual_width_of_justified_middle_lines() {
        let text: Arc<str> =
            "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu".into();
        let mut font_context = FontContext::new();
        let mut layout_context = LayoutContext::new();
        let mut builder =
            layout_context.ranged_builder(&mut font_context, text.as_ref(), 1.0, false);
        builder.push_default(StyleProperty::FontSize(18.0));
        builder.push_default(StyleProperty::Brush(TextBrush {
            color: Rgba::BLACK,
            underline: false,
            baseline: TextBaseline::Normal,
            footnote_reference: false,
            footnote_reference_group: 0,
        }));
        let mut layout = builder.build(text.as_ref());
        layout.break_all_lines(Some(150.0));
        layout.align(Alignment::Justify, AlignmentOptions::default());

        let line_y = layout
            .lines()
            .skip(1)
            .take(layout.len().saturating_sub(2))
            .find_map(|line| {
                let visual_advance = line
                    .runs()
                    .map(|run| {
                        run.visual_clusters()
                            .map(|cluster| cluster.advance())
                            .sum::<f32>()
                    })
                    .sum::<f32>();
                (visual_advance > line.metrics().advance + 1.0)
                    .then_some(line.metrics().block_min_coord)
            })
            .expect("the fixture should contain a justified middle line");
        let expected_right = shared_wrapped_content_end(&layout);

        let spine = SpineItemId::new("chapter-1").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "paragraph-1".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "paragraph-1".into(),
                text_offset: u64::try_from(text.chars().count()).unwrap(),
            },
        };
        let line_count = layout.len();
        let origin_x = 24.0;
        let origin_y = 24.0;
        let page = PageLayout {
            viewport: LayoutViewport::new(320, 240).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Text(TextPlacement {
                layout: Arc::new(layout),
                text: Arc::clone(&text),
                source_text_start: 0,
                lines: 0..line_count,
                origin_x,
                origin_y,
                available_width: 240.0,
                source: Some(source),
                inline_images: Arc::from([]),
            })],
        };

        let fragment = DisplayListCompiler
            .compile(&page)
            .selection_fragment(0, 0..text.len())
            .unwrap();
        let rect = fragment
            .rects
            .iter()
            .find(|rect| (rect.y0 - f64::from(line_y + origin_y)).abs() < 0.01)
            .expect("the justified line should have selection geometry");

        assert!((rect.x1 - f64::from(expected_right + origin_x)).abs() < 0.01);
    }

    #[test]
    fn hanging_indent_highlight_aligns_first_and_continuation_line_right_edges() {
        let text: Arc<str> =
            "•\u{00a0}alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu".into();
        let mut font_context = FontContext::new();
        let mut layout_context = LayoutContext::new();
        let mut builder =
            layout_context.ranged_builder(&mut font_context, text.as_ref(), 1.0, false);
        builder.push_default(StyleProperty::FontSize(18.0));
        builder.push_default(StyleProperty::Brush(TextBrush {
            color: Rgba::BLACK,
            underline: false,
            baseline: TextBaseline::Normal,
            footnote_reference: false,
            footnote_reference_group: 0,
        }));
        let mut layout = builder.build(text.as_ref());
        layout.break_all_lines(None);
        let source_start = "•\u{00a0}".len();
        let initial = Selection::new(
            Cursor::from_byte_index(&layout, source_start, Affinity::Downstream),
            Cursor::from_byte_index(&layout, text.len(), Affinity::Upstream),
        )
        .geometry(&layout);
        let text_inset = initial[0].0.x0 as f32 - 0.5;
        layout.set_text_indent(
            text_inset,
            parley::IndentOptions {
                hanging: true,
                ..parley::IndentOptions::default()
            },
        );
        layout.break_all_lines(Some(150.0));
        layout.align(Alignment::Justify, AlignmentOptions::default());
        let first = layout.get(0).expect("fixture should have a first line");
        let continuation = layout.get(1).expect("fixture should wrap");
        let expected_right = shared_wrapped_content_end(&layout);
        let first_y = first.metrics().block_min_coord;
        let continuation_y = continuation.metrics().block_min_coord;

        let spine = SpineItemId::new("chapter-1").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "list-item".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "list-item".into(),
                text_offset: u64::try_from(text.chars().count()).unwrap(),
            },
        };
        let line_count = layout.len();
        let page = PageLayout {
            viewport: LayoutViewport::new(240, 240).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Text(TextPlacement {
                layout: Arc::new(layout),
                text: Arc::clone(&text),
                source_text_start: "•\u{00a0}".len(),
                lines: 0..line_count,
                origin_x: 24.0,
                origin_y: 24.0,
                available_width: 180.0,
                source: Some(source),
                inline_images: Arc::from([]),
            })],
        };
        let fragment = DisplayListCompiler
            .compile(&page)
            .selection_fragment(0, "•\u{00a0}".len()..text.len())
            .unwrap();
        let first_rect = fragment
            .rects
            .iter()
            .find(|rect| (rect.y0 - f64::from(first_y + 24.0)).abs() < 0.01)
            .unwrap();
        let continuation_rect = fragment
            .rects
            .iter()
            .find(|rect| (rect.y0 - f64::from(continuation_y + 24.0)).abs() < 0.01)
            .unwrap();
        assert!((first_rect.x1 - f64::from(expected_right + 24.0)).abs() < 0.01);
        assert!(
            first_rect.x0 > 24.0,
            "synthetic list marker must remain outside the source-backed highlight"
        );
        assert!(
            (first_rect.x0 - continuation_rect.x0).abs() < 0.01,
            "the first line must share the hanging inset without including the marker"
        );
        assert!((continuation_rect.x1 - f64::from(expected_right + 24.0)).abs() < 0.01);
    }

    #[test]
    fn wrapped_mixed_text_uses_line_width_while_the_last_line_stays_content_sized() {
        let text: Arc<str> =
            "中文 FitText mixed content with several English words and 中文结尾".into();
        let mut font_context = FontContext::new();
        let mut layout_context = LayoutContext::new();
        let mut builder =
            layout_context.ranged_builder(&mut font_context, text.as_ref(), 1.0, false);
        builder.push_default(StyleProperty::FontSize(18.0));
        builder.push_default(StyleProperty::Brush(TextBrush {
            color: Rgba::BLACK,
            underline: false,
            baseline: TextBaseline::Normal,
            footnote_reference: false,
            footnote_reference_group: 0,
        }));
        let mut layout = builder.build(text.as_ref());
        layout.break_all_lines(Some(180.0));
        layout.align(Alignment::Justify, AlignmentOptions::default());
        assert!(layout.len() >= 2);

        let first = layout.get(0).unwrap();
        let last = layout.get(layout.len() - 1).unwrap();
        assert!(matches!(first.break_reason(), BreakReason::Regular));
        assert_eq!(last.break_reason(), BreakReason::None);
        let expected_wrapped_right = shared_wrapped_content_end(&layout);
        let last_content_right = last.metrics().offset
            + last.metrics().inline_min_coord
            + last
                .runs()
                .map(|run| {
                    run.visual_clusters()
                        .map(|cluster| cluster.advance())
                        .sum::<f32>()
                })
                .sum::<f32>();
        let expected_last_limit = last.metrics().inline_max_coord;

        let spine = SpineItemId::new("chapter-1").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "mixed-paragraph".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "mixed-paragraph".into(),
                text_offset: u64::try_from(text.chars().count()).unwrap(),
            },
        };
        let first_y = first.metrics().block_min_coord;
        let last_y = last.metrics().block_min_coord;
        let line_count = layout.len();
        let origin_x = 24.0;
        let origin_y = 24.0;
        let page = PageLayout {
            viewport: LayoutViewport::new(320, 300).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Text(TextPlacement {
                layout: Arc::new(layout),
                text: Arc::clone(&text),
                source_text_start: 0,
                lines: 0..line_count,
                origin_x,
                origin_y,
                available_width: 240.0,
                source: Some(source),
                inline_images: Arc::from([]),
            })],
        };

        let fragment = DisplayListCompiler
            .compile(&page)
            .selection_fragment(0, 0..text.len())
            .unwrap();
        let wrapped_rect = fragment
            .rects
            .iter()
            .find(|rect| (rect.y0 - f64::from(first_y + origin_y)).abs() < 0.01)
            .unwrap();
        let last_rect = fragment
            .rects
            .iter()
            .find(|rect| (rect.y0 - f64::from(last_y + origin_y)).abs() < 0.01)
            .unwrap();

        assert!(
            (wrapped_rect.x1 - f64::from(expected_wrapped_right + origin_x)).abs() < 0.01,
            "wrapped right={} expected={}",
            wrapped_rect.x1,
            expected_wrapped_right + origin_x
        );
        assert!((last_rect.x1 - f64::from(last_content_right + origin_x)).abs() < 0.01);
        assert!(last_rect.x1 < f64::from(expected_last_limit + origin_x));
    }

    #[test]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the test uses small, bounded logical page coordinates"
    )]
    fn fixed_page_text_geometry_supports_hit_testing_and_source_ranges() {
        let spine = SpineItemId::new("pdf-page-1").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "pdf-page-text".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "pdf-page-text".into(),
                text_offset: 3,
            },
        };
        let spans = (0_u16..3)
            .map(|index| FixedPageTextSpan {
                char_range: u64::from(index)..u64::from(index + 1),
                rect: FixedPageTextRect {
                    x: 10.0 + f32::from(index) * 10.0,
                    y: 20.0,
                    width: 9.0,
                    height: 12.0,
                },
            })
            .collect();
        let page = PageLayout {
            viewport: LayoutViewport::new(200, 200).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Image(ImagePlacement {
                image: RasterImage {
                    width: 100,
                    height: 100,
                    pixels: vec![255; 100 * 100 * 4].into(),
                },
                x: 50.0,
                y: 40.0,
                width: 100.0,
                height: 100.0,
                source: Some(source.clone()),
                text_layer: Some(FixedPageTextLayer {
                    width: 100.0,
                    height: 100.0,
                    text: "PDF".into(),
                    spans,
                    replacement: None,
                }),
                replacement: None,
            })],
        };

        let list = DisplayListCompiler.compile(&page);
        assert_eq!(list.text_region_count(), 1);
        let image_data = list.image_data().collect::<Vec<_>>();
        assert_eq!(image_data.len(), 1);
        assert_eq!((image_data[0].width, image_data[0].height), (100, 100));
        assert_eq!(image_data[0].data.len(), 100 * 100 * 4);
        assert_eq!(
            list.image_bounds(),
            Some(Rect::new(50.0, 40.0, 150.0, 140.0))
        );
        assert_eq!(
            list.image_source_rects(std::slice::from_ref(&source)),
            [Rect::new(50.0, 40.0, 150.0, 140.0)]
        );
        let rects = list.source_rects(std::slice::from_ref(&source));
        assert_eq!(rects.len(), 1);
        let point = rects[0].center();
        let image = list
            .image_at(point.x as f32, point.y as f32)
            .expect("fixed image should be hit-testable");
        assert_eq!((image.width, image.height), (100, 100));
        assert_eq!(image.pixels.len(), 100 * 100 * 4);
        let hit = list
            .hit_test_text(point.x as f32, point.y as f32, true)
            .expect("fixed text should be hit-testable");
        let fragment = list.selection_fragment(hit.region_index, 0..3).unwrap();
        assert_eq!(fragment.quote, "PDF");
        assert_eq!(fragment.range, source);
    }

    #[test]
    fn fixed_page_replacement_compiles_image_mask_and_translated_glyphs() {
        let text: Arc<str> = "译文".into();
        let mut font_context = FontContext::new();
        let mut layout_context = LayoutContext::new();
        let mut builder =
            layout_context.ranged_builder(&mut font_context, text.as_ref(), 1.0, false);
        builder.push_default(StyleProperty::FontSize(14.0));
        builder.push_default(StyleProperty::Brush(TextBrush {
            color: Rgba::BLACK,
            underline: false,
            baseline: TextBaseline::Normal,
            footnote_reference: false,
            footnote_reference_group: 0,
        }));
        let mut layout = builder.build(text.as_ref());
        layout.break_all_lines(Some(80.0));
        layout.align(Alignment::Start, AlignmentOptions::default());
        let line_count = layout.len();
        let spine = SpineItemId::new("pdf-page-1").unwrap();
        let source = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "pdf-page-text".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "pdf-page-text".into(),
                text_offset: 2,
            },
        };
        let page = PageLayout {
            viewport: LayoutViewport::new(200, 200).unwrap(),
            background: Rgba::BLACK,
            leading_gap: 0.0,
            items: vec![PageItem::Image(ImagePlacement {
                image: RasterImage {
                    width: 100,
                    height: 100,
                    pixels: [200, 201, 202, 255].repeat(100 * 100).into(),
                },
                x: 50.0,
                y: 40.0,
                width: 100.0,
                height: 100.0,
                source: Some(source.clone()),
                text_layer: None,
                replacement: Some(FixedPageTextReplacementPlacement {
                    segments: vec![FixedPageTextReplacementSegmentPlacement {
                        rect: FixedPageTextRect {
                            x: 60.0,
                            y: 60.0,
                            width: 80.0,
                            height: 30.0,
                        },
                        text: TextPlacement {
                            layout: Arc::new(layout),
                            text,
                            source_text_start: 0,
                            lines: 0..line_count,
                            origin_x: 64.0,
                            origin_y: 64.0,
                            available_width: 76.0,
                            source: Some(source.clone()),
                            inline_images: Arc::from([]),
                        },
                    }],
                }),
            })],
        };

        let list = DisplayListCompiler.compile(&page);

        assert!(matches!(
            list.commands.first(),
            Some(DisplayCommand::Image(_))
        ));
        assert!(matches!(
            list.commands.get(1),
            Some(DisplayCommand::FillRect(_))
        ));
        let Some(DisplayCommand::FillRect(mask)) = list.commands.get(1) else {
            unreachable!();
        };
        assert_eq!(mask.color, Color::from_rgba8(200, 201, 202, 255));
        assert!(
            list.commands
                .iter()
                .skip(2)
                .any(|command| matches!(command, DisplayCommand::Glyphs(_)))
        );
        assert_eq!(list.text_region_count(), 1);
        let fragment = list.selection_fragment(0, 0.."译文".len()).unwrap();
        assert_eq!(fragment.quote, "译文");
        assert_eq!(fragment.range, source);
    }
