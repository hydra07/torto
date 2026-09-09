    use super::*;

    #[test]
    fn unified_semantic_styles_follow_each_mixed_script_span() {
        let emphasis = TextStyle {
            italic: true,
            emphasis: true,
            ..TextStyle::default()
        };
        let citation = TextStyle {
            italic: true,
            citation: true,
            ..TextStyle::default()
        };
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![
                Inline::Text(TextRun {
                    text: "重点 important 结论".into(),
                    style: emphasis,
                    link: None,
                }),
                Inline::Text(TextRun {
                    text: "《Rolling Stone》杂志".into(),
                    style: citation,
                    link: None,
                }),
            ],
            style: BlockStyle::default(),
            source: None,
        };
        let reader_style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            writing_system: WritingSystem::Cjk,
            ..ReaderStyle::default()
        };

        let resolved = resolve_text_block(&block, &reader_style, TextContext::Flow);
        let runs = resolved
            .content
            .iter()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            runs.iter()
                .any(|run| { run.text.contains("重点") && run.style.bold && !run.style.italic })
        );
        assert!(
            runs.iter().any(|run| {
                run.text.contains("important") && !run.style.bold && run.style.italic
            })
        );
        assert!(
            runs.iter()
                .any(|run| { run.text.contains("结论") && run.style.bold && !run.style.italic })
        );
        assert!(runs.iter().any(|run| {
            run.text.contains("Rolling Stone") && run.style.citation && run.style.italic
        }));
        assert!(
            runs.iter().any(|run| {
                run.text.contains("杂志") && run.style.citation && !run.style.italic
            })
        );
    }

    #[test]
    fn book_typesetting_keeps_authored_semantic_tag_presentation() {
        let style = TextStyle {
            italic: true,
            emphasis: true,
            ..TextStyle::default()
        };
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: "中文强调".into(),
                style,
                link: None,
            })],
            style: BlockStyle::default(),
            source: None,
        };

        let resolved = resolve_text_block(&block, &ReaderStyle::default(), TextContext::Flow);

        assert!(matches!(resolved, Cow::Borrowed(_)));
        let Inline::Text(run) = &resolved.content[0] else {
            panic!("expected text run");
        };
        assert!(run.style.italic);
        assert!(!run.style.bold);
    }

    #[test]
    fn focus_layout_omits_semantic_footnote_definitions_from_the_main_flow() {
        let block = Block::Text(TextBlock {
            kind: TextBlockKind::FootnoteDefinition,
            content: vec![Inline::Text(TextRun {
                text: "Footnote body".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: Default::default(),
            source: None,
        });

        assert!(should_layout_flow_block(&block, &ReaderStyle::default()));
        assert!(!should_layout_flow_block(
            &block,
            &ReaderStyle {
                focus_footnote_icons: true,
                ..ReaderStyle::default()
            }
        ));
    }

    #[test]
    fn unified_layout_hides_note_sections_while_book_layout_keeps_them() {
        let body = Block::Text(TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: "Endnote body".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: Default::default(),
            source: None,
        });
        let section_note = Block::Note(rebook_publication::NoteBlock {
            kind: NoteBlockKind::Section,
            blocks: vec![body.clone()],
            source: None,
        });
        let definition_note = Block::Note(rebook_publication::NoteBlock {
            kind: NoteBlockKind::Definition,
            blocks: vec![body],
            source: None,
        });

        let mut output = Vec::new();
        collect_layout_blocks(
            std::slice::from_ref(&section_note),
            &ReaderStyle::default(),
            &mut output,
        );
        assert_eq!(output.len(), 1);

        output.clear();
        collect_layout_blocks(
            std::slice::from_ref(&section_note),
            &ReaderStyle {
                typesetting: ReaderTypesetting::unified(),
                ..ReaderStyle::default()
            },
            &mut output,
        );
        assert!(output.is_empty());

        output.clear();
        collect_layout_blocks(
            std::slice::from_ref(&definition_note),
            &ReaderStyle {
                focus_footnote_icons: true,
                ..ReaderStyle::default()
            },
            &mut output,
        );
        assert!(output.is_empty());
    }

    #[test]
    fn focus_footnote_icons_mark_semantic_references_and_legacy_linked_superscripts() {
        let linked_superscript = TextRun {
            text: "1".into(),
            style: TextStyle {
                baseline: TextBaseline::Superscript,
                ..TextStyle::default()
            },
            link: Some(rebook_publication::PublicationUrl::parse("notes.xhtml#note-1").unwrap()),
        };
        let unlinked_superscript = TextRun {
            text: "2".into(),
            style: linked_superscript.style,
            link: None,
        };
        let linked_baseline_text = TextRun {
            text: "3".into(),
            style: TextStyle::default(),
            link: linked_superscript.link.clone(),
        };
        let semantic_baseline_reference = TextRun {
            text: "[4]".into(),
            style: TextStyle {
                link_role: LinkRole::FootnoteReference,
                ..TextStyle::default()
            },
            link: linked_superscript.link.clone(),
        };
        let superscript_backlink = TextRun {
            text: "[5]".into(),
            style: TextStyle {
                baseline: TextBaseline::Superscript,
                link_role: LinkRole::FootnoteBacklink,
                ..TextStyle::default()
            },
            link: Some(rebook_publication::PublicationUrl::parse("chapter.xhtml#ref-5").unwrap()),
        };
        let inline_footnote = TextRun {
            text: "inline note".into(),
            style: TextStyle {
                inline_role: InlineRole::Footnote,
                ..TextStyle::default()
            },
            link: None,
        };
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![
                Inline::Text(linked_superscript),
                Inline::Text(unlinked_superscript),
                Inline::Text(linked_baseline_text),
                Inline::Text(semantic_baseline_reference),
                Inline::Text(superscript_backlink),
                Inline::Text(inline_footnote),
            ],
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let svg_options = resvg::usvg::Options::default();

        let (focus_text, spans, _, _) = prepare_inline_content(
            &block,
            Rgba::BLACK,
            &ReaderTypography::default(),
            320.0,
            &svg_options,
            true,
            &[],
        );
        assert_eq!(
            spans
                .iter()
                .map(|span| span.footnote_reference_group != 0)
                .collect::<Vec<_>>(),
            [true, false, false, true, false, true]
        );
        assert_eq!(
            focus_text,
            format!(
                "023{}[5]{}",
                footnote_icon_placeholder("[4]"),
                footnote_icon_placeholder("inline note")
            )
        );

        let (classic_text, disabled_spans, _, _) = prepare_inline_content(
            &block,
            Rgba::BLACK,
            &ReaderTypography::default(),
            320.0,
            &svg_options,
            false,
            &[],
        );
        assert!(
            disabled_spans
                .iter()
                .all(|span| span.footnote_reference_group == 0)
        );
        assert_eq!(classic_text, "123[4][5]inline note");
    }

    #[test]
    fn prepares_semantic_inline_images_at_their_text_position() {
        let block = TextBlock {
            kind: TextBlockKind::Heading(1),
            content: vec![
                Inline::Image(Box::new(rebook_publication::InlineImageRun {
                    image: ImageBlock {
                        href: PublicationUrl::parse("images/chapter-icon.jpg").unwrap(),
                        alt: String::new(),
                        style: ImageStyle::default(),
                        source: None,
                        text_layer: None,
                    },
                    size_scale: 1.0,
                    intrinsic_sizing: false,
                    vertical_align: InlineImageAlignment::Middle,
                    presentation: true,
                })),
                Inline::Text(TextRun {
                    text: "Chapter title".into(),
                    style: TextStyle::default(),
                    link: None,
                }),
            ],
            style: BlockStyle::default(),
            source: None,
        };
        let raster = RasterImage {
            width: 200,
            height: 100,
            pixels: vec![0; 200 * 100 * 4].into(),
        };
        let typography = ReaderTypography::default();
        let svg_options = resvg::usvg::Options::default();

        let (text, _, images, _) = prepare_inline_content(
            &block,
            Rgba::BLACK,
            &typography,
            320.0,
            &svg_options,
            false,
            &[Some(raster), None],
        );

        assert_eq!(text, "Chapter title");
        let [image] = images.as_slice() else {
            panic!("expected one prepared inline image");
        };
        assert_eq!(image.index, 0);
        assert!((image.height - typography.font_size).abs() < f32::EPSILON);
        assert!((image.width - typography.font_size * 2.0).abs() < f32::EPSILON);
        assert!(image.offset_y > 0.0);
    }

    #[test]
    fn scales_unstyled_inline_images_from_intrinsic_css_pixels() {
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Image(Box::new(
                rebook_publication::InlineImageRun {
                    image: ImageBlock {
                        href: PublicationUrl::parse("images/pi.jpg").unwrap(),
                        alt: "Image".into(),
                        style: ImageStyle::default(),
                        source: None,
                        text_layer: None,
                    },
                    size_scale: 1.0,
                    intrinsic_sizing: true,
                    vertical_align: InlineImageAlignment::Middle,
                    presentation: false,
                },
            ))],
            style: BlockStyle::default(),
            source: None,
        };
        let raster = RasterImage {
            width: 12,
            height: 12,
            pixels: vec![0; 12 * 12 * 4].into(),
        };
        let typography = ReaderTypography::default();
        let svg_options = resvg::usvg::Options::default();

        let (_, _, images, _) = prepare_inline_content(
            &block,
            Rgba::BLACK,
            &typography,
            320.0,
            &svg_options,
            false,
            &[Some(raster)],
        );

        let [image] = images.as_slice() else {
            panic!("expected one prepared inline image");
        };
        let expected = typography.font_size * 12.0 / 16.0;
        assert!((image.height - expected).abs() < f32::EPSILON);
        assert!((image.width - expected).abs() < f32::EPSILON);
        assert!(image.offset_y > 0.0);
        assert!(image.offset_y < image.height * 0.5);
        let visual_center_from_baseline = -image.box_height + image.offset_y + image.height * 0.5;
        assert!((visual_center_from_baseline + typography.font_size * 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn large_unstyled_inline_illustrations_keep_intrinsic_aspect_ratio() {
        let run = rebook_publication::InlineImageRun {
            image: ImageBlock {
                href: PublicationUrl::parse("images/illustration.jpg").unwrap(),
                alt: "Chapter illustration".into(),
                style: ImageStyle::default(),
                source: None,
                text_layer: None,
            },
            size_scale: 1.0,
            intrinsic_sizing: true,
            vertical_align: InlineImageAlignment::Baseline,
            presentation: false,
        };
        let image = prepare_inline_raster(
            &run,
            RasterImage {
                width: 240,
                height: 320,
                pixels: vec![0; 240 * 320 * 4].into(),
            },
            &ReaderTypography::default(),
            300.0,
            1,
            0,
        );

        assert!((image.width - 240.0).abs() < 0.001);
        assert!((image.height - 320.0).abs() < 0.001);
        assert!((image.width / image.height - 0.75).abs() < 0.001);
    }

    #[test]
    fn quote_accent_tracks_the_reader_foreground_theme() {
        assert_eq!(
            quote_accent_for_foreground(Rgba::BLACK),
            LIGHT_QUOTE_ACCENT_COLOR
        );
        assert_eq!(
            quote_accent_for_foreground(Rgba {
                red: 232,
                green: 230,
                blue: 225,
                alpha: 255,
            }),
            DARK_QUOTE_ACCENT_COLOR
        );
    }

    #[test]
    fn sentence_justification_preserves_subparagraph_indents_and_gaps() {
        let sentence = "威斯康星大学麦迪逊分校的比较心理学家哈里·哈洛（Harry Harlow）进行了一项臭名昭著的实验。";
        let run = Inline::Text(TextRun {
            text: sentence.into(),
            style: TextStyle::default(),
            link: None,
        });
        let mut block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![run.clone(), Inline::Break, Inline::Break, run],
            style: rebook_publication::BlockStyle {
                indent: 32.0,
                subparagraph_gap_em: Some(0.3),
                ..rebook_publication::BlockStyle::default()
            },
            source: None,
        };
        let style = ReaderStyle::default();
        let mut engine = LayoutEngine::new();
        let natural = engine.shape_text(&block, &style, 400.0);
        block.style.align = TextAlignment::Justify;
        let justified = engine.shape_text(&block, &style, 400.0);
        assert_eq!(natural.layout.len(), justified.layout.len());
        assert_eq!(natural.text, justified.text);
        for (before, after) in natural.layout.lines().zip(justified.layout.lines()) {
            assert_eq!(before.text_range(), after.text_range());
            assert_eq!(before.break_reason(), after.break_reason());
            assert!((before.metrics().offset - after.metrics().offset).abs() < 0.01);
            assert!((before.metrics().line_height - after.metrics().line_height).abs() < 0.01);
            if matches!(
                after.break_reason(),
                parley::layout::BreakReason::None | parley::layout::BreakReason::Explicit
            ) {
                assert!((before.metrics().advance - after.metrics().advance).abs() < 0.01);
            } else {
                assert!(
                    (linebreak::parley::positioned_line_content_end(after) - 400.0).abs() < 0.1
                );
            }
        }
    }

    #[test]
    fn semantic_subparagraph_break_uses_the_compact_configured_gap() {
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![
                Inline::Text(TextRun {
                    text: "First sentence.".into(),
                    style: TextStyle::default(),
                    link: None,
                }),
                Inline::Break,
                Inline::Break,
                Inline::Text(TextRun {
                    text: "Second sentence.".into(),
                    style: TextStyle::default(),
                    link: None,
                }),
            ],
            style: rebook_publication::BlockStyle {
                indent: 24.0,
                line_height: 1.5,
                subparagraph_gap_em: Some(0.3),
                ..rebook_publication::BlockStyle::default()
            },
            source: None,
        };
        let style = ReaderStyle::default();
        let prepared = LayoutEngine::new().shape_text(&block, &style, 400.0);

        assert_eq!(prepared.layout.len(), 2);
        assert!((prepared.layout.get(0).unwrap().metrics().offset - 24.0).abs() < 0.01);
        assert!((prepared.layout.get(1).unwrap().metrics().offset - 24.0).abs() < 0.01);
        let body_line_height = prepared.layout.get(0).unwrap().metrics().line_height;
        let next_line_height = prepared.layout.get(1).unwrap().metrics().line_height;
        assert!(
            (body_line_height - style.typography.font_size * 1.5).abs() < 0.5,
            "expected the preceding line to keep the 1.5em body line height, got {body_line_height}"
        );
        assert!(
            (next_line_height - style.typography.font_size * 1.8).abs() < 0.5,
            "expected 1.5em line height plus a 0.3em subparagraph gap, got {next_line_height}"
        );
    }

    #[test]
    fn unified_typesetting_replaces_authored_heading_metrics() {
        let block = TextBlock {
            kind: TextBlockKind::Heading(2),
            content: vec![Inline::Text(TextRun {
                text: "Heading".into(),
                style: TextStyle {
                    size_scale: 2.8,
                    italic: true,
                    ..TextStyle::default()
                },
                link: None,
            })],
            style: rebook_publication::BlockStyle {
                align: TextAlignment::Center,
                authored_alignment: Some(TextAlignment::Center),
                margin_before: 40.0,
                margin_after: 50.0,
                indent: 20.0,
                line_height: 0.8,
                ..rebook_publication::BlockStyle::default()
            },
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let resolved = resolve_text_block(&block, &style, TextContext::Flow);
        assert_eq!(resolved.style.align, TextAlignment::Start);
        assert!(resolved.style.margin_before.abs() < 0.001);
        assert!((resolved.style.margin_after - 14.0).abs() < 0.001);
        assert!(resolved.style.indent.abs() < 0.001);
        assert!((resolved.style.line_height - 1.3).abs() < 0.001);
        let Inline::Text(run) = &resolved.content[0] else {
            panic!("expected text run");
        };
        assert!((run.style.size_scale - 1.432).abs() < 0.001);
        assert!(run.style.bold);
        assert!(!run.style.italic);
    }

    #[test]
    fn translated_prose_uses_display_language_line_height() {
        let mut block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: "译文 translated text".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: BlockStyle::default(),
            source: None,
        };
        let mut style = ReaderStyle {
            writing_system: WritingSystem::Latin,
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        assert!(
            (resolve_text_block(&block, &style, TextContext::Flow)
                .style
                .line_height
                - 1.4)
                .abs()
                < 0.001
        );
        if let Inline::Text(run) = &mut block.content[0] {
            run.style.display_writing_system = Some(WritingSystem::Cjk);
        }
        assert!(
            (resolve_text_block(&block, &style, TextContext::Flow)
                .style
                .line_height
                - 1.7)
                .abs()
                < 0.001
        );
        block.kind = TextBlockKind::Caption;
        assert!(
            (resolve_text_block(&block, &style, TextContext::Flow)
                .style
                .line_height
                - 1.4)
                .abs()
                < 0.001
        );
        style.typesetting.mode = TypesettingMode::Book;
        assert_eq!(
            resolve_text_block(&block, &style, TextContext::Flow).as_ref(),
            &block
        );
    }

    #[test]
    fn unified_split_heading_uses_a_compact_ordinal_before_the_title() {
        let text_block = |kind| TextBlock {
            kind,
            content: vec![Inline::Text(TextRun {
                text: "Heading".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        let ordinal = text_block(TextBlockKind::HeadingOrdinal(2));
        let title = text_block(TextBlockKind::Heading(2));

        let ordinal = resolve_text_block(&ordinal, &style, TextContext::Flow);
        let title = resolve_text_block(&title, &style, TextContext::Flow);
        let Inline::Text(ordinal_run) = &ordinal.content[0] else {
            panic!("expected ordinal text run");
        };
        let Inline::Text(title_run) = &title.content[0] else {
            panic!("expected title text run");
        };
        assert!(ordinal_run.style.size_scale < title_run.style.size_scale);
        assert!(ordinal.style.margin_after < title.style.margin_after);
        assert!(ordinal_run.style.bold);
        assert!(!ordinal_run.style.italic);
    }

    #[test]
    fn unified_captions_clear_all_authored_bold_and_italic_sources() {
        let block = TextBlock {
            kind: TextBlockKind::Caption,
            content: vec![Inline::Text(TextRun {
                text: "Figure 1. Authored emphasis and citation.".into(),
                style: TextStyle {
                    bold: true,
                    italic: true,
                    emphasis: true,
                    alternate_voice: true,
                    citation: true,
                    ..TextStyle::default()
                },
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let classic = ReaderStyle::default();
        let unified = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let classic = resolve_text_block(&block, &classic, TextContext::Flow);
        let unified = resolve_text_block(&block, &unified, TextContext::Flow);
        let Inline::Text(classic_run) = &classic.content[0] else {
            panic!("expected classic caption text");
        };
        let Inline::Text(unified_run) = &unified.content[0] else {
            panic!("expected unified caption text");
        };
        assert!(classic_run.style.bold);
        assert!(classic_run.style.italic);
        assert!(!unified_run.style.bold);
        assert!(!unified_run.style.italic);
        assert!(!unified_run.style.emphasis);
        assert!(!unified_run.style.alternate_voice);
        assert!(!unified_run.style.citation);
    }

    #[test]
    fn unified_typesetting_justifies_latin_paragraphs_and_list_items() {
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        for kind in [
            TextBlockKind::Paragraph,
            TextBlockKind::ListItem {
                ordered: false,
                ordinal: 1,
                depth: 0,
                marker_visible: true,
            },
        ] {
            let block = TextBlock {
                kind,
                content: vec![Inline::Text(TextRun {
                    text: "Unified prose".into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle {
                    align: TextAlignment::End,
                    ..rebook_publication::BlockStyle::default()
                },
                source: None,
            };

            let resolved = resolve_text_block(&block, &style, TextContext::Flow);
            assert_eq!(resolved.style.align, TextAlignment::Justify);
        }
    }

    #[test]
    fn unified_typesetting_ignores_authored_start_alignment_for_paragraphs() {
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        for (alignment, expected) in [
            (TextAlignment::Start, TextAlignment::Justify),
            (TextAlignment::Center, TextAlignment::Center),
            (TextAlignment::End, TextAlignment::End),
            (TextAlignment::Justify, TextAlignment::Justify),
        ] {
            let block = TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: "作者声明的正文对齐".into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle {
                    align: alignment,
                    authored_alignment: Some(alignment),
                    ..rebook_publication::BlockStyle::default()
                },
                source: None,
            };

            let resolved = resolve_text_block(&block, &style, TextContext::Flow);
            assert_eq!(resolved.style.align, expected);
        }

        let list_item = TextBlock {
            kind: TextBlockKind::ListItem {
                ordered: false,
                ordinal: 1,
                depth: 0,
                marker_visible: true,
            },
            content: vec![Inline::Text(TextRun {
                text: "Unified list prose".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle {
                align: TextAlignment::End,
                authored_alignment: Some(TextAlignment::End),
                ..rebook_publication::BlockStyle::default()
            },
            source: None,
        };

        let resolved = resolve_text_block(&list_item, &style, TextContext::Flow);
        assert_eq!(resolved.style.align, TextAlignment::Justify);
    }

    #[test]
    fn unified_typesetting_justifies_cjk_and_nbsp_paragraphs_but_not_lists() {
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        let cases = [
            (TextBlockKind::Paragraph, "2015年，Dark Reading报道"),
            (
                TextBlockKind::ListItem {
                    ordered: true,
                    ordinal: 1,
                    depth: 0,
                    marker_visible: true,
                },
                "攻击者调查了几个目标",
            ),
            (TextBlockKind::Paragraph, "Dark\u{00a0}Reading report"),
        ];

        for (kind, text) in cases {
            let expected = if kind == TextBlockKind::Paragraph {
                TextAlignment::Justify
            } else {
                TextAlignment::Start
            };
            let block = TextBlock {
                kind,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            };

            let resolved = resolve_text_block(&block, &style, TextContext::Flow);
            assert_eq!(resolved.style.align, expected, "{text}");
        }
    }

    #[test]
    fn book_typesetting_preserves_authored_metrics() {
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: "Body".into(),
                style: TextStyle {
                    size_scale: 1.35,
                    ..TextStyle::default()
                },
                link: None,
            })],
            style: rebook_publication::BlockStyle {
                margin_after: 23.0,
                line_height: 1.25,
                ..rebook_publication::BlockStyle::default()
            },
            source: None,
        };

        let resolved = resolve_text_block(&block, &ReaderStyle::default(), TextContext::Flow);
        assert_eq!(resolved.as_ref(), &block);
    }

    #[test]
    fn structural_break_after_survives_unified_and_book_typesetting() {
        let normal = TextBlock {
            kind: TextBlockKind::Blockquote,
            content: vec![Inline::Text(TextRun {
                text: "First stanza".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle {
                margin_after: 6.0,
                line_height: 1.3,
                ..rebook_publication::BlockStyle::default()
            },
            source: None,
        };
        let mut separated = normal.clone();
        separated.style.hard_break_after = true;
        let mut style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let normal_unified = resolve_text_block(&normal, &style, TextContext::Flow);
        let separated_unified = resolve_text_block(&separated, &style, TextContext::Flow);
        assert!(
            (separated_unified.style.margin_after
                - normal_unified.style.margin_after
                - style.typography.font_size * style.typesetting.body_line_height)
                .abs()
                < 0.001
        );

        style.typesetting.mode = TypesettingMode::Book;
        let normal_book = resolve_text_block(&normal, &style, TextContext::Flow);
        let separated_book = resolve_text_block(&separated, &style, TextContext::Flow);
        assert!(
            (separated_book.style.margin_after
                - normal_book.style.margin_after
                - style.typography.font_size * separated.style.line_height)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn reader_typesetting_normalizes_persisted_values() {
        let mut typesetting = ReaderTypesetting {
            mode: TypesettingMode::Unified,
            line_break_strategy: LineBreakStrategy::Greedy,
            heading_scale: f32::NAN,
            body_line_height: 9.0,
            paragraph_indent_mode: ParagraphIndentMode::Custom,
            paragraph_indent_em: 8.0,
            paragraph_gap_em: -1.0,
            heading_body_gap_em: 4.0,
            media_gap_em: 0.1,
            caption_font_scale: 0.1,
            caption_gap_em: 4.0,
            list_indent_em: 8.0,
            table_font_scale: 0.1,
            table_line_height: f32::INFINITY,
            table_cell_padding_em: 2.0,
        };
        typesetting.normalize();
        assert!((typesetting.heading_scale - 1.6).abs() < 0.001);
        assert!((typesetting.body_line_height - 2.4).abs() < 0.001);
        assert!((typesetting.paragraph_indent_em - 4.0).abs() < 0.001);
        assert!(typesetting.paragraph_gap_em.abs() < 0.001);
        assert!((typesetting.heading_body_gap_em - 2.0).abs() < 0.001);
        assert!((typesetting.media_gap_em - 0.5).abs() < 0.001);
        assert!((typesetting.caption_font_scale - 0.7).abs() < 0.001);
        assert!((typesetting.caption_gap_em - 1.0).abs() < 0.001);
        assert!((typesetting.list_indent_em - 3.0).abs() < 0.001);
        assert!((typesetting.table_font_scale - 0.7).abs() < 0.001);
        assert!((typesetting.table_line_height - 1.45).abs() < 0.001);
        assert!((typesetting.table_cell_padding_em - 1.0).abs() < 0.001);
    }

    #[test]
    fn unified_typesetting_enables_optimized_line_breaking() {
        assert_eq!(
            ReaderTypesetting::default().line_break_strategy,
            LineBreakStrategy::Greedy
        );
        assert_eq!(
            ReaderTypesetting::unified().line_break_strategy,
            LineBreakStrategy::Optimized
        );
    }

    #[test]
    fn adaptive_table_widths_preserve_the_available_measure() {
        let widths = fit_adaptive_column_widths(&[50.0, 200.0], 40.0, 300.0);
        assert_eq!(widths.len(), 2);
        assert!(widths[0] < widths[1]);
        assert!((widths.iter().sum::<f32>() - 250.0).abs() < 0.001);
        assert!(widths.iter().all(|width| *width >= 40.0));
    }

    #[test]
    fn unified_table_preserves_authored_alignment_and_centers_unspecified_cells() {
        let cell = |authored_alignment| TableCell {
            text: TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: "Same content".into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            },
            authored_alignment,
            column_span: 1,
            row_span: 1,
            header: false,
        };
        let table = TableBlock {
            rows: vec![TableRow {
                cells: vec![cell(None), cell(Some(TextAlignment::Start))],
            }],
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let table = LayoutEngine::new().shape_table(&table, &style, 500.0);
        let centered_offset = table.cells[0]
            .text
            .layout
            .get(0)
            .expect("default cell should contain text")
            .metrics()
            .offset;
        let authored_offset = table.cells[1]
            .text
            .layout
            .get(0)
            .expect("authored cell should contain text")
            .metrics()
            .offset;

        assert!(
            centered_offset > 0.0,
            "unspecified cells should be centered"
        );
        assert!(
            authored_offset.abs() < 0.001,
            "authored left alignment should be preserved"
        );
    }

    #[test]
    fn unified_table_keeps_short_cells_on_one_line_when_space_allows() {
        let cell = |text: &str| TableCell {
            text: TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            },
            authored_alignment: None,
            column_span: 1,
            row_span: 1,
            header: false,
        };
        let table = TableBlock {
            rows: vec![
                TableRow {
                    cells: vec![cell(""), cell("U.S."), cell("Norway")],
                },
                TableRow {
                    cells: vec![
                        cell("Introductory course"),
                        cell("1.7 books"),
                        cell("2.8 books"),
                    ],
                },
                TableRow {
                    cells: vec![
                        cell("Advanced course"),
                        cell("2.3 books"),
                        cell("2.8 books"),
                    ],
                },
            ],
            source: None,
        };
        let mut style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        style.typography.font_size = 16.0;
        style.typesetting.table_font_scale = 0.8;

        let table = LayoutEngine::new().shape_table(&table, &style, 744.0);
        assert!(table.column_widths.iter().sum::<f32>() < 744.0);
        assert!(
            table.cells.iter().all(|cell| cell.text.layout.len() == 1),
            "short table cells should remain unwrapped"
        );
    }

    #[test]
    fn unified_typesetting_clears_authored_and_link_underlines() {
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: "Linked and underlined".into(),
                style: TextStyle {
                    underline: true,
                    ..TextStyle::default()
                },
                link: Some(PublicationUrl::parse("chapter.xhtml#target").unwrap()),
            })],
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let resolved = resolve_text_block(&block, &style, TextContext::Flow);
        let Inline::Text(run) = &resolved.content[0] else {
            panic!("expected text run");
        };
        assert!(!run.style.underline);
        assert!(run.link.is_some());
    }

    #[test]
    fn unified_quotes_clear_authored_bold_and_italic_styles() {
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        for kind in [TextBlockKind::Blockquote, TextBlockKind::QuoteAttribution] {
            let block = TextBlock {
                kind,
                content: vec![Inline::Text(TextRun {
                    text: "Authored emphasis".into(),
                    style: TextStyle {
                        bold: true,
                        italic: true,
                        ..TextStyle::default()
                    },
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            };

            let resolved = resolve_text_block(&block, &style, TextContext::Flow);
            let Inline::Text(run) = &resolved.content[0] else {
                panic!("expected text run");
            };
            assert!(!run.style.bold);
            assert!(!run.style.italic);
        }
    }

    #[test]
    fn unified_quotes_justify_baseline_alignment_and_preserve_special_alignment() {
        let block = TextBlock {
            kind: TextBlockKind::Blockquote,
            content: vec![Inline::Text(TextRun {
                text: "An indented quotation with baseline alignment".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle {
                margin_start: 60.0,
                indent: 32.0,
                ..rebook_publication::BlockStyle::default()
            },
            source: None,
        };
        let style = ReaderStyle {
            typography: ReaderTypography {
                font_size: 20.0,
                ..ReaderTypography::default()
            },
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let resolved = resolve_text_block(&block, &style, TextContext::Flow);
        assert_eq!(resolved.style.align, TextAlignment::Justify);
        assert!((resolved.style.indent - 40.0).abs() < 0.001);
        assert!(resolved.style.margin_start.abs() < 0.001);
        let (start_offset, _, first_line_indent) = resolve_text_measure(&resolved, 320.0, 40.0);
        assert!(start_offset.abs() < 0.001);
        assert!((first_line_indent - 40.0).abs() < 0.001);

        let mut authored_start = block.clone();
        authored_start.style.authored_alignment = Some(TextAlignment::Start);
        let resolved = resolve_text_block(&authored_start, &style, TextContext::Flow);
        assert_eq!(resolved.style.align, TextAlignment::Justify);

        for alignment in [
            TextAlignment::Center,
            TextAlignment::End,
            TextAlignment::Justify,
        ] {
            let mut specially_aligned = block.clone();
            specially_aligned.style.align = alignment;
            specially_aligned.style.authored_alignment = Some(alignment);
            let resolved = resolve_text_block(&specially_aligned, &style, TextContext::Flow);
            assert_eq!(resolved.style.align, alignment);
        }

        let mut unindented = block;
        unindented.style.indent = 0.0;
        let resolved = resolve_text_block(&unindented, &style, TextContext::Flow);
        assert_eq!(resolved.style.align, TextAlignment::Justify);
        assert!(resolved.style.indent.abs() < 0.001);
    }

    #[test]
    fn unified_typesetting_applies_a_consistent_list_indent() {
        let block = TextBlock {
            kind: TextBlockKind::ListItem {
                ordered: false,
                ordinal: 1,
                depth: 0,
                marker_visible: true,
            },
            content: vec![Inline::Text(TextRun {
                text: "A list item".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle {
                margin_start: 90.0,
                margin_start_fraction: 0.2,
                ..rebook_publication::BlockStyle::default()
            },
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let resolved = resolve_text_block(&block, &style, TextContext::Flow);
        assert!((resolved.style.margin_start - 30.0).abs() < 0.001);
        assert!(resolved.style.margin_start_fraction.abs() < 0.001);
    }

    #[test]
    fn unified_typesetting_increases_indent_for_nested_list_items() {
        let block = TextBlock {
            kind: TextBlockKind::ListItem {
                ordered: false,
                ordinal: 1,
                depth: 2,
                marker_visible: true,
            },
            content: vec![Inline::Text(TextRun {
                text: "A nested list item".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let resolved = resolve_text_block(&block, &style, TextContext::Flow);
        assert!((resolved.style.margin_start - 90.0).abs() < 0.001);
        assert!(resolved.style.margin_start_fraction.abs() < 0.001);
    }

    #[test]
    fn unified_typesetting_normalizes_markerless_nested_list_items() {
        let block = TextBlock {
            kind: TextBlockKind::ListItem {
                ordered: false,
                ordinal: 1,
                depth: 1,
                marker_visible: false,
            },
            content: vec![Inline::Text(TextRun {
                text: "A marker-less nested outline item".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle {
                margin_start: 75.2,
                margin_start_fraction: 0.0,
                indent: -17.6,
                ..rebook_publication::BlockStyle::default()
            },
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let resolved = resolve_text_block(&block, &style, TextContext::Flow);
        assert!((resolved.style.margin_start - 60.0).abs() < 0.001);
        assert!(resolved.style.margin_start_fraction.abs() < 0.001);
        assert!(resolved.style.indent.abs() < 0.001);
        assert!(list_marker_prefix(resolved.kind).is_empty());
    }

    #[test]
    fn unified_typesetting_distinguishes_definition_terms_and_descriptions() {
        let text = || {
            vec![Inline::Text(TextRun {
                text: "Definition".into(),
                style: TextStyle::default(),
                link: None,
            })]
        };
        let term = TextBlock {
            kind: TextBlockKind::DefinitionTerm { depth: 0 },
            content: text(),
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let description = TextBlock {
            kind: TextBlockKind::DefinitionDescription { depth: 0 },
            content: text(),
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let resolved_term = resolve_text_block(&term, &style, TextContext::Flow);
        let resolved_description = resolve_text_block(&description, &style, TextContext::Flow);
        assert!(resolved_term.style.margin_start.abs() < 0.001);
        assert!((resolved_description.style.margin_start - 30.0).abs() < 0.001);
        let Inline::Text(term_text) = &resolved_term.content[0] else {
            panic!("expected definition term text");
        };
        assert!(term_text.style.bold);
    }

    #[test]
    fn unified_typesetting_applies_first_line_paragraph_indent() {
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: "A sufficiently long paragraph that wraps onto another line so its first-line indentation can be distinguished from continuation lines.".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        let resolved = resolve_text_block(&block, &style, TextContext::Flow);
        assert!((resolved.style.indent - 40.0).abs() < 0.001);

        let mut engine = LayoutEngine::new();
        let prepared = engine.shape_text_with_min_width(&resolved, &style, 320.0, 40.0);
        assert!(prepared.layout.len() > 1);
        let first_offset = prepared.layout.get(0).unwrap().metrics().offset;
        let continuation_offset = prepared.layout.get(1).unwrap().metrics().offset;
        assert!((first_offset - 40.0).abs() < 0.01);
        assert!(continuation_offset.abs() < 0.01);
    }

    #[test]
    fn automatic_paragraph_indent_uses_publication_writing_system() {
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: "Paragraph".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let mut style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            writing_system: WritingSystem::Cjk,
            ..ReaderStyle::default()
        };

        let cjk = resolve_text_block(&block, &style, TextContext::Flow);
        assert!((cjk.style.indent - 40.0).abs() < 0.001);

        style.writing_system = WritingSystem::Latin;
        let latin = resolve_text_block(&block, &style, TextContext::Flow);
        assert!((latin.style.indent - 30.0).abs() < 0.001);

        style.typesetting.paragraph_indent_mode = ParagraphIndentMode::Custom;
        style.typesetting.paragraph_indent_em = 0.8;
        let custom = resolve_text_block(&block, &style, TextContext::Flow);
        assert!((custom.style.indent - 16.0).abs() < 0.001);
    }

    #[test]
    fn reader_typography_uses_bundled_language_defaults_and_builds_cjk_stacks() {
        let typography = ReaderTypography::default();
        assert_eq!(typography.default_font, ReaderDefaultFont::Serif);
        assert_eq!(typography.default_cjk_font, "LXGW WenKai GB Screen");
        assert_eq!(typography.serif_font, "Literata");
        assert_eq!(typography.sans_serif_font, "Arial");
        assert!(typography.other_font.is_empty());
        assert_eq!(
            typography.cjk_default_font,
            Some(ReaderFontChoice {
                category: ReaderDefaultFont::Serif,
                family: "Literata".into(),
            })
        );
        assert!(typography.latin_cjk_font.is_none());
        assert_eq!(typography.monospace_font, "Consolas");
        assert!((typography.font_size - 20.0).abs() < f32::EPSILON);
        assert!((typography.minimum_font_size - 12.0).abs() < f32::EPSILON);
        assert_eq!(typography.font_weight, 400);
        assert!(typography.serif_stack().starts_with("\"Literata\""));
        assert!(typography.serif_stack().contains("\"SimSun\""));
        assert!(typography.serif_stack().ends_with("serif"));
        assert!(
            typography
                .sans_serif_stack()
                .contains("\"Microsoft YaHei\"")
        );
        assert!(typography.sans_serif_stack().ends_with("sans-serif"));
        assert_eq!(
            typography.monospace_stack(),
            "\"Consolas\", \"LXGW WenKai GB Screen\", monospace"
        );
        assert!(!typography.monospace_stack().contains("Fira Code"));
    }

    #[test]
    fn typography_selects_independent_cjk_and_latin_book_stacks() {
        let typography = ReaderTypography {
            default_font: ReaderDefaultFont::Serif,
            default_cjk_font: "CJK Primary".into(),
            serif_font: "Latin Primary".into(),
            cjk_default_font: Some(ReaderFontChoice {
                category: ReaderDefaultFont::SansSerif,
                family: "CJK Western".into(),
            }),
            latin_cjk_font: Some("Latin CJK Fallback".into()),
            ..ReaderTypography::default()
        };

        assert!(
            typography
                .default_stack_for(WritingSystem::Cjk)
                .starts_with("\"CJK Western\", \"CJK Primary\"")
        );
        assert!(
            typography
                .default_stack_for(WritingSystem::Latin)
                .starts_with("\"Latin Primary\", \"Latin CJK Fallback\"")
        );
        assert_eq!(
            typography.default_stack_for(WritingSystem::Unknown),
            typography.default_stack_for(WritingSystem::Latin)
        );
    }

    #[test]
    fn other_font_category_uses_the_selected_family_with_a_safe_fallback() {
        let typography = ReaderTypography {
            default_font: ReaderDefaultFont::Other,
            other_font: "Decorative Reader".into(),
            ..ReaderTypography::default()
        };

        let stack = typography.default_stack_for(WritingSystem::Latin);
        assert!(stack.starts_with("\"Decorative Reader\""));
        assert!(stack.ends_with("sans-serif"));
    }

    #[test]
    fn legacy_ysabeau_defaults_migrate_to_literata() {
        let mut typography = ReaderTypography {
            default_font: ReaderDefaultFont::Other,
            serif_font: "Georgia".into(),
            other_font: " Ysabeau Office ".into(),
            cjk_default_font: Some(ReaderFontChoice {
                category: ReaderDefaultFont::Other,
                family: "Ysabeau Office".into(),
            }),
            ..ReaderTypography::default()
        };

        typography.normalize();

        assert_eq!(typography.default_font, ReaderDefaultFont::Serif);
        assert_eq!(typography.serif_font, "Literata");
        assert!(typography.other_font.is_empty());
        assert_eq!(
            typography.cjk_default_font,
            Some(ReaderFontChoice {
                category: ReaderDefaultFont::Serif,
                family: "Literata".into(),
            })
        );
    }

    #[test]
    fn transitional_literata_other_category_is_canonicalized_as_serif() {
        let mut typography = ReaderTypography {
            default_font: ReaderDefaultFont::Other,
            serif_font: "Georgia".into(),
            other_font: "Literata".into(),
            cjk_default_font: Some(ReaderFontChoice {
                category: ReaderDefaultFont::Other,
                family: "Literata".into(),
            }),
            ..ReaderTypography::default()
        };

        typography.normalize();

        assert_eq!(typography.default_font, ReaderDefaultFont::Serif);
        assert_eq!(typography.serif_font, "Literata");
        assert!(typography.other_font.is_empty());
        assert_eq!(
            typography.cjk_default_font,
            Some(ReaderFontChoice {
                category: ReaderDefaultFont::Serif,
                family: "Literata".into(),
            })
        );
    }

    #[test]
    fn optical_size_tracks_each_span_size_in_typographic_points() {
        assert!((optical_size_for_font(4.0) - 7.0).abs() < f32::EPSILON);
        assert!((optical_size_for_font(20.0) - 15.0).abs() < f32::EPSILON);
        assert!((optical_size_for_font(32.0) - 24.0).abs() < f32::EPSILON);
        assert!((optical_size_for_font(120.0) - 72.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cjk_prose_digits_use_the_configured_western_font() {
        const LITERATA: &[u8] = include_bytes!("../../../assets/fonts/Literata-opsz-wght.ttf");
        const CJK: &[u8] = include_bytes!("../../../assets/fonts/LXGWWenKaiGBScreen.ttf");
        let literata = ReaderFontBlob::new(Arc::new(LITERATA));
        let cjk = ReaderFontBlob::new(Arc::new(CJK));
        let mut engine = LayoutEngine::with_fonts([literata, cjk]);
        let text = "卷二，93页14行—94页1—4行";
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: text.into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: BlockStyle::default(),
            source: None,
        };
        let style = ReaderStyle {
            typography: ReaderTypography {
                default_font: ReaderDefaultFont::Other,
                other_font: "Literata".into(),
                cjk_default_font: Some(ReaderFontChoice {
                    category: ReaderDefaultFont::Other,
                    family: "Literata".into(),
                }),
                ..ReaderTypography::default()
            },
            writing_system: WritingSystem::Cjk,
            ..ReaderStyle::default()
        };
        let prepared = engine.shape_text_with_min_width(&block, &style, 800.0, 40.0);
        let runs = prepared
            .layout
            .lines()
            .flat_map(|line| line.items())
            .filter_map(|item| match item {
                PositionedLayoutItem::GlyphRun(glyphs) => Some((
                    text[glyphs.run().text_range()].to_owned(),
                    glyphs.run().font().data.as_ref() == LITERATA,
                )),
                PositionedLayoutItem::InlineBox(_) => None,
            })
            .collect::<Vec<_>>();

        assert!(
            runs.iter()
                .filter(|(run, _)| run.chars().any(|character| character.is_ascii_digit()))
                .all(|(_, uses_literata)| *uses_literata),
            "CJK prose digits did not use Literata: {runs:?}"
        );
    }

    #[test]
    fn literata_defaults_to_lining_figures() {
        use parley::FontFeatures;

        const LITERATA: &[u8] = include_bytes!("../../../assets/fonts/Literata-opsz-wght.ttf");
        const LITERATA_ITALIC: &[u8] =
            include_bytes!("../../../assets/fonts/Literata-Italic-opsz-wght.ttf");
        let literata = ReaderFontBlob::new(Arc::new(LITERATA));
        let literata_italic = ReaderFontBlob::new(Arc::new(LITERATA_ITALIC));
        let mut engine = LayoutEngine::with_fonts([literata, literata_italic]);
        let mut glyph_ids = |features: Option<&str>, italic: bool| {
            let mut builder = engine.layout_context.ranged_builder(
                &mut engine.font_context,
                "0123456789",
                1.0,
                false,
            );
            builder.push_default(StyleProperty::FontFamily(FontFamily::from("Literata")));
            builder.push_default(StyleProperty::FontSize(20.0));
            if italic {
                builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
            }
            if let Some(features) = features {
                builder.push_default(StyleProperty::FontFeatures(FontFeatures::from(features)));
            }
            let mut layout: Layout<TextBrush> = builder.build("0123456789");
            layout.break_all_lines(None);
            layout
                .get(0)
                .unwrap()
                .items()
                .flat_map(|item| match item {
                    PositionedLayoutItem::GlyphRun(run) => {
                        run.glyphs().map(|glyph| glyph.id).collect::<Vec<_>>()
                    }
                    PositionedLayoutItem::InlineBox(_) => Vec::new(),
                })
                .collect::<Vec<_>>()
        };

        for italic in [false, true] {
            let default = glyph_ids(None, italic);
            let lining = glyph_ids(Some("\"lnum\""), italic);
            let oldstyle = glyph_ids(Some("\"onum\""), italic);
            assert_eq!(default, lining);
            assert_ne!(default, oldstyle);
        }
    }

    #[test]
    fn reader_typography_normalizes_persisted_values() {
        let mut typography = ReaderTypography {
            default_cjk_font: "  ".into(),
            serif_font: "  Georgia  ".into(),
            sans_serif_font: String::new(),
            cjk_default_font: Some(ReaderFontChoice {
                category: ReaderDefaultFont::Serif,
                family: "  ".into(),
            }),
            latin_cjk_font: Some("  ".into()),
            monospace_font: String::new(),
            font_size: f32::NAN,
            minimum_font_size: -4.0,
            font_weight: 455,
            ..ReaderTypography::default()
        };
        typography.normalize();
        assert_eq!(typography.default_cjk_font, "LXGW WenKai GB Screen");
        assert_eq!(typography.serif_font, "Georgia");
        assert_eq!(typography.sans_serif_font, "Arial");
        assert!(typography.cjk_default_font.is_none());
        assert!(typography.latin_cjk_font.is_none());
        assert_eq!(typography.monospace_font, "Consolas");
        assert!((typography.font_size - 20.0).abs() < f32::EPSILON);
        assert!((typography.minimum_font_size - 1.0).abs() < f32::EPSILON);
        assert_eq!(typography.font_weight, 455);

        typography.default_cjk_font = "LXGW WenKai".into();
        typography.normalize();
        assert_eq!(typography.default_cjk_font, "LXGW WenKai GB Screen");
    }

    #[test]
    fn unavailable_cjk_preference_is_repaired_to_a_validated_family() {
        let families = ReaderFontFamilies {
            all: vec![
                "LXGW WenKai GB Screen".into(),
                "Georgia".into(),
                "Arial".into(),
                "Consolas".into(),
            ],
            chinese: vec!["LXGW WenKai GB Screen".into()],
            serif: vec!["Georgia".into()],
            sans_serif: vec!["Arial".into()],
            monospace: vec!["Consolas".into()],
            ..ReaderFontFamilies::default()
        };
        let mut typography = ReaderTypography {
            default_font: ReaderDefaultFont::Other,
            default_cjk_font: "宋体".into(),
            serif_font: "Unavailable Serif".into(),
            sans_serif_font: "Unavailable Sans".into(),
            other_font: "Unavailable Other".into(),
            cjk_default_font: Some(ReaderFontChoice {
                category: ReaderDefaultFont::Serif,
                family: "Unavailable CJK Western".into(),
            }),
            latin_cjk_font: Some("Unavailable CJK".into()),
            monospace_font: "Unavailable Mono".into(),
            ..ReaderTypography::default()
        };

        assert!(families.repair_typography(&mut typography));
        assert_eq!(typography.default_cjk_font, "LXGW WenKai GB Screen");
        assert_eq!(typography.serif_font, "Georgia");
        assert_eq!(typography.sans_serif_font, "Arial");
        assert_eq!(typography.default_font, ReaderDefaultFont::Serif);
        assert!(typography.other_font.is_empty());
        assert!(typography.cjk_default_font.is_none());
        assert!(typography.latin_cjk_font.is_none());
        assert_eq!(typography.monospace_font, "Consolas");
        assert!(!families.repair_typography(&mut typography));
    }

    #[test]
    fn reader_font_classification_uses_panose_and_fixed_pitch_metadata() {
        let serif = classify_reader_font(Some(&[2, 2, 5, 3, 0, 0, 0, 0, 0, 0]), None, false);
        assert!(serif.serif);
        assert!(!serif.sans_serif);
        assert!(!serif.monospace);

        let sans = classify_reader_font(Some(&[2, 11, 5, 3, 0, 0, 0, 0, 0, 0]), None, false);
        assert!(!sans.serif);
        assert!(sans.sans_serif);
        assert!(!sans.monospace);

        let monospace = classify_reader_font(Some(&[2, 11, 5, 9, 0, 0, 0, 0, 0, 0]), None, false);
        assert!(!monospace.serif);
        assert!(!monospace.sans_serif);
        assert!(monospace.monospace);

        assert!(classify_reader_font(None, None, true).monospace);

        let family_class_serif = classify_reader_font(Some(&[0; 10]), Some(1 << 8), false);
        assert!(family_class_serif.serif);
        let family_class_sans = classify_reader_font(Some(&[0; 10]), Some(8 << 8), false);
        assert!(family_class_sans.sans_serif);
        let unclassified = classify_reader_font(Some(&[0; 10]), Some(10 << 8), false);
        assert!(!unclassified.serif && !unclassified.sans_serif && !unclassified.monospace);

        assert!(infer_reader_font_classification("Sitka Text").serif);
        assert!(infer_reader_font_classification("Segoe UI Variable Text").sans_serif);
        assert!(infer_reader_font_classification("Comic Sans MS").sans_serif);
        assert!(infer_reader_font_classification("Literata").serif);
        assert!(is_symbolic_reader_font("Symbol", Some(&[5; 10]), None));
        assert!(is_symbolic_reader_font("DejaVu Math TeX Gyre", None, None));
    }

    #[test]
    fn default_page_geometry_compacts_only_the_top_inset() {
        let style = ReaderStyle::default();
        let geometry = resolve_page_geometry(800.0, 600.0, &style);

        assert!((style.top_margin - DEFAULT_TOP_MARGIN).abs() < f32::EPSILON);
        assert!((style.bottom_margin - DEFAULT_BOTTOM_MARGIN).abs() < f32::EPSILON);
        assert!((geometry.top - DEFAULT_TOP_MARGIN).abs() < f32::EPSILON);
        assert!((geometry.bottom - (600.0 - DEFAULT_BOTTOM_MARGIN)).abs() < f32::EPSILON);
    }

    #[test]
    fn wide_viewports_cap_and_center_each_reading_column() {
        let viewport_width = 3_000.0;
        let page_height = 900.0;
        let single = resolve_page_geometry(
            viewport_width,
            page_height,
            &ReaderStyle {
                spread: SpreadMode::Single,
                ..ReaderStyle::default()
            },
        );
        assert_eq!(single.visible_pages, 1);
        assert!((single.width - MAX_COLUMN_WIDTH).abs() < f32::EPSILON);
        assert!((single.left - (viewport_width - MAX_COLUMN_WIDTH) / 2.0).abs() < f32::EPSILON);

        let scroll = resolve_page_geometry(
            viewport_width,
            page_height,
            &ReaderStyle {
                spread: SpreadMode::Scroll,
                ..ReaderStyle::default()
            },
        );
        assert_eq!(scroll.visible_pages, 1);
        assert!((scroll.width - single.width).abs() < f32::EPSILON);
        assert!((scroll.left - single.left).abs() < f32::EPSILON);
        assert!((scroll.top - single.top).abs() < f32::EPSILON);
        assert!((scroll.bottom - single.bottom).abs() < f32::EPSILON);

        let double = resolve_page_geometry(
            viewport_width,
            page_height,
            &ReaderStyle {
                spread: SpreadMode::Double,
                ..ReaderStyle::default()
            },
        );
        let spread_width = MAX_COLUMN_WIDTH * 2.0 + DEFAULT_COLUMN_GAP;
        assert_eq!(double.visible_pages, 2);
        assert!((double.width - MAX_COLUMN_WIDTH).abs() < f32::EPSILON);
        assert!((double.left - (viewport_width - spread_width) / 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_column_gap_places_double_pages_next_to_each_other() {
        let geometry = resolve_page_geometry(
            1_200.0,
            700.0,
            &ReaderStyle {
                spread: SpreadMode::Double,
                column_gap: 0.0,
                ..ReaderStyle::default()
            },
        );

        assert_eq!(geometry.visible_pages, 2);
        assert!((geometry.continuation_offset_x - geometry.width).abs() < f32::EPSILON);
    }

    #[test]
    fn wrapped_list_items_use_the_full_marker_advance_as_hanging_indent() {
        let block = TextBlock {
            kind: TextBlockKind::ListItem {
                ordered: false,
                ordinal: 1,
                depth: 0,
                marker_visible: true,
            },
            content: vec![Inline::Text(TextRun {
                text: "Create hierarchy. Type embodies what you want to say with your design, and it creates and supports your website structure.".into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let mut engine = LayoutEngine::new();
        let prepared =
            engine.shape_text_with_min_width(&block, &ReaderStyle::default(), 320.0, 40.0);
        assert!(prepared.layout.len() > 1);
        let marker_width = engine.measure_list_marker_width(
            "•\u{00a0}",
            &ReaderStyle::default().typography.default_stack(),
            &ReaderStyle::default().typography,
        );
        let continuation_x = prepared.layout.get(1).unwrap().metrics().offset;
        assert!((continuation_x - marker_width).abs() < 0.01);
    }

    #[test]
    fn optimized_list_text_stays_inside_the_shared_right_edge() {
        let block = TextBlock {
            kind: TextBlockKind::ListItem { ordered:true, ordinal:1, depth:0, marker_visible:true },
            content: vec![Inline::Text(TextRun {
                text: "In continuous signaling you often have to amplify the signal to compensate for natural losses along the way. Any error made at one stage is amplified by the next stage. ".repeat(5),
                style: TextStyle::default(), link:None,
            })],
            style: BlockStyle { align:TextAlignment::Justify, ..BlockStyle::default() }, source:None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        let prepared = LayoutEngine::new().shape_text_with_min_width(&block, &style, 480.0, 40.0);
        assert!(prepared.layout.len() > 2);
        for line in prepared.layout.lines().take(prepared.layout.len() - 1) {
            let end = linebreak::parley::positioned_line_content_end(line);
            assert!(
                (end - prepared.available_width).abs() < 1.0,
                "list text right edge {end} differs from available width {}",
                prepared.available_width
            );
        }
    }

    #[test]
    fn optimized_english_paragraph_prepares_only_selected_line_hyphens() {
        let block = TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: "Extraordinary typographical considerations improve international readability and representation.".into(),
                style: TextStyle {
                    language: rebook_publication::TextLanguage::EnglishUs,
                    ..TextStyle::default()
                },
                link: None,
            })],
            style: BlockStyle {
                align: TextAlignment::Justify,
                ..BlockStyle::default()
            },
            source: None,
        };
        let mut engine = LayoutEngine::new();
        engine.publication_languages = vec!["en-US".into()];
        let reader_style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let prepared = (100_u16..=240)
            .step_by(5)
            .map(|width| {
                engine.shape_text_with_min_width(&block, &reader_style, f32::from(width), 40.0)
            })
            .find(|prepared| !prepared.hyphens.is_empty())
            .expect("at least one narrow measure should select a dictionary break");

        assert!(prepared.layout.len() > 1);
        assert!(
            prepared
                .hyphens
                .iter()
                .all(|hyphen| hyphen.line_index + 1 < prepared.layout.len())
        );
        assert!(!prepared.text.contains('\u{2010}'));
    }
    use rebook_publication::{
        Book, FixedPageTextReplacement, FixedPageTextReplacementSegment, FixedPageTextSpan,
        ImageBlock, ImageLength, Metadata, PublicationId, PublicationUrl, QuoteBlock,
        RasterResource, RenditionLayout, Resource, SeparatorBlock, SourceAnchor, SpineItemId,
        TableCell, TableRow, TocEntry,
    };

    struct EmptySource {
        book: Book,
    }

    impl BookSource for EmptySource {
        fn book(&self) -> &Book {
            &self.book
        }

        fn parse_section(&self, _index: usize) -> Result<Section, PublicationError> {
            unreachable!()
        }

        fn resource(&self, href: &PublicationUrl) -> Result<Resource, PublicationError> {
            Err(PublicationError::ResourceNotFound(href.to_string()))
        }

        fn raster_resource(
            &self,
            _href: &PublicationUrl,
        ) -> Result<Option<RasterResource>, PublicationError> {
            Ok(Some(RasterResource {
                width: 200,
                height: 100,
                pixels: Vec::new().into(),
            }))
        }
    }

    #[test]
    fn long_paragraph_is_split_into_multiple_pages() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::<TocEntry>::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(rebook_publication::TextRun {
                    text: "这是用于验证分页的数据。".repeat(500),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            })],
            anchors: Vec::new(),
        };
        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(600, 400).unwrap(),
                &ReaderStyle::default(),
            )
            .unwrap();
        assert!(layout.pages.len() > 1);
    }

    #[test]
    fn selected_hyphens_are_emitted_as_source_free_text_items() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("hyphenation-test").unwrap(),
                metadata: Metadata {
                    languages: vec!["en-US".into()],
                    ..Metadata::default()
                },
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: "Extraordinary typographical considerations improve international readability and representation.".into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: BlockStyle::default(),
                source: None,
            })],
            anchors: Vec::new(),
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            horizontal_margin: 0.0,
            ..ReaderStyle::default()
        };
        let mut engine = LayoutEngine::new();
        let found = (100_u16..=240).step_by(5).find_map(|width| {
            let layout = engine
                .layout_section(
                    &source,
                    &section,
                    LayoutViewport::new(u32::from(width), 800).unwrap(),
                    &style,
                )
                .ok()?;
            layout.pages.iter().find_map(|page| {
                page.items.iter().find_map(|item| match item {
                    PageItem::Text(text) if text.text.as_ref() == "\u{2010}" => Some(text),
                    _ => None,
                })
            })?;
            Some(())
        });

        assert!(found.is_some());
    }

    #[test]
    fn standalone_line_break_adds_spacing_without_forcing_a_page() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("line-break-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let paragraph = |text: &str| {
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            })
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![paragraph("Before"), Block::LineBreak, paragraph("After")],
            anchors: Vec::new(),
        };
        let mut style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        style.spread = SpreadMode::Scroll;
        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(600, 400).unwrap(),
                &style,
            )
            .unwrap();

        assert_eq!(layout.pages.len(), 1);
        let text_origins = layout.pages[0]
            .items
            .iter()
            .filter_map(|item| match item {
                PageItem::Text(text) => Some(text.origin_y),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(text_origins.len(), 2);
        assert!(text_origins[1] - text_origins[0] > style.typography.font_size * 2.0);
    }

    #[test]
    fn unified_typesetting_filters_body_separators_but_book_typesetting_preserves_them() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("separator-filter-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let paragraph = |text: &str| {
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            })
        };
        let ornament = ImageBlock {
            href: PublicationUrl::parse("rule.png").unwrap(),
            alt: String::new(),
            style: ImageStyle::default(),
            source: None,
            text_layer: None,
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                paragraph("Before"),
                Block::Separator(SeparatorBlock {
                    kind: SeparatorKind::Symbols,
                    text: Some(TextBlock {
                        kind: TextBlockKind::Paragraph,
                        content: vec![Inline::Text(TextRun {
                            text: "* * *".into(),
                            style: TextStyle::default(),
                            link: None,
                        })],
                        style: BlockStyle::default(),
                        source: None,
                    }),
                    in_quote: false,
                    image: None,
                    style: BlockStyle::default(),
                }),
                Block::Separator(SeparatorBlock::spacing(
                    rebook_publication::BlockStyle::default(),
                )),
                Block::Separator(SeparatorBlock::rule()),
                Block::Separator(SeparatorBlock::ornament(ornament)),
                paragraph("After"),
            ],
            anchors: Vec::new(),
        };
        let viewport = LayoutViewport::new(600, 600).unwrap();
        let unified = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                viewport,
                &ReaderStyle {
                    typesetting: ReaderTypesetting::unified(),
                    ..ReaderStyle::default()
                },
            )
            .unwrap();
        assert!(
            unified
                .pages
                .iter()
                .flat_map(|page| &page.items)
                .all(|item| { !matches!(item, PageItem::Separator(_) | PageItem::Image(_)) })
        );

        let book = LayoutEngine::new()
            .layout_section(&source, &section, viewport, &ReaderStyle::default())
            .unwrap();
        assert!(
            !unified
                .pages
                .iter()
                .flat_map(|p| &p.items)
                .any(|item| matches!(item,PageItem::Text(text) if text.text.as_ref()=="* * *"))
        );
        assert!(
            book.pages
                .iter()
                .flat_map(|p| &p.items)
                .any(|item| matches!(item,PageItem::Text(text) if text.text.as_ref()=="* * *"))
        );
        assert!(
            book.pages
                .iter()
                .flat_map(|page| &page.items)
                .any(|item| matches!(item, PageItem::Separator(_)))
        );
        assert!(
            book.pages
                .iter()
                .flat_map(|page| &page.items)
                .any(|item| matches!(item, PageItem::Image(_)))
        );
    }

    #[test]
    fn minimum_paragraph_gap_only_expands_consecutive_prose_spacing() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("paragraph-gap-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let paragraph = |text: &str| {
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle {
                    margin_before: 0.0,
                    margin_after: 0.0,
                    ..rebook_publication::BlockStyle::default()
                },
                source: None,
            })
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![paragraph("First paragraph"), paragraph("Second paragraph")],
            anchors: Vec::new(),
        };
        let style = ReaderStyle {
            minimum_paragraph_gap: 12.0,
            ..ReaderStyle::default()
        };
        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(600, 400).unwrap(),
                &style,
            )
            .unwrap();
        let placements = layout.pages[0]
            .items
            .iter()
            .filter_map(|item| match item {
                PageItem::Text(text) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        let first_line = placements[0].layout.get(0).unwrap();
        let first_bottom = placements[0].origin_y + first_line.metrics().block_max_coord;

        assert!((placements[1].origin_y - first_bottom - 12.0).abs() < 0.001);
    }

    #[test]
    fn paragraph_margin_starts_after_the_complete_last_line_box() {
        use parley::editing::{Cursor, Selection};
        use parley::layout::Affinity;

        let source = EmptySource {
            book: Book {
                id: PublicationId::new("paragraph-line-box-gap-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let paragraph = |text: &str, margin_after: f32| {
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle {
                    line_height: 2.0,
                    margin_before: 0.0,
                    margin_after,
                    ..rebook_publication::BlockStyle::default()
                },
                source: None,
            })
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                paragraph("翻译后的中文段落使用不同字体指标。", 12.0),
                paragraph("下一个段落", 0.0),
            ],
            anchors: Vec::new(),
        };
        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(600, 400).unwrap(),
                &ReaderStyle::default(),
            )
            .unwrap();
        let placements = layout.pages[0]
            .items
            .iter()
            .filter_map(|item| match item {
                PageItem::Text(text) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        let first_line = placements[0].layout.get(0).unwrap();
        let first_metrics = first_line.metrics();
        let first_line_box_bottom =
            placements[0].origin_y + first_metrics.block_min_coord + first_metrics.line_height;
        let second_line = placements[1].layout.get(0).unwrap();
        let second_top = placements[1].origin_y + second_line.metrics().block_min_coord;

        assert!((second_top - first_line_box_bottom - 12.0).abs() < 0.001);

        let first_selection = Selection::new(
            Cursor::from_byte_index(&placements[0].layout, 0, Affinity::Downstream),
            Cursor::from_byte_index(
                &placements[0].layout,
                placements[0].text.len(),
                Affinity::Upstream,
            ),
        );
        let first_highlight_bottom = first_selection
            .geometry(&placements[0].layout)
            .into_iter()
            .map(|(rect, _)| rect.y1 as f32 + placements[0].origin_y)
            .fold(f32::NEG_INFINITY, f32::max);
        let second_selection = Selection::new(
            Cursor::from_byte_index(&placements[1].layout, 0, Affinity::Downstream),
            Cursor::from_byte_index(
                &placements[1].layout,
                placements[1].text.len(),
                Affinity::Upstream,
            ),
        );
        let second_highlight_top = second_selection
            .geometry(&placements[1].layout)
            .into_iter()
            .map(|(rect, _)| rect.y0 as f32 + placements[1].origin_y)
            .fold(f32::INFINITY, f32::min);

        assert!((second_highlight_top - first_highlight_bottom - 12.0).abs() < 0.001);
    }

    #[test]
    fn inline_math_is_laid_out_as_a_non_text_raster_box() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("math-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![
                    Inline::Text(TextRun {
                        text: "Energy ".into(),
                        style: TextStyle::default(),
                        link: None,
                    }),
                    Inline::Math(MathRun {
                        latex: r"E=mc^2".into(),
                        display: false,
                        size_scale: 1.0,
                    }),
                ],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            })],
            anchors: Vec::new(),
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(600, 400).unwrap(),
                &ReaderStyle::default(),
            )
            .unwrap();
        let formula_count = layout
            .pages
            .iter()
            .flat_map(|page| page.items.iter())
            .filter_map(|item| match item {
                PageItem::Text(text) => Some(text.inline_images.len()),
                PageItem::Quote(_)
                | PageItem::Table(_)
                | PageItem::Image(_)
                | PageItem::Separator(_) => None,
            })
            .sum::<usize>();
        assert_eq!(formula_count, 1);
    }

    #[test]
    fn unified_tables_adapt_columns_wrap_and_center_cell_content() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("adaptive-table-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let cell = |text: &str| TableCell {
            text: TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            },
            authored_alignment: None,
            column_span: 1,
            row_span: 1,
            header: false,
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![Block::Table(TableBlock {
                rows: vec![TableRow {
                    cells: vec![
                        cell("ID"),
                        cell(
                            "A substantially longer description that must wrap inside its adaptive column.",
                        ),
                    ],
                }],
                source: None,
            })],
            anchors: Vec::new(),
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(420, 360).unwrap(),
                &style,
            )
            .unwrap();
        let table = layout
            .pages
            .iter()
            .flat_map(|page| &page.items)
            .find_map(|item| match item {
                PageItem::Table(table) => Some(table),
                _ => None,
            })
            .expect("table should be laid out");
        let [short, long] = table.cells.as_slice() else {
            panic!("expected two cells");
        };
        assert!(short.width < long.width);
        let short_text = short.text.as_ref().expect("short cell should have text");
        let long_text = long.text.as_ref().expect("long cell should have text");
        assert!(long_text.layout.len() > 1, "long content should wrap");
        assert!(
            short_text
                .layout
                .get(0)
                .is_some_and(|line| line.metrics().offset > 0.0),
            "short content should be horizontally centered"
        );
        assert!(
            short_text.origin_y > long_text.origin_y,
            "short content should be vertically centered beside wrapped content"
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "the table fixture verifies spans, formula layout, and pagination together"
    )]
    fn structured_tables_keep_spans_formulas_and_safe_page_breaks() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("table-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let cell = |text: &str, column_span, row_span, header| TableCell {
            text: TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            },
            authored_alignment: None,
            column_span,
            row_span,
            header,
        };
        let mut rows = vec![TableRow {
            cells: vec![cell("Header", 2, 1, true)],
        }];
        rows.push(TableRow {
            cells: vec![cell("Merged", 1, 2, false), cell("$", 1, 1, false)],
        });
        rows.push(TableRow {
            cells: vec![TableCell {
                text: TextBlock {
                    kind: TextBlockKind::Paragraph,
                    content: vec![Inline::Math(MathRun {
                        latex: "E=mc^2".into(),
                        display: false,
                        size_scale: 1.0,
                    })],
                    style: rebook_publication::BlockStyle::default(),
                    source: None,
                },
                authored_alignment: None,
                column_span: 1,
                row_span: 1,
                header: false,
            }],
        });
        rows.extend((0..12).map(|index| TableRow {
            cells: vec![
                cell(&format!("row {index}"), 1, 1, false),
                cell("value", 1, 1, false),
            ],
        }));
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![Block::Table(TableBlock { rows, source: None })],
            anchors: Vec::new(),
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(600, 240).unwrap(),
                &ReaderStyle::default(),
            )
            .unwrap();
        let tables = layout
            .pages
            .iter()
            .flat_map(|page| page.items.iter())
            .filter_map(|item| match item {
                PageItem::Table(table) => Some(table),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(tables.len() > 1, "long table should paginate");
        let header = tables[0]
            .cells
            .iter()
            .find(|cell| cell.header)
            .expect("header cell should be retained");
        let regular = tables
            .iter()
            .flat_map(|table| &table.cells)
            .find(|cell| !cell.header && cell.width < header.width)
            .expect("regular-width cell should exist");
        assert!((header.width - regular.width * 2.0).abs() < 0.1);
        assert!(tables.iter().any(|table| {
            table.cells.iter().any(|cell| {
                cell.text
                    .as_ref()
                    .is_some_and(|text| !text.inline_images.is_empty())
            })
        }));
        assert!(tables.iter().all(|table| {
            table
                .cells
                .iter()
                .all(|cell| cell.y + cell.height <= table.y + table.height + 0.1)
        }));
    }

    #[test]
    fn block_media_uses_the_dominant_paragraph_measure() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("media-measure-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let paragraph_style = rebook_publication::BlockStyle {
            margin_start: 32.0,
            ..rebook_publication::BlockStyle::default()
        };
        let text_block = |text: &str| TextBlock {
            kind: TextBlockKind::Paragraph,
            content: vec![Inline::Text(TextRun {
                text: text.into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: paragraph_style,
            source: None,
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                Block::Text(text_block("Body paragraph")),
                Block::Table(TableBlock {
                    rows: vec![TableRow {
                        cells: vec![TableCell {
                            text: text_block("Cell"),
                            authored_alignment: None,
                            column_span: 1,
                            row_span: 1,
                            header: false,
                        }],
                    }],
                    source: None,
                }),
                Block::Image(ImageBlock {
                    href: PublicationUrl::parse("figure.png").unwrap(),
                    alt: "Figure".into(),
                    style: ImageStyle {
                        width: Some(ImageLength::Fraction(1.0)),
                        ..ImageStyle::default()
                    },
                    source: None,
                    text_layer: None,
                }),
            ],
            anchors: Vec::new(),
        };
        let viewport = LayoutViewport::new(600, 500).unwrap();
        let style = ReaderStyle::default();
        let geometry = resolve_page_geometry(600.0, 500.0, &style);
        let layout = LayoutEngine::new()
            .layout_section(&source, &section, viewport, &style)
            .unwrap();
        let items = layout.pages.iter().flat_map(|page| &page.items);
        let mut text_x = None;
        let mut table_bounds = None;
        let mut image_bounds = None;
        for item in items {
            match item {
                PageItem::Text(text) => {
                    text_x.get_or_insert(text.origin_x);
                }
                PageItem::Table(table) => {
                    let first = table.cells.first().unwrap();
                    table_bounds = Some((first.x, first.x + first.width));
                }
                PageItem::Image(image) => {
                    image_bounds = Some((image.x, image.x + image.width));
                }
                PageItem::Quote(_) => {}
                PageItem::Separator(_) => {}
            }
        }
        let expected_left = geometry.left + 32.0;
        let expected_right = geometry.left + geometry.width;
        assert!((text_x.unwrap() - expected_left).abs() < 0.001);
        let (table_left, table_right) = table_bounds.unwrap();
        assert!((table_left - expected_left).abs() < 0.001);
        assert!((table_right - expected_right).abs() < 0.001);
        let (image_left, image_right) = image_bounds.unwrap();
        assert!((image_left - expected_left).abs() < 0.001);
        assert!((image_right - expected_right).abs() < 0.001);
    }

    #[test]
    fn block_start_fraction_tracks_the_available_content_width() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let text_block = |text: &str, style| {
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(rebook_publication::TextRun {
                    text: text.into(),
                    style: TextStyle::default(),
                    link: None,
                })],
                style,
                source: None,
            })
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                text_block("Top-level entry", rebook_publication::BlockStyle::default()),
                text_block(
                    "Nested entry",
                    rebook_publication::BlockStyle {
                        margin_start: 12.0,
                        margin_start_fraction: 0.1,
                        ..rebook_publication::BlockStyle::default()
                    },
                ),
            ],
            anchors: Vec::new(),
        };
        let viewport = LayoutViewport::new(600, 400).unwrap();
        let reader_style = ReaderStyle::default();
        let content_width = resolve_page_geometry(600.0, 400.0, &reader_style).width;
        let layout = LayoutEngine::new()
            .layout_section(&source, &section, viewport, &reader_style)
            .unwrap();
        let origins = layout.pages[0]
            .items
            .iter()
            .filter_map(|item| match item {
                PageItem::Text(text) => Some(text.origin_x),
                PageItem::Quote(_)
                | PageItem::Table(_)
                | PageItem::Image(_)
                | PageItem::Separator(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(origins.len(), 2);
        let expected_offset = 12.0 + content_width * 0.1;
        assert!(((origins[1] - origins[0]) - expected_offset).abs() < 0.001);
    }

    #[test]
    fn double_spread_emits_independent_logical_pages_for_composition() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                content: vec![Inline::Text(rebook_publication::TextRun {
                    text: "双栏分页应当把连续内容放进同一屏幕的左右页面。".repeat(500),
                    style: TextStyle::default(),
                    link: None,
                })],
                style: rebook_publication::BlockStyle::default(),
                source: None,
            })],
            anchors: Vec::new(),
        };
        let viewport = LayoutViewport::new(900, 700).unwrap();
        let single = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                viewport,
                &ReaderStyle {
                    spread: SpreadMode::Single,
                    ..ReaderStyle::default()
                },
            )
            .unwrap();
        let double = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                viewport,
                &ReaderStyle {
                    spread: SpreadMode::Double,
                    ..ReaderStyle::default()
                },
            )
            .unwrap();

        assert_eq!(single.visible_pages, 1);
        assert_eq!(double.visible_pages, 2);
        assert!(double.pages.len() >= 2);
        let first_origin = double.pages[0]
            .items
            .iter()
            .find_map(|item| match item {
                PageItem::Text(text) => Some(text.origin_x),
                _ => None,
            })
            .expect("first logical page should contain text");
        let second_origin = double.pages[1]
            .items
            .iter()
            .find_map(|item| match item {
                PageItem::Text(text) => Some(text.origin_x),
                _ => None,
            })
            .expect("second logical page should contain text");
        assert!((first_origin - second_origin).abs() < f32::EPSILON);
        assert!(double.continuation_offset_x > 0.0);
    }

    #[test]
    fn image_css_dimensions_are_resolved_and_aspect_ratio_is_preserved() {
        let viewport = LayoutViewport::new(400, 500).unwrap();
        let mut paginator = Paginator::new(
            viewport,
            Rgba::BLACK,
            PageGeometry {
                left: 0.0,
                top: 0.0,
                width: 400.0,
                bottom: 500.0,
                visible_pages: 1,
                continuation_offset_x: 0.0,
            },
            false,
            0.0,
        );
        paginator.push_image(
            RasterImage {
                width: 800,
                height: 600,
                pixels: Vec::new().into(),
            },
            ImageStyle {
                width: Some(ImageLength::Fraction(0.8)),
                max_width: Some(ImageLength::Pixels(250.0)),
                ..ImageStyle::default()
            },
            None,
            None,
        );

        let pages = paginator.finish();
        let PageItem::Image(image) = &pages[0].items[0] else {
            panic!("expected an image placement");
        };
        assert!((image.width - 250.0).abs() < 0.001);
        assert!((image.height - 187.5).abs() < 0.001);
        assert!((image.x - 75.0).abs() < 0.001);
    }

    #[test]
    fn image_after_zero_margin_text_keeps_a_minimum_block_gap() {
        let image_href = PublicationUrl::parse("images/figure.png").unwrap();
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("image-gap-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                Block::Text(TextBlock {
                    kind: TextBlockKind::Paragraph,
                    content: vec![Inline::Text(rebook_publication::TextRun {
                        text: "Text immediately before a figure.".into(),
                        style: TextStyle::default(),
                        link: None,
                    })],
                    style: rebook_publication::BlockStyle {
                        margin_after: 0.0,
                        ..rebook_publication::BlockStyle::default()
                    },
                    source: None,
                }),
                Block::Image(ImageBlock {
                    href: image_href,
                    alt: "Figure".into(),
                    style: ImageStyle::default(),
                    source: None,
                    text_layer: None,
                }),
            ],
            anchors: Vec::new(),
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(400, 500).unwrap(),
                &ReaderStyle::default(),
            )
            .unwrap();
        let [PageItem::Text(text), PageItem::Image(image)] = layout.pages[0].items.as_slice()
        else {
            panic!("expected text followed by an image");
        };
        let last_line = text.layout.get(text.lines.end - 1).unwrap();
        let metrics = last_line.metrics();
        let text_bottom = text.origin_y
            + metrics
                .block_max_coord
                .max(metrics.block_min_coord + metrics.line_height);

        assert!((image.y - text_bottom - IMAGE_BLOCK_GAP).abs() < 0.001);
    }

    #[test]
    fn unified_figure_keeps_image_and_caption_together_with_semantic_spacing() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("figure-caption-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![Block::Figure(rebook_publication::FigureBlock {
                images: vec![ImageBlock {
                    href: PublicationUrl::parse("images/figure.png").unwrap(),
                    alt: "Figure".into(),
                    style: ImageStyle {
                        margin_after: 30.0,
                        ..ImageStyle::default()
                    },
                    source: None,
                    text_layer: None,
                }],
                captions: vec![TextBlock {
                    kind: TextBlockKind::Caption,
                    content: vec![Inline::Text(TextRun {
                        text: "A concise figure caption".into(),
                        style: TextStyle::default(),
                        link: None,
                    })],
                    style: rebook_publication::BlockStyle::default(),
                    source: None,
                }],
                caption_position: CaptionPosition::After,
                style: rebook_publication::BlockStyle::default(),
                source: None,
            })],
            anchors: Vec::new(),
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(400, 500).unwrap(),
                &style,
            )
            .unwrap();
        let [PageItem::Image(image), PageItem::Text(caption)] = layout.pages[0].items.as_slice()
        else {
            panic!("expected one grouped image and caption");
        };
        let first = caption.layout.get(caption.lines.start).unwrap();
        let caption_top = caption.origin_y + first.metrics().block_min_coord;
        let expected_gap = style.typography.font_size * style.typesetting.caption_gap_em;
        assert!((caption_top - (image.y + image.height) - expected_gap).abs() < 0.01);
        assert!(
            first.metrics().offset > 0.0,
            "a short unified caption should be centered"
        );
    }

    #[test]
    fn inferred_adjacent_caption_is_grouped_only_by_unified_typesetting() {
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("inferred-caption-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                Block::Image(ImageBlock {
                    href: PublicationUrl::parse("images/figure.png").unwrap(),
                    alt: "Figure".into(),
                    style: ImageStyle::default(),
                    source: None,
                    text_layer: None,
                }),
                Block::Text(TextBlock {
                    kind: TextBlockKind::Caption,
                    content: vec![Inline::Text(TextRun {
                        text: "Figure 1. A leaf.".into(),
                        style: TextStyle::default(),
                        link: None,
                    })],
                    style: BlockStyle::default(),
                    source: None,
                }),
            ],
            anchors: Vec::new(),
        };
        let classic_style = ReaderStyle::default();
        let unified_style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        let viewport = LayoutViewport::new(400, 500).unwrap();

        let classic = LayoutEngine::new()
            .layout_section(&source, &section, viewport, &classic_style)
            .unwrap();
        let unified = LayoutEngine::new()
            .layout_section(&source, &section, viewport, &unified_style)
            .unwrap();
        let [PageItem::Image(_), PageItem::Text(classic_caption)] =
            classic.pages[0].items.as_slice()
        else {
            panic!("classic typesetting should preserve the two authored blocks");
        };
        let [
            PageItem::Image(unified_image),
            PageItem::Text(unified_caption),
        ] = unified.pages[0].items.as_slice()
        else {
            panic!("unified typesetting should lay out one inferred figure and caption");
        };
        let classic_line = classic_caption
            .layout
            .get(classic_caption.lines.start)
            .unwrap();
        let unified_line = unified_caption
            .layout
            .get(unified_caption.lines.start)
            .unwrap();
        assert!(classic_line.metrics().offset.abs() < 0.001);
        assert!(unified_line.metrics().offset > 0.0);
        let unified_caption_top = unified_caption.origin_y + unified_line.metrics().block_min_coord;
        let expected_gap =
            unified_style.typography.font_size * unified_style.typesetting.caption_gap_em;
        assert!(
            (unified_caption_top - (unified_image.y + unified_image.height) - expected_gap).abs()
                < 0.01
        );
    }

    #[test]
    fn unified_figure_captions_center_one_line_and_left_align_multiple_lines() {
        let caption = |text: &str| TextBlock {
            kind: TextBlockKind::Caption,
            content: vec![Inline::Text(TextRun {
                text: text.into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: None,
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };
        let mut engine = LayoutEngine::new();

        let short = caption("Figure 1. A leaf.");
        let (short, _) = engine.shape_figure_caption(&short, &style, 320.0, true);
        assert_eq!(short.layout.len(), 1);
        assert!(
            short
                .layout
                .get(0)
                .is_some_and(|line| line.metrics().offset > 0.0),
            "single-line caption should be centered"
        );

        let long = caption(
            "Figure 2. A deliberately long caption that wraps across several lines at this width.",
        );
        let (long, _) = engine.shape_figure_caption(&long, &style, 180.0, true);
        assert!(long.layout.len() > 1);
        assert!(
            (0..long.layout.len()).all(|index| long.layout.get(index).is_some_and(|line| line
                .metrics()
                .offset
                .abs()
                < 0.01)),
            "multi-line caption should be left aligned"
        );
        assert!(
            long.layout
                .lines()
                .take(long.layout.len().saturating_sub(1))
                .all(|line| (line.metrics().inline_max_coord - 180.0).abs() < 0.01),
            "multi-line caption should use optimized full-measure breaks"
        );
        assert!(
            long.layout
                .lines()
                .take(long.layout.len().saturating_sub(1))
                .all(|line| linebreak::parley::positioned_line_content_end(line) >= 179.0),
            "optimized caption lines should visually fill the shared measure"
        );
    }

    #[test]
    fn authored_image_margin_larger_than_the_default_gap_is_preserved() {
        let viewport = LayoutViewport::new(400, 500).unwrap();
        let mut paginator = Paginator::new(
            viewport,
            Rgba::BLACK,
            PageGeometry {
                left: 20.0,
                top: 40.0,
                width: 360.0,
                bottom: 460.0,
                visible_pages: 1,
                continuation_offset_x: 0.0,
            },
            false,
            0.0,
        );
        paginator.push_separator();
        paginator.push_image(
            RasterImage {
                width: 200,
                height: 100,
                pixels: Vec::new().into(),
            },
            ImageStyle {
                margin_before: 25.0,
                ..ImageStyle::default()
            },
            None,
            None,
        );

        let pages = paginator.finish();
        let [PageItem::Separator(separator), PageItem::Image(image)] = pages[0].items.as_slice()
        else {
            panic!("expected a separator followed by an image");
        };

        assert!((image.y - (separator.y + 1.0) - 25.0).abs() < 0.001);
    }

    #[test]
    fn image_moved_to_the_next_page_starts_at_the_page_margin() {
        let image_href = PublicationUrl::parse("images/figure.png").unwrap();
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("image-page-break-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                Block::Text(TextBlock {
                    kind: TextBlockKind::Paragraph,
                    content: vec![Inline::Text(rebook_publication::TextRun {
                        text: "Text before a figure that must move.".into(),
                        style: TextStyle::default(),
                        link: None,
                    })],
                    style: rebook_publication::BlockStyle {
                        margin_after: 0.0,
                        ..rebook_publication::BlockStyle::default()
                    },
                    source: None,
                }),
                Block::Image(ImageBlock {
                    href: image_href,
                    alt: "Figure".into(),
                    style: ImageStyle::default(),
                    source: None,
                    text_layer: None,
                }),
            ],
            anchors: Vec::new(),
        };
        let viewport = LayoutViewport::new(400, 140).unwrap();
        let style = ReaderStyle::default();
        let page_top = resolve_page_geometry(400.0, 140.0, &style).top;

        let layout = LayoutEngine::new()
            .layout_section(&source, &section, viewport, &style)
            .unwrap();
        let PageItem::Image(image) = &layout.pages[1].items[0] else {
            panic!("expected the image on the next page");
        };

        assert!((image.y - page_top).abs() < 0.001);
        assert!((layout.pages[1].leading_gap - IMAGE_BLOCK_GAP).abs() < 0.001);
    }

    #[test]
    fn image_keeps_its_gap_when_previous_block_spacing_already_advanced_the_page() {
        let image_href = PublicationUrl::parse("images/figure.png").unwrap();
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("image-pre-advanced-page-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                Block::Text(TextBlock {
                    kind: TextBlockKind::Paragraph,
                    content: vec![Inline::Text(rebook_publication::TextRun {
                        text: "Text before a figure.".into(),
                        style: TextStyle::default(),
                        link: None,
                    })],
                    style: rebook_publication::BlockStyle {
                        margin_after: 300.0,
                        ..rebook_publication::BlockStyle::default()
                    },
                    source: None,
                }),
                Block::Image(ImageBlock {
                    href: image_href,
                    alt: "Figure".into(),
                    style: ImageStyle::default(),
                    source: None,
                    text_layer: None,
                }),
            ],
            anchors: Vec::new(),
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(400, 300).unwrap(),
                &ReaderStyle::default(),
            )
            .unwrap();

        assert!(matches!(layout.pages[1].items[0], PageItem::Image(_)));
        assert!((layout.pages[1].leading_gap - IMAGE_BLOCK_GAP).abs() < 0.001);
    }

    #[test]
    fn oversized_figure_restores_its_outer_gap_after_moving_to_the_next_page() {
        let viewport = LayoutViewport::new(400, 300).unwrap();
        let mut paginator = Paginator::new(
            viewport,
            Rgba::BLACK,
            PageGeometry {
                left: 20.0,
                top: 40.0,
                width: 360.0,
                bottom: 260.0,
                visible_pages: 1,
                continuation_offset_x: 0.0,
            },
            false,
            0.0,
        );
        paginator.push_separator();
        paginator.add_spacing(180.0);

        let outer_gap = 20.0;
        paginator.prepare_group(300.0, outer_gap);
        paginator.push_image_with_gaps(
            RasterImage {
                width: 200,
                height: 100,
                pixels: Vec::new().into(),
            },
            ImageStyle::default(),
            None,
            None,
            0.0,
            0.0,
        );

        let pages = paginator.finish();
        assert_eq!(pages.len(), 2);
        assert!((pages[1].leading_gap - outer_gap).abs() < 0.001);
    }

    #[test]
    fn semantic_spacing_that_crosses_a_page_is_restored_in_continuous_layout() {
        let viewport = LayoutViewport::new(400, 300).unwrap();
        let mut paginator = Paginator::new(
            viewport,
            Rgba::BLACK,
            PageGeometry {
                left: 20.0,
                top: 40.0,
                width: 360.0,
                bottom: 260.0,
                visible_pages: 1,
                continuation_offset_x: 0.0,
            },
            false,
            0.0,
        );
        paginator.push_image_with_gaps(
            RasterImage {
                width: 200,
                height: 210,
                pixels: Vec::new().into(),
            },
            ImageStyle::default(),
            None,
            None,
            0.0,
            0.0,
        );

        let semantic_gap = 20.0;
        paginator.add_semantic_spacing(semantic_gap);
        paginator.push_separator();

        let pages = paginator.finish();
        assert_eq!(pages.len(), 2);
        assert!((pages[1].leading_gap - semantic_gap).abs() < 0.001);
    }

    #[test]
    fn authored_spacing_that_crosses_a_page_is_restored_in_continuous_layout() {
        let viewport = LayoutViewport::new(400, 300).unwrap();
        let mut paginator = Paginator::new(
            viewport,
            Rgba::BLACK,
            PageGeometry {
                left: 20.0,
                top: 40.0,
                width: 360.0,
                bottom: 260.0,
                visible_pages: 1,
                continuation_offset_x: 0.0,
            },
            false,
            0.0,
        );
        paginator.push_image_with_gaps(
            RasterImage {
                width: 200,
                height: 210,
                pixels: Vec::new().into(),
            },
            ImageStyle::default(),
            None,
            None,
            0.0,
            0.0,
        );

        paginator.add_preserved_spacing(15.0);
        paginator.add_preserved_spacing(5.0);
        paginator.push_image_with_gaps(
            RasterImage {
                width: 100,
                height: 50,
                pixels: Vec::new().into(),
            },
            ImageStyle::default(),
            None,
            None,
            0.0,
            0.0,
        );

        let pages = paginator.finish();
        assert_eq!(pages.len(), 2);
        assert!((pages[1].leading_gap - 20.0).abs() < 0.001);
    }

    #[test]
    fn trailing_block_spacing_survives_a_later_fit_page_break() {
        let viewport = LayoutViewport::new(400, 300).unwrap();
        let mut paginator = Paginator::new(
            viewport,
            Rgba::BLACK,
            PageGeometry {
                left: 20.0,
                top: 40.0,
                width: 360.0,
                bottom: 260.0,
                visible_pages: 1,
                continuation_offset_x: 0.0,
            },
            false,
            0.0,
        );
        paginator.push_image_with_gaps(
            RasterImage {
                width: 200,
                height: 210,
                pixels: Vec::new().into(),
            },
            ImageStyle::default(),
            None,
            None,
            0.0,
            10.0,
        );
        paginator.push_image_with_gaps(
            RasterImage {
                width: 100,
                height: 100,
                pixels: Vec::new().into(),
            },
            ImageStyle::default(),
            None,
            None,
            0.0,
            0.0,
        );

        let pages = paginator.finish();
        assert_eq!(pages.len(), 2);
        assert!((pages[1].leading_gap - 10.0).abs() < 0.001);
    }

    #[test]
    fn fixed_page_image_is_vertically_centered_in_the_content_area() {
        let viewport = LayoutViewport::new(400, 500).unwrap();
        let mut paginator = Paginator::new(
            viewport,
            Rgba::BLACK,
            PageGeometry {
                left: 20.0,
                top: 40.0,
                width: 360.0,
                bottom: 460.0,
                visible_pages: 1,
                continuation_offset_x: 0.0,
            },
            true,
            0.0,
        );
        paginator.push_image(
            RasterImage {
                width: 200,
                height: 100,
                pixels: Vec::new().into(),
            },
            ImageStyle::default(),
            None,
            None,
        );

        let pages = paginator.finish();
        let PageItem::Image(image) = &pages[0].items[0] else {
            panic!("expected an image placement");
        };
        assert!((image.y - 200.0).abs() < 0.001);
    }

    #[test]
    fn fixed_page_replacement_stays_on_the_original_page_image() {
        let href = PublicationUrl::parse("page-1.png").unwrap();
        let spine = SpineItemId::new("pdf-page-1").unwrap();
        let source_range = SourceRange {
            start: SourceAnchor {
                spine: spine.clone(),
                node: "pdf-page-text".into(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine,
                node: "pdf-page-text".into(),
                text_offset: 4,
            },
        };
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("fixed-page-translation").unwrap(),
                metadata: Metadata {
                    layout: RenditionLayout::PrePaginated,
                    ..Metadata::default()
                },
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let replacement_rect = FixedPageTextRect {
            x: 20.0,
            y: 10.0,
            width: 100.0,
            height: 40.0,
        };
        let section = Section {
            id: SpineItemId::new("pdf-page-1").unwrap(),
            href: PublicationUrl::parse("page-1.pdf").unwrap(),
            blocks: vec![Block::Image(ImageBlock {
                href,
                alt: "PDF page 1".into(),
                style: ImageStyle::default(),
                source: Some(source_range),
                text_layer: Some(FixedPageTextLayer {
                    width: 200.0,
                    height: 100.0,
                    text: "PDF text".into(),
                    spans: vec![FixedPageTextSpan {
                        char_range: 0..8,
                        rect: replacement_rect,
                    }],
                    replacement: Some(FixedPageTextReplacement {
                        segments: vec![FixedPageTextReplacementSegment {
                            text: "译文".into(),
                            rect: replacement_rect,
                            source_offset: 0,
                        }],
                    }),
                }),
            })],
            anchors: Vec::new(),
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(400, 500).unwrap(),
                &ReaderStyle::default(),
            )
            .unwrap();

        assert_eq!(layout.pages.len(), 1);
        let [PageItem::Image(image)] = layout.pages[0].items.as_slice() else {
            panic!("translation must remain attached to the fixed page image");
        };
        let replacement = image
            .replacement
            .as_ref()
            .expect("fixed page should retain its replacement overlay");
        let [segment] = replacement.segments.as_slice() else {
            panic!("expected one translated fixed-page segment");
        };
        assert!(segment.rect.x >= image.x);
        assert!(segment.rect.y >= image.y);
        assert!(segment.rect.x + segment.rect.width <= image.x + image.width);
        assert!(segment.rect.y + segment.rect.height <= image.y + image.height);
        assert_eq!(segment.text.text.as_ref(), "译文");
    }

    #[test]
    fn reflowable_standalone_cover_is_vertically_centered() {
        let cover = PublicationUrl::parse("images/cover.jpg").unwrap();
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("cover-test").unwrap(),
                metadata: Metadata::default(),
                cover: Some(cover.clone()),
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("cover").unwrap(),
            href: PublicationUrl::parse("cover.xhtml").unwrap(),
            blocks: vec![Block::Image(ImageBlock {
                href: cover,
                alt: "Cover".into(),
                style: ImageStyle::default(),
                source: None,
                text_layer: None,
            })],
            anchors: Vec::new(),
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(400, 500).unwrap(),
                &ReaderStyle::default(),
            )
            .unwrap();
        let PageItem::Image(image) = &layout.pages[0].items[0] else {
            panic!("expected a cover image placement");
        };

        let style = ReaderStyle::default();
        let geometry = resolve_page_geometry(400.0, 500.0, &style);
        let expected_y = geometry.top + (geometry.bottom - geometry.top - image.height) / 2.0;
        assert!((image.y - expected_y).abs() < 0.001);
    }

    #[test]
    fn reflowable_standalone_non_cover_image_stays_in_normal_flow() {
        let image_href = PublicationUrl::parse("images/illustration.jpg").unwrap();
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("illustration-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let section = Section {
            id: SpineItemId::new("illustration").unwrap(),
            href: PublicationUrl::parse("illustration.xhtml").unwrap(),
            blocks: vec![Block::Image(ImageBlock {
                href: image_href,
                alt: "Illustration".into(),
                style: ImageStyle::default(),
                source: None,
                text_layer: None,
            })],
            anchors: Vec::new(),
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(400, 500).unwrap(),
                &ReaderStyle::default(),
            )
            .unwrap();
        let PageItem::Image(image) = &layout.pages[0].items[0] else {
            panic!("expected an illustration image placement");
        };

        assert!((image.y - ReaderStyle::default().top_margin).abs() < 0.001);
    }

    #[test]
    fn unified_quote_is_one_padded_card_with_right_aligned_attribution() {
        let spine = SpineItemId::new("chapter").unwrap();
        let range = |node: &str, length: u64| SourceRange {
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
        let text = |kind, value: &str, source| TextBlock {
            kind,
            content: vec![Inline::Text(TextRun {
                text: value.into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: Some(source),
        };
        let body_range = range("quote-body", 18);
        let attribution_range = range("quote-source", 12);
        let quote_range = SourceRange {
            start: body_range.start.clone(),
            end: attribution_range.end.clone(),
        };
        let section = Section {
            id: spine.clone(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![Block::Quote(QuoteBlock {
                body: vec![text(
                    TextBlockKind::Blockquote,
                    "A structural quote.",
                    body_range,
                )],
                attribution: Some(text(
                    TextBlockKind::QuoteAttribution,
                    "The source",
                    attribution_range,
                )),
                source: Some(quote_range),
            })],
            anchors: Vec::new(),
        };
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("quote-layout-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(420, 360).unwrap(),
                &style,
            )
            .unwrap();
        let page = &layout.pages[0];
        let quote = page
            .items
            .iter()
            .find_map(|item| match item {
                PageItem::Quote(quote) => Some(quote),
                _ => None,
            })
            .expect("quote card should be positioned");
        let texts = page
            .items
            .iter()
            .filter_map(|item| match item {
                PageItem::Text(text) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(texts.len(), 2);
        assert_eq!(quote.sources.len(), 2);
        let expected_quote_padding = style.typography.font_size
            * paragraph_indent_em(&style.typesetting, style.writing_system);
        assert!(texts[0].origin_x >= quote.x + expected_quote_padding);
        assert!(quote.height > QUOTE_VERTICAL_PADDING * 2.0);
        assert!(
            texts[1]
                .layout
                .get(0)
                .is_some_and(|line| line.metrics().offset > 0.0),
            "attribution should align to the inline end"
        );

        let solo_range = range("quote-without-source", 26);
        let solo_section = Section {
            id: spine.clone(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![Block::Quote(QuoteBlock {
                body: vec![text(
                    TextBlockKind::Blockquote,
                    "A quotation without a source.",
                    solo_range.clone(),
                )],
                attribution: None,
                source: Some(solo_range),
            })],
            anchors: Vec::new(),
        };
        let solo_layout = LayoutEngine::new()
            .layout_section(
                &source,
                &solo_section,
                LayoutViewport::new(420, 360).unwrap(),
                &style,
            )
            .unwrap();
        let solo_page = &solo_layout.pages[0];
        let solo_quote = solo_page
            .items
            .iter()
            .find_map(|item| match item {
                PageItem::Quote(quote) => Some(quote),
                _ => None,
            })
            .expect("source-free quote card should be positioned");
        let solo_text = solo_page
            .items
            .iter()
            .find_map(|item| match item {
                PageItem::Text(text) => Some(text),
                _ => None,
            })
            .expect("source-free quote body should be positioned");
        let last_line = solo_text
            .lines
            .end
            .checked_sub(1)
            .and_then(|index| solo_text.layout.get(index))
            .expect("source-free quote should have a visible line");
        let metrics = last_line.metrics();
        let text_bottom = solo_text.origin_y
            + metrics
                .block_max_coord
                .max(metrics.block_min_coord + metrics.line_height);
        let bottom_padding = solo_quote.y + solo_quote.height - text_bottom;
        assert!(
            (bottom_padding - QUOTE_VERTICAL_PADDING).abs() < 0.01,
            "source-free quote should have one bottom padding, got {bottom_padding}"
        );

        let before_range = range("before-quote", 13);
        let balanced_quote_range = range("balanced-quote", 17);
        let after_range = range("after-quote", 12);
        let balanced_section = Section {
            id: solo_section.id.clone(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                Block::Text(text(
                    TextBlockKind::Paragraph,
                    "Before quote.",
                    before_range,
                )),
                Block::Quote(QuoteBlock {
                    body: vec![text(
                        TextBlockKind::Blockquote,
                        "Balanced quotation.",
                        balanced_quote_range.clone(),
                    )],
                    attribution: None,
                    source: Some(balanced_quote_range),
                }),
                Block::Text(text(TextBlockKind::Paragraph, "After quote.", after_range)),
            ],
            anchors: Vec::new(),
        };
        let balanced_layout = LayoutEngine::new()
            .layout_section(
                &source,
                &balanced_section,
                LayoutViewport::new(420, 360).unwrap(),
                &style,
            )
            .unwrap();
        let balanced_page = &balanced_layout.pages[0];
        let balanced_quote = balanced_page
            .items
            .iter()
            .find_map(|item| match item {
                PageItem::Quote(quote) => Some(quote),
                _ => None,
            })
            .expect("balanced quote card should be positioned");
        let balanced_texts = balanced_page
            .items
            .iter()
            .filter_map(|item| match item {
                PageItem::Text(text) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(balanced_texts.len(), 3);
        let text_top = |text: &TextPlacement| {
            let first = text
                .layout
                .get(text.lines.start)
                .expect("text placement should have a first line");
            text.origin_y + first.metrics().block_min_coord
        };
        let text_bottom = |text: &TextPlacement| {
            let last = text
                .lines
                .end
                .checked_sub(1)
                .and_then(|index| text.layout.get(index))
                .expect("text placement should have a last line");
            let metrics = last.metrics();
            text.origin_y
                + metrics
                    .block_max_coord
                    .max(metrics.block_min_coord + metrics.line_height)
        };
        let margin_before = balanced_quote.y - text_bottom(balanced_texts[0]);
        let margin_after = text_top(balanced_texts[2]) - (balanced_quote.y + balanced_quote.height);
        assert!(
            (margin_before - margin_after).abs() < 0.01,
            "quote margins should be symmetric, got {margin_before} before and {margin_after} after"
        );
    }

    #[test]
    fn unified_short_quote_keeps_its_attribution_on_the_same_page() {
        let spine = SpineItemId::new("chapter").unwrap();
        let range = |node: &str, length: u64| SourceRange {
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
        let text = |kind, value: &str, source| TextBlock {
            kind,
            content: vec![Inline::Text(TextRun {
                text: value.into(),
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: Some(source),
        };
        // Nine quote-owned rules leave enough room for the quote body but not its
        // attribution. The complete short quote should move as one unit.
        let mut blocks = vec![Block::Separator(SeparatorBlock::rule_in_quote()); 9];
        blocks.push(Block::Quote(QuoteBlock {
            body: vec![text(
                TextBlockKind::Blockquote,
                "Reading entails intense mental activity; thoughtful readers pause and reflect.",
                range("quote-body", 79),
            )],
            attribution: Some(text(
                TextBlockKind::QuoteAttribution,
                "Mortimer Adler",
                range("quote-source", 14),
            )),
            source: None,
        }));
        let section = Section {
            id: spine,
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks,
            anchors: Vec::new(),
        };
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("quote-keep-together-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        let layout = LayoutEngine::new()
            .layout_section(
                &source,
                &section,
                LayoutViewport::new(420, 360).unwrap(),
                &style,
            )
            .unwrap();
        let page_for = |node: &str| {
            layout.pages.iter().position(|page| {
                page.items.iter().any(|item| {
                    let PageItem::Text(text) = item else {
                        return false;
                    };
                    text.source
                        .as_ref()
                        .is_some_and(|source| source.start.node == node)
                })
            })
        };
        let body_page = page_for("quote-body").expect("quote body should be laid out");
        let source_page = page_for("quote-source").expect("quote attribution should be laid out");
        assert_eq!(body_page, source_page);
        assert!(body_page > 0, "awkward remainder should move the quote");
    }

    #[test]
    fn unified_quote_never_leaves_an_orphaned_accent_bar_at_a_page_boundary() {
        let spine = SpineItemId::new("chapter").unwrap();
        let range = |node: &str, length: u64| SourceRange {
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
        let text = |kind, value: String, source| TextBlock {
            kind,
            content: vec![Inline::Text(TextRun {
                text: value,
                style: TextStyle::default(),
                link: None,
            })],
            style: rebook_publication::BlockStyle::default(),
            source: Some(source),
        };
        let section = Section {
            id: spine.clone(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            blocks: vec![
                Block::Text(text(
                    TextBlockKind::Paragraph,
                    "A preceding paragraph fills the page before the quotation. ".repeat(18),
                    range("preceding", 1_026),
                )),
                Block::Quote(QuoteBlock {
                    body: vec![text(
                        TextBlockKind::Blockquote,
                        "The quotation must begin together with its accent bar.".into(),
                        range("quote-body", 55),
                    )],
                    attribution: Some(text(
                        TextBlockKind::QuoteAttribution,
                        "The source".into(),
                        range("quote-source", 10),
                    )),
                    source: None,
                }),
            ],
            anchors: Vec::new(),
        };
        let source = EmptySource {
            book: Book {
                id: PublicationId::new("quote-boundary-test").unwrap(),
                metadata: Metadata::default(),
                cover: None,
                sections: Vec::new(),
                table_of_contents: Vec::new(),
            },
        };
        let style = ReaderStyle {
            typesetting: ReaderTypesetting::unified(),
            ..ReaderStyle::default()
        };

        for height in (220..=420).step_by(8) {
            let layout = LayoutEngine::new()
                .layout_section(
                    &source,
                    &section,
                    LayoutViewport::new(420, height).unwrap(),
                    &style,
                )
                .unwrap();
            for page in &layout.pages {
                if !page
                    .items
                    .iter()
                    .any(|item| matches!(item, PageItem::Quote(_)))
                {
                    continue;
                }
                let has_quote_text = page.items.iter().any(|item| {
                    let PageItem::Text(text) = item else {
                        return false;
                    };
                    text.source.as_ref().is_some_and(|source| {
                        matches!(source.start.node.as_str(), "quote-body" | "quote-source")
                    })
                });
                assert!(
                    has_quote_text,
                    "viewport height {height} produced a quote decoration without quote text"
                );
            }
        }
    }
