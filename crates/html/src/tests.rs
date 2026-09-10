    use super::*;

    fn collect_text_blocks<'a>(blocks: &'a [Block], output: &mut Vec<&'a TextBlock>) {
        for block in blocks {
            match block {
                Block::Text(block) => output.push(block),
                Block::Note(note) => collect_text_blocks(&note.blocks, output),
                _ => {}
            }
        }
    }

    fn all_text_blocks(section: &Section) -> Vec<&TextBlock> {
        let mut blocks = Vec::new();
        collect_text_blocks(&section.blocks, &mut blocks);
        blocks
    }

    fn text_block_text(block: &TextBlock) -> String {
        block
            .content
            .iter()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run.text.as_str()),
                Inline::Math(run) => Some(run.latex.as_str()),
                Inline::Image(_) => None,
                Inline::Break => Some("\n"),
            })
            .collect()
    }

    fn note_text(note: &NoteBlock) -> String {
        note.blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(text_block_text(block)),
                Block::Note(note) => Some(note_text(note)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn preserves_authored_indent_except_for_normalized_list_items() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .indented { text-indent: 2em; }
            .bullet { margin-left: 2em; text-indent: -1em; }
        </style></head><body>
            <h2 class="indented">Heading</h2>
            <div class="indented">Container prose</div>
            <p class="indented">Paragraph prose</p>
            <p class="bullet"><span class="enumerator">•</span> List item</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let text_blocks = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(block),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(text_blocks.len(), 4);
        assert!(matches!(text_blocks[0].kind, TextBlockKind::Heading(2)));
        assert!(text_blocks[0].style.indent > 0.0);
        assert_eq!(text_blocks[1].kind, TextBlockKind::Paragraph);
        assert!(text_blocks[1].style.indent > 0.0);
        assert_eq!(text_blocks[2].kind, TextBlockKind::Paragraph);
        assert!(text_blocks[2].style.indent > 0.0);
        assert!(matches!(
            text_blocks[3].kind,
            TextBlockKind::ListItem { .. }
        ));
        assert!(text_blocks[3].style.indent.abs() < f32::EPSILON);
    }

    #[test]
    fn applies_external_class_styles_and_inline_cascade() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><link rel="stylesheet" href="styles/book.css"/></head>
            <body><div class="centered"><p class="body" style="text-align:right">
                Hello <span class="emphasis">world</span>
            </p></div><img class="figure" src="images/chart.png" width="640"/></body>
        </html>"#;
        let css = r"
            .centered { text-align: center; }
            p.body {
                font-size: 1.25em;
                line-height: 1.8em;
                margin: 2em 0 1em;
                text-indent: 2em;
            }
            .emphasis { font-weight: bold; color: #123456; }
            img.figure { width: 80%; max-width: 420px; max-height: 60%; }
        ";
        let mut loaded = false;

        let section = parse_section(xml, &descriptor, |href| {
            loaded = true;
            assert_eq!(href.path(), "OPS/styles/book.css");
            Some(css.into())
        })
        .unwrap();

        assert!(loaded);
        let Some(Block::Text(block)) = section.blocks.first() else {
            panic!("expected a text block");
        };
        assert_eq!(block.style.align, TextAlignment::End);
        assert_eq!(block.style.authored_alignment, Some(TextAlignment::End));
        assert_close(block.style.line_height, 1.8);
        assert_close(block.style.margin_before, 32.0);
        assert_close(block.style.margin_after, 16.0);
        assert_close(block.style.indent, 32.0);
        let Inline::Text(regular) = &block.content[0] else {
            panic!("expected regular text");
        };
        assert_close(regular.style.size_scale, 1.25);
        let Inline::Text(emphasis) = &block.content[1] else {
            panic!("expected emphasized text");
        };
        assert!(emphasis.style.bold);
        assert_eq!(emphasis.style.color.red, 0x12);
        assert_eq!(emphasis.style.color.green, 0x34);
        assert_eq!(emphasis.style.color.blue, 0x56);
        let Some(Block::Image(image)) = section.blocks.get(1) else {
            panic!("expected an image block");
        };
        assert_eq!(image.style.width, Some(ImageLength::Fraction(0.8)));
        assert_eq!(image.style.max_width, Some(ImageLength::Pixels(420.0)));
        assert_eq!(image.style.max_height, Some(ImageLength::Fraction(0.6)));
    }

    #[test]
    fn preserves_alignment_from_a_sole_block_inline_wrapper() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><style>
                .signature { display: block; text-align: right; }
            </style></head>
            <body>
                <p><span><span class="signature">Visual memo no. 100</span></span></p>
                <p>Following prose</p>
            </body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| None).unwrap();
        let blocks = all_text_blocks(&section);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].style.align, TextAlignment::End);
        assert_eq!(blocks[0].style.authored_alignment, Some(TextAlignment::End));
        assert_eq!(blocks[1].style.align, TextAlignment::Start);
        assert_eq!(blocks[1].style.authored_alignment, None);
    }

    #[test]
    fn table_cells_record_authored_alignment_and_leave_unspecified_cells_unset() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><style>
                .left { text-align: left; }
                .right { text-align: right; }
            </style></head>
            <body><table><tr>
                <td><p class="left">Left paragraph</p></td>
                <td class="right">Right cell</td>
                <td>Default cell</td>
            </tr></table></body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| None).unwrap();
        let [Block::Table(table)] = section.blocks.as_slice() else {
            panic!("expected one table");
        };
        let [left, right, default] = table.rows[0].cells.as_slice() else {
            panic!("expected three cells");
        };

        assert_eq!(left.authored_alignment, Some(TextAlignment::Start));
        assert_eq!(right.authored_alignment, Some(TextAlignment::End));
        assert_eq!(default.authored_alignment, None);
    }

    #[test]
    fn table_cells_keep_nested_list_paragraphs_on_separate_lines() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <table><tr><td><div>
                <p class="bullet">• First item</p>
                <p class="bullet">• Second item</p>
                <p class="bullet">• Third item</p>
            </div></td></tr></table>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| None).unwrap();
        let [Block::Table(table)] = section.blocks.as_slice() else {
            panic!("expected one table");
        };
        let content = &table.rows[0].cells[0].text.content;
        let breaks = content
            .iter()
            .filter(|inline| matches!(inline, Inline::Break))
            .count();
        let lines = content
            .iter()
            .map(|inline| match inline {
                Inline::Text(run) => run.text.as_str(),
                Inline::Break => "\n",
                Inline::Math(_) | Inline::Image(_) => "",
            })
            .collect::<String>();

        assert_eq!(breaks, 2);
        assert_eq!(lines, "• First item\n• Second item\n• Third item");
    }

    #[test]
    fn preserves_superscript_and_subscript_baselines() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><style>.css-super { vertical-align: super; font-size: 80%; }</style></head>
            <body><p>read.<a href="notes.xhtml#note-4"><sup>4</sup></a>
                H<sub>2</sub>O <span class="css-super">5</span></p></body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Text(block)] = section.blocks.as_slice() else {
            panic!("expected one text block");
        };
        let styled = block
            .content
            .iter()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some((run.text.trim(), run.style)),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<Vec<_>>();

        assert!(styled.iter().any(|(text, style)| {
            *text == "4"
                && style.baseline == TextBaseline::Superscript
                && (style.size_scale - 0.75).abs() < 0.001
        }));
        assert!(styled.iter().any(|(text, style)| {
            *text == "2"
                && style.baseline == TextBaseline::Subscript
                && (style.size_scale - 0.75).abs() < 0.001
        }));
        assert!(styled.iter().any(|(text, style)| {
            *text == "5"
                && style.baseline == TextBaseline::Superscript
                && (style.size_scale - 0.8).abs() < 0.001
        }));
    }

    #[test]
    fn preserves_non_breaking_spaces_while_collapsing_html_whitespace() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p>2015年，Dark&#160;Reading
                报道</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let Block::Text(paragraph) = &section.blocks[0] else {
            panic!("expected paragraph");
        };
        let text = paragraph
            .content
            .iter()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run.text.as_str()),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<String>();
        assert_eq!(text, "2015年，Dark\u{00a0}Reading 报道");
    }

    #[test]
    fn classifies_only_supported_inline_footnote_classes() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p>Body<span class="footnote">First note</span>
                <span class="minor footnote1">Second note</span>
                <span class="footnote2">Block-style note</span></p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Text(block)] = section.blocks.as_slice() else {
            panic!("expected one text block");
        };
        let roles = block
            .content
            .iter()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some((run.text.trim(), run.style.inline_role)),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<Vec<_>>();

        assert!(roles.contains(&("First note", InlineRole::Footnote)));
        assert!(roles.contains(&("Second note", InlineRole::Footnote)));
        assert!(roles.contains(&("Block-style note", InlineRole::Normal)));
    }

    #[test]
    fn classifies_explicit_epub_footnote_links_without_superscript() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"
            xmlns:epub="http://www.idpf.org/2007/ops"><body>
            <p>Body<a id="ref-1" href="#note-1" epub:type="noteref">[1]</a></p>
            <aside epub:type="footnote"><p><a id="note-1" href="#ref-1"
                role="doc-backlink">[1]</a>Definition</p></aside>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let runs = all_text_blocks(&section)
            .into_iter()
            .map(|block| &block.content)
            .flatten()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<Vec<_>>();

        assert!(runs.iter().any(|run| {
            run.text == "[1]"
                && run.style.baseline == TextBaseline::Normal
                && run.style.link_role == LinkRole::FootnoteReference
        }));
        assert!(runs.iter().any(|run| {
            run.text == "[1]"
                && run.style.baseline == TextBaseline::Normal
                && run.style.link_role == LinkRole::FootnoteBacklink
        }));
    }

    #[test]
    fn parses_image_noteref_and_marks_unlinked_epub_footnote_definition() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("EPUB/xhtml/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"
            xmlns:epub="http://www.idpf.org/2007/ops"><body>
            <aside epub:type="footnote" id="footnote-18-20">
                <ol class="duokan-footnote-content">
                    <li class="duokan-footnote-item">国际知名的演说家、作家。——译者注</li>
                </ol>
            </aside>
            <p>——齐格·金克拉（Zig Ziglar）网站&#160;
                <sup><a epub:type="noteref" href="#footnote-18-20"> <img
                src="../images/image_010.png"
                alt="国际知名的演说家、作家。——译者注"
                zy-footnote="国际知名的演说家、作家。——译者注"
                class="epub-footnote"/></a></sup>以及其他网站</p>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        assert!(
            !section
                .blocks
                .iter()
                .any(|block| matches!(block, Block::Image(_)))
        );

        let definition = all_text_blocks(&section)
            .into_iter()
            .find(|block| block.kind == TextBlockKind::FootnoteDefinition)
            .expect("semantic footnote definition");
        assert!(definition.content.iter().any(|inline| {
            matches!(inline, Inline::Text(run) if run.text.contains("国际知名的演说家"))
        }));

        let reference = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(&block.content),
                _ => None,
            })
            .flatten()
            .find_map(|inline| match inline {
                Inline::Text(run) if run.style.link_role == LinkRole::FootnoteReference => {
                    Some(run)
                }
                Inline::Text(_) | Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .expect("image-backed footnote reference");
        assert_eq!(reference.text, "译");
        assert_eq!(reference.style.baseline, TextBaseline::Superscript);
        assert_eq!(
            reference.link.as_ref().and_then(PublicationUrl::fragment),
            Some("footnote-18-20")
        );
        let paragraph_text = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) if block.kind == TextBlockKind::Paragraph => Some(block),
                _ => None,
            })
            .flat_map(|block| &block.content)
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run.text.as_str()),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<String>();
        assert!(paragraph_text.contains("网站译以及"));
    }

    #[test]
    fn classifies_legacy_reciprocal_bracketed_footnotes() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p>Authored prose.<a id="q0d3" href="#h0d3">【3】</a></p>
            <p><a id="h0d3" href="#q0d3">【3】</a>Footnote definition.</p>
            <p>Ordinary <a href="#section">[section]</a> link.</p>
            <h2 id="section">Section</h2>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let runs = all_text_blocks(&section)
            .into_iter()
            .map(|block| &block.content)
            .flatten()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<Vec<_>>();

        assert!(runs.iter().any(|run| {
            run.text == "【3】" && run.style.link_role == LinkRole::FootnoteReference
        }));
        assert!(runs.iter().any(|run| {
            run.text == "【3】" && run.style.link_role == LinkRole::FootnoteBacklink
        }));
        assert!(
            runs.iter()
                .any(|run| { run.text == "[section]" && run.style.link_role == LinkRole::Normal })
        );
    }

    #[test]
    fn classifies_reciprocal_footnote_with_target_on_container() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p>Authored prose.<a id="ref-5" href="#note-5"><sup>[5]</sup></a></p>
            <div id="note-5"><p><a href="#ref-5"><sup>[5] </sup></a>Definition.</p></div>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let runs = all_text_blocks(&section)
            .into_iter()
            .map(|block| &block.content)
            .flatten()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<Vec<_>>();

        assert!(runs.iter().any(|run| {
            run.text.trim() == "[5]" && run.style.link_role == LinkRole::FootnoteReference
        }));
        assert!(runs.iter().any(|run| {
            run.text.trim() == "[5]" && run.style.link_role == LinkRole::FootnoteBacklink
        }));
    }

    #[test]
    fn classifies_reciprocal_footnote_with_split_reference_anchor() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p>Authored prose.<a id="ref-1"/><a href="#note-1"><sup>*</sup></a></p>
            <div><p id="note-1"><a href="#ref-1"><sup>*</sup></a>Footnote definition.</p></div>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let runs = all_text_blocks(&section)
            .into_iter()
            .flat_map(|block| &block.content)
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<Vec<_>>();

        assert!(
            runs.iter().any(|run| {
                run.text == "*" && run.style.link_role == LinkRole::FootnoteReference
            })
        );
        assert!(
            runs.iter().any(|run| {
                run.text == "*" && run.style.link_role == LinkRole::FootnoteBacklink
            })
        );
        assert!(section.blocks.iter().any(|block| matches!(
            block,
            Block::Note(note) if note.kind == NoteBlockKind::Definition
        )));
    }

    #[test]
    fn split_heading_reference_hides_an_image_bearing_footnote() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <h1>12<br/>Lucy to LuLu to Rose<a id="ref-1"/><a href="#note-1"><sup>*</sup></a></h1>
            <p>Chapter content.</p>
            <div><p id="note-1"><a href="#ref-1"><sup>*</sup></a>Footnote text.<br/>
                <img alt="" src="images/diagram.jpg"/></p></div>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| None).unwrap();

        assert!(section.blocks.iter().any(|block| matches!(
            block,
            Block::Note(note) if note.kind == NoteBlockKind::Definition
        )));
    }

    #[test]
    fn groups_multiblock_implicit_footnote_definitions() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p>Body<a id="ref-1" href="#note-1">1</a>
                and more<a id="ref-2" href="#note-2">2</a>.</p>
            <div class="footnotes">
                <p id="note-1"><a href="#ref-1">1.</a> First definition.</p>
                <p>Continuation of the first definition.</p>
                <p id="note-2"><a href="#ref-2">2.</a> Second definition.</p>
            </div>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let notes = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Note(note) if note.kind == NoteBlockKind::Definition => Some(note),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(notes.len(), 2);
        assert!(note_text(notes[0]).contains("First definition"));
        assert!(note_text(notes[0]).contains("Continuation of the first definition"));
        assert!(!note_text(notes[0]).contains("Second definition"));
        assert!(note_text(notes[1]).contains("Second definition"));
    }

    #[test]
    fn groups_a_local_notes_suffix_without_reclassifying_a_plain_note_callout() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <div class="note"><p>An ordinary editorial callout.</p></div>
            <h2>Notes</h2>
            <p id="note-1"><a href="chapter.xhtml#ref-1">1.</a> First note.</p>
            <p>Continuation of the first note.</p>
            <p id="note-2"><a href="chapter.xhtml#ref-2">2.</a> Second note.</p>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        assert!(matches!(section.blocks.first(), Some(Block::Text(_))));
        let notes = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Note(note) if note.kind == NoteBlockKind::Section => Some(note),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(notes.len(), 3);
        assert_eq!(note_text(notes[0]).trim(), "Notes");
        assert!(note_text(notes[1]).contains("First note"));
        assert!(note_text(notes[1]).contains("Continuation of the first note"));
        assert!(!note_text(notes[1]).contains("Second note"));
        assert!(note_text(notes[2]).contains("Second note"));
    }

    #[test]
    fn splits_a_paragraph_titled_notes_suffix_inside_a_large_body_container() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("text/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <div class="calibre1">
                <h3>第二章</h3>
                <p>正文内容。<a id="q2d1" href="#h2d1">【1】</a></p>
                <p><b>注释：</b></p>
                <p id="note-row"><a id="h2d1" href="#q2d1">【1】</a>第一条注释。</p>
                <p>第一条注释的续段。</p>
            </div>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let body_text = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(text_block_text(block)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        let notes = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Note(note) if note.kind == NoteBlockKind::Section => Some(note),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(body_text.contains("第二章"));
        assert!(body_text.contains("正文内容"));
        assert_eq!(notes.len(), 2);
        assert_eq!(note_text(notes[0]).trim(), "注释：");
        assert!(note_text(notes[1]).contains("第一条注释"));
        assert!(note_text(notes[1]).contains("第一条注释的续段"));
        assert!(!note_text(notes[1]).contains("正文内容"));
    }

    #[test]
    fn whole_section_hint_groups_container_anchored_multiblock_notes() {
        let descriptor = SpineItem {
            id: SpineItemId::new("notes").unwrap(),
            href: PublicationUrl::parse("OPS/notes.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p class="book-title">Notes</p>
            <h2>Chapter One</h2>
            <p id="note-1"><a href="chapter.xhtml#ref-1">1.</a> First note.</p>
            <p class="indent">A second paragraph in the first note.</p>
            <p id="note-2"><a href="chapter.xhtml#ref-2">2.</a> Second note.</p>
        </body></html>"##;

        let section = parse_section_with_hints_and_image_classifier(
            xml,
            &descriptor,
            |_| unreachable!(),
            |_| false,
            SectionParseHints { note_section: true },
        )
        .unwrap();
        assert!(
            section
                .blocks
                .iter()
                .all(|block| matches!(block, Block::Note(_)))
        );
        let notes = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Note(note) => Some(note),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(notes.len(), 4);
        assert!(note_text(notes[2]).contains("First note"));
        assert!(note_text(notes[2]).contains("A second paragraph in the first note"));
        assert!(!note_text(notes[2]).contains("Second note"));
        assert!(note_text(notes[3]).contains("Second note"));
    }

    #[test]
    fn preserves_margins_from_an_image_only_block_container() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><style>
                p.IMG { margin-top: 25px; margin-bottom: 10px; text-align: center; }
            </style></head>
            <body><p class="IMG"><a id="figure"/><img src="images/chart.png"/></p></body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Image(image)] = section.blocks.as_slice() else {
            panic!("expected only the image block");
        };

        assert_close(image.style.margin_before, 25.0);
        assert_close(image.style.margin_after, 10.0);
    }

    #[test]
    fn keeps_em_sized_presentation_images_inline_with_heading_text() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OEBPS/xhtml/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><style>
                img.height_1em { height: 1em; vertical-align: middle; margin-right: .25em; }
            </style></head>
            <body><h2><img alt="" class="height_1em" role="presentation"
                src="../images/chapter-icon.jpg"/>Why Goal Setting Is Broken</h2></body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Text(heading)] = section.blocks.as_slice() else {
            panic!("expected one heading block with no standalone image");
        };
        assert!(matches!(heading.kind, TextBlockKind::Heading(2)));
        let [Inline::Image(image), Inline::Text(text)] = heading.content.as_slice() else {
            panic!("expected the chapter icon before the heading text");
        };
        assert_eq!(image.image.href.path(), "OEBPS/images/chapter-icon.jpg");
        assert!(image.presentation);
        assert!(!image.intrinsic_sizing);
        assert_eq!(image.vertical_align, InlineImageAlignment::Middle);
        assert_close(image.size_scale, text.style.size_scale);
        assert_eq!(text.text, "Why Goal Setting Is Broken");
    }

    #[test]
    fn keeps_formula_rasters_inline_by_text_context_not_class_or_alt() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><style>img.block { vertical-align: middle; }</style></head>
            <body><p class="para">Compare <img class="block" alt="Image"
                src="images/pv.jpg"/> versus <img class="block" alt="Image"
                src="images/nv.jpg"/> today.</p></body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Text(paragraph)] = section.blocks.as_slice() else {
            panic!("expected formula rasters to remain in one text block");
        };
        assert_eq!(paragraph.content.len(), 5);
        let inline_images = paragraph
            .content
            .iter()
            .filter_map(|inline| match inline {
                Inline::Image(image) => Some(image.as_ref()),
                Inline::Text(_) | Inline::Math(_) | Inline::Break => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(inline_images.len(), 2);
        assert!(inline_images.iter().all(|image| image.intrinsic_sizing));
        assert!(
            inline_images
                .iter()
                .all(|image| image.vertical_align == InlineImageAlignment::Middle)
        );
        assert_eq!(inline_images[0].image.alt, "Image");
        assert_eq!(inline_images[0].image.href.path(), "OPS/images/pv.jpg");
        assert_eq!(inline_images[1].image.href.path(), "OPS/images/nv.jpg");
    }

    #[test]
    fn preserves_inherited_language_and_hyphenation_policy() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en-US">
            <head><style>.manual { hyphens: manual; }</style></head>
            <body><p>American <span lang="en-GB">British</span>
                <span class="manual">manual</span> <span lang="fr">français</span></p></body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Text(paragraph)] = section.blocks.as_slice() else {
            panic!("expected one language-bearing paragraph");
        };
        let runs = paragraph
            .content
            .iter()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<Vec<_>>();
        let run = |needle: &str| {
            runs.iter()
                .copied()
                .find(|run| run.text.contains(needle))
                .unwrap()
        };
        assert_eq!(run("American").style.language, TextLanguage::EnglishUs);
        assert_eq!(run("British").style.language, TextLanguage::EnglishGb);
        assert_eq!(run("manual").style.hyphenation, HyphenationMode::Manual);
        assert_eq!(run("français").style.language, TextLanguage::Other);
    }

    #[test]
    fn keeps_short_image_only_equations_as_blocks() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p class="image"><img alt="Equation" height="17" width="255"
                src="images/display-equation.jpg"/></p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Image(image)] = section.blocks.as_slice() else {
            panic!("expected an image-only equation to remain a block");
        };
        assert_eq!(image.href.path(), "OPS/images/display-equation.jpg");
        assert_eq!(image.style.height, Some(ImageLength::Pixels(17.0)));
        assert_eq!(image.style.width, Some(ImageLength::Pixels(255.0)));
    }

    #[test]
    fn parses_figure_image_and_caption_as_one_semantic_block() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <figure id="figure-1" class="image">
                <img alt="Leaf detail" src="images/leaf.jpg"/>
                <figcaption><p class="caption"><strong>Figure 1.</strong> New growth.</p></figcaption>
            </figure>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Figure(figure)] = section.blocks.as_slice() else {
            panic!("expected one semantic figure block");
        };
        assert_eq!(figure.images.len(), 1);
        assert_eq!(figure.images[0].href.path(), "OPS/images/leaf.jpg");
        assert_eq!(figure.images[0].alt, "Leaf detail");
        assert_eq!(figure.captions.len(), 1);
        assert_eq!(figure.captions[0].kind, TextBlockKind::Caption);
        assert_eq!(figure.caption_position, CaptionPosition::After);
        let caption_text = figure.captions[0]
            .content
            .iter()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run.text.as_str()),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<String>();
        assert_eq!(caption_text, "Figure 1. New growth.");
        let Inline::Text(label) = &figure.captions[0].content[0] else {
            panic!("expected a caption label");
        };
        assert!(label.style.bold);
        assert_eq!(figure.source, figure.images[0].source);
        assert_eq!(
            section.anchors[0].source,
            figure.source.clone().unwrap().start
        );
    }

    #[test]
    fn preserves_caption_before_image_and_captionless_figures() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <figure><figcaption>Before</figcaption><img src="images/a.jpg"/></figure>
            <figure><img src="images/b.jpg"/></figure>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Figure(before), Block::Figure(captionless)] = section.blocks.as_slice() else {
            panic!("expected two figure blocks");
        };
        assert_eq!(before.caption_position, CaptionPosition::Before);
        assert_eq!(before.captions.len(), 1);
        assert!(captionless.captions.is_empty());
    }

    #[test]
    fn marks_adjacent_class_and_numbered_paragraphs_as_inferred_captions() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><div>
            <p class="IMG"><a id="leaf"/><img src="images/leaf.jpg"/></p>
            <p class="caption">A leaf without a numbered label.</p>
            <div class="calibre21"><img src="images/chart.jpg"/></div>
            <p class="calibre7"><span>▲图6-5 实战中的图表</span></p>
            <img src="images/direct.jpg"/>
            <p>Figure 2-1. A directly adjacent image caption.</p>
            <p>Ordinary body text.</p>
        </div></body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [
            Block::Image(_),
            Block::Text(class_caption),
            Block::Image(_),
            Block::Text(label_caption),
            Block::Image(_),
            Block::Text(direct_caption),
            Block::Text(body),
        ] = section.blocks.as_slice()
        else {
            panic!("expected adjacent images, inferred captions, and body text");
        };
        assert_eq!(class_caption.kind, TextBlockKind::Caption);
        assert_eq!(label_caption.kind, TextBlockKind::Caption);
        assert_eq!(direct_caption.kind, TextBlockKind::Caption);
        assert_eq!(body.kind, TextBlockKind::Paragraph);
        assert_eq!(
            text_block_text(class_caption),
            "A leaf without a numbered label."
        );
        assert_eq!(text_block_text(label_caption), "▲图6-5 实战中的图表");
    }

    #[test]
    fn leaves_unnumbered_figure_references_and_nonadjacent_labels_as_paragraphs() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p><img src="images/a.jpg"/></p>
            <p>Figure shows the ordinary workflow.</p>
            <p><img src="images/b.jpg"/></p>
            <hr/>
            <p>Figure 2. This label is not adjacent to its image.</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let text = all_text_blocks(&section);
        assert_eq!(text.len(), 2);
        assert!(
            text.iter()
                .all(|block| block.kind == TextBlockKind::Paragraph)
        );
    }

    #[test]
    fn visible_toc_navigation_preserves_each_authored_block() {
        let descriptor = SpineItem {
            id: SpineItemId::new("contents").unwrap(),
            href: PublicationUrl::parse("OPS/toc.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"
            xmlns:epub="http://www.idpf.org/2007/ops">
            <head><style>.toc-chap { margin-left: 2em; }</style></head>
            <body><nav epub:type="toc">
                <h2>Contents</h2>
                <p class="toc-part">I. Caring for Your Collection</p>
                <p class="toc-chap"><strong>1.</strong> The New Plant Collector</p>
                <p class="toc-chap"><strong>2.</strong> Light: Make It Make Sense</p>
            </nav></body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        assert_eq!(section.blocks.len(), 4);
        let block_texts = section
            .blocks
            .iter()
            .map(|block| match block {
                Block::Text(block) => block
                    .content
                    .iter()
                    .filter_map(|inline| match inline {
                        Inline::Text(run) => Some(run.text.as_str()),
                        Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
                    })
                    .collect::<String>(),
                _ => panic!("expected only text blocks"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            block_texts,
            [
                "Contents",
                "I. Caring for Your Collection",
                "1. The New Plant Collector",
                "2. Light: Make It Make Sense",
            ]
        );
        let Block::Text(first_chapter) = &section.blocks[2] else {
            panic!("expected a chapter text block");
        };
        assert_close(first_chapter.style.margin_start, 32.0);
        let Inline::Text(number) = &first_chapter.content[0] else {
            panic!("expected a styled chapter number");
        };
        assert!(number.style.bold);
    }

    #[test]
    fn navigation_metadata_is_suppressed_without_flattening_fallback() {
        let descriptor = SpineItem {
            id: SpineItemId::new("navigation").unwrap(),
            href: PublicationUrl::parse("OPS/nav.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: false,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"
            xmlns:epub="http://www.idpf.org/2007/ops">
            <head><style>.hidden { display: none; }</style></head><body>
                <nav epub:type="landmarks"><ol><li>Guide</li></ol></nav>
                <nav role="doc-pagelist"><ol><li>1</li></ol></nav>
                <nav epub:type="toc" class="hidden"><ol><li>Hidden contents</li></ol></nav>
            </body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        assert!(section.blocks.is_empty());
    }

    #[test]
    fn explicit_enumerator_paragraph_becomes_a_semantic_list_item() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p class="bullet"><span class="enumerator">•</span> A semantic item</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Text(item)] = section.blocks.as_slice() else {
            panic!("expected one text block");
        };
        assert_eq!(
            item.kind,
            TextBlockKind::ListItem {
                ordered: false,
                ordinal: 1,
                depth: 0,
                marker_visible: true,
            }
        );
        let text = item
            .content
            .iter()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run.text.as_str()),
                Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
            })
            .collect::<String>();
        assert_eq!(text, "A semantic item");
    }

    #[test]
    fn explicit_enumerator_paragraphs_recover_css_indent_hierarchy() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><style>
                .bullet { margin-left: 2.5em; }
                .bulletind { margin-left: 3.9em; }
                .bulletind1 { margin-left: 5.5em; }
                .bulletind2 { margin-left: 6.9em; }
            </style></head><body>
            <p class="bullet"><span class="enumerator">•</span> Parent</p>
            <p class="bulletind"><span class="enumerator">•</span> Child</p>
            <p class="bulletind1"><span class="enumerator">•</span> Grandchild</p>
            <p class="bulletind2"><span class="enumerator">•</span> Great-grandchild</p>
            <p class="bulletind"><span class="enumerator">•</span> Second child</p>
            <p class="bullet"><span class="enumerator">•</span> Second parent</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let depths = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(TextBlock {
                    kind: TextBlockKind::ListItem { depth, .. },
                    ..
                }) => Some(*depth),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(depths, [0, 1, 2, 3, 1, 0]);
    }

    #[test]
    fn css_hanging_paragraph_recovers_a_markerless_nested_list_item() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <head><style>
                .parent { margin-left: 1.2em; text-indent: -1.8em; }
                .child { margin-left: 4.7em; text-indent: -1.1em; }
            </style></head><body>
            <p class="parent"><span class="enumerator">»</span> Parent one</p>
            <p class="child">Child one without its own marker</p>
            <p class="child">Child two without its own marker</p>
            <p class="parent"><span class="enumerator">»</span> Parent two</p>
            <p class="child">Child three without its own marker</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let items = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(block),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(items.len(), 5);
        assert_eq!(
            items
                .iter()
                .map(|item| match item.kind {
                    TextBlockKind::ListItem {
                        depth,
                        marker_visible,
                        ..
                    } => (depth, marker_visible),
                    _ => panic!("expected a recovered list item"),
                })
                .collect::<Vec<_>>(),
            [(0, true), (1, false), (1, false), (0, true), (1, false)]
        );
        assert_close(items[1].style.margin_start, 75.2);
        assert_close(items[1].style.indent, 0.0);
    }

    #[test]
    fn nested_html_lists_keep_each_item_and_its_depth() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <ul>
                <li>Parent<ul><li>Child<ol><li>Grandchild</li></ol></li></ul></li>
                <li>Sibling</li>
            </ul>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let items = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(
                    item @ TextBlock {
                        kind: TextBlockKind::ListItem { .. },
                        ..
                    },
                ) => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        let kinds = items.iter().map(|item| item.kind).collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                TextBlockKind::ListItem {
                    ordered: false,
                    ordinal: 1,
                    depth: 0,
                    marker_visible: true,
                },
                TextBlockKind::ListItem {
                    ordered: false,
                    ordinal: 1,
                    depth: 1,
                    marker_visible: true,
                },
                TextBlockKind::ListItem {
                    ordered: true,
                    ordinal: 1,
                    depth: 2,
                    marker_visible: true,
                },
                TextBlockKind::ListItem {
                    ordered: false,
                    ordinal: 2,
                    depth: 0,
                    marker_visible: true,
                },
            ]
        );
        let texts = items
            .iter()
            .map(|item| {
                item.content
                    .iter()
                    .filter_map(|inline| match inline {
                        Inline::Text(run) => Some(run.text.as_str()),
                        Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert_eq!(texts, ["Parent", "Child", "Grandchild", "Sibling"]);
    }

    #[test]
    fn parses_svg_image_href_as_an_image_block() {
        let descriptor = SpineItem {
            id: SpineItemId::new("cover").unwrap(),
            href: PublicationUrl::parse("OPS/titlepage.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
            <body><svg xmlns="http://www.w3.org/2000/svg"
                xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 622 910">
                <image width="622" height="910" xlink:href="images/cover.jpeg"/>
            </svg></body>
        </html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let Some(Block::Image(image)) = section.blocks.first() else {
            panic!("expected an SVG image block");
        };
        assert_eq!(image.href.path(), "OPS/images/cover.jpeg");
        assert_eq!(image.style.width, Some(ImageLength::Pixels(622.0)));
        assert_eq!(image.style.height, Some(ImageLength::Pixels(910.0)));
    }

    #[test]
    fn preserves_block_container_and_empty_element_fragment_anchors() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <a id="before-heading"></a>
            <div id="chapter-start"><h2 id="heading">Heading</h2></div>
            <p>Text <span id="inside-paragraph">target</span></p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let anchors = section
            .anchors
            .iter()
            .map(|anchor| (anchor.fragment.as_str(), anchor.source.node.as_str()))
            .collect::<HashMap<_, _>>();
        assert_eq!(anchors.get("before-heading"), Some(&"n0"));
        assert_eq!(anchors.get("chapter-start"), Some(&"n0"));
        assert_eq!(anchors.get("heading"), Some(&"n0"));
        assert_eq!(anchors.get("inside-paragraph"), Some(&"n1"));
    }

    #[test]
    fn parses_nested_generic_block_containers_without_flattening_or_duplication() {
        let descriptor = SpineItem {
            id: SpineItemId::new("contents").unwrap(),
            href: PublicationUrl::parse("Text/contents.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <div id="title">目录</div>
            <div id="chapter"><a id="chapter-link">第一章</a>
                <div id="item"><a id="item-link">◎故事的力量</a></div>
            </div>
            <div id="mixed">开头<p id="paragraph">正文</p>结尾</div>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let texts = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(
                    block
                        .content
                        .iter()
                        .filter_map(|inline| match inline {
                            Inline::Text(run) => Some(run.text.as_str()),
                            Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
                        })
                        .collect::<String>(),
                ),
                Block::Table(_)
                | Block::Quote(_)
                | Block::Image(_)
                | Block::Figure(_)
                | Block::Note(_)
                | Block::Separator(_)
                | Block::LineBreak
                | Block::PageBreak => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            ["目录", "第一章", "◎故事的力量", "开头", "正文", "结尾"]
        );

        let anchors = section
            .anchors
            .iter()
            .map(|anchor| (anchor.fragment.as_str(), anchor.source.node.as_str()))
            .collect::<HashMap<_, _>>();
        assert_eq!(anchors.get("title"), Some(&"n0"));
        assert_eq!(anchors.get("chapter"), Some(&"n1"));
        assert_eq!(anchors.get("chapter-link"), Some(&"n1"));
        assert_eq!(anchors.get("item"), Some(&"n2"));
        assert_eq!(anchors.get("item-link"), Some(&"n2"));
        assert_eq!(anchors.get("mixed"), Some(&"n3"));
        assert_eq!(anchors.get("paragraph"), Some(&"n4"));
    }

    #[test]
    fn preserves_images_wrapped_by_inline_elements_in_block_containers() {
        let descriptor = SpineItem {
            id: SpineItemId::new("plates").unwrap(),
            href: PublicationUrl::parse("OPS/text/plates.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><div>
            <h3>Plates</h3>
            <div><a id="plate-one"><img src="../images/one.jpeg"/></a></div>
            <div><span><a id="plate-two"><img src="../images/two.jpeg"/></a></span></div>
        </div></body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let images = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Image(image) => Some(image.href.path()),
                Block::Text(_)
                | Block::Quote(_)
                | Block::Table(_)
                | Block::Figure(_)
                | Block::Note(_)
                | Block::Separator(_)
                | Block::LineBreak
                | Block::PageBreak => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(images, ["OPS/images/one.jpeg", "OPS/images/two.jpeg"]);
        assert!(
            section
                .anchors
                .iter()
                .any(|anchor| anchor.fragment == "plate-one")
        );
        assert!(
            section
                .anchors
                .iter()
                .any(|anchor| anchor.fragment == "plate-two")
        );
    }

    #[test]
    fn keeps_definition_list_entries_as_separate_blocks() {
        let descriptor = SpineItem {
            id: SpineItemId::new("contents").unwrap(),
            href: PublicationUrl::parse("OPS/text/contents.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            dl { margin: 1em 0 1em 10%; }
            dt { padding: 0 0 0 .5em; }
            dd { margin: 0 0 .4em 2.75em; }
        </style></head><body><div>
            <h3>目录</h3>
            <dl>
                <dt><a href="chapter.xhtml#one">第一章</a></dt>
                <dd><a href="chapter.xhtml#section">第一节</a></dd>
                <dt><a href="chapter.xhtml#two">第二章</a></dt>
            </dl>
        </div></body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let texts = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(
                    block
                        .content
                        .iter()
                        .filter_map(|inline| match inline {
                            Inline::Text(run) => Some(run.text.as_str()),
                            Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
                        })
                        .collect::<String>(),
                ),
                Block::Quote(_)
                | Block::Table(_)
                | Block::Image(_)
                | Block::Figure(_)
                | Block::Note(_)
                | Block::Separator(_)
                | Block::LineBreak
                | Block::PageBreak => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(texts, ["目录", "第一章", "第一节", "第二章"]);
        let kinds = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(block.kind),
                Block::Quote(_)
                | Block::Table(_)
                | Block::Image(_)
                | Block::Figure(_)
                | Block::Note(_)
                | Block::Separator(_)
                | Block::LineBreak
                | Block::PageBreak => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                TextBlockKind::Heading(3),
                TextBlockKind::DefinitionTerm { depth: 0 },
                TextBlockKind::DefinitionDescription { depth: 0 },
                TextBlockKind::DefinitionTerm { depth: 0 },
            ]
        );
        let styles = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(block.style),
                Block::Quote(_)
                | Block::Table(_)
                | Block::Image(_)
                | Block::Figure(_)
                | Block::Note(_)
                | Block::Separator(_)
                | Block::LineBreak
                | Block::PageBreak => None,
            })
            .collect::<Vec<_>>();
        assert_close(styles[1].margin_start, 8.0);
        assert_close(styles[1].margin_start_fraction, 0.1);
        assert_close(styles[2].margin_start, 44.0);
        assert_close(styles[2].margin_start_fraction, 0.1);
    }

    #[test]
    fn nested_definition_lists_preserve_roles_depth_and_reading_order() {
        let descriptor = SpineItem {
            id: SpineItemId::new("index").unwrap(),
            href: PublicationUrl::parse("OPS/index.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <dl>
                <dt>Markup language</dt>
                <dd>A notation for documents
                    <dl>
                        <dt>Abstract markup</dt>
                        <dd>Expresses structure</dd>
                    </dl>
                </dd>
                <dt>Media domain</dt>
                <dd>Controls presentation</dd>
            </dl>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let entries = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some((
                    block.kind,
                    block
                        .content
                        .iter()
                        .filter_map(|inline| match inline {
                            Inline::Text(run) => Some(run.text.as_str()),
                            Inline::Math(_) | Inline::Image(_) | Inline::Break => None,
                        })
                        .collect::<String>(),
                )),
                Block::Quote(_)
                | Block::Table(_)
                | Block::Image(_)
                | Block::Figure(_)
                | Block::Note(_)
                | Block::Separator(_)
                | Block::LineBreak
                | Block::PageBreak => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            entries,
            [
                (
                    TextBlockKind::DefinitionTerm { depth: 0 },
                    "Markup language".to_owned(),
                ),
                (
                    TextBlockKind::DefinitionDescription { depth: 0 },
                    "A notation for documents".to_owned(),
                ),
                (
                    TextBlockKind::DefinitionTerm { depth: 1 },
                    "Abstract markup".to_owned(),
                ),
                (
                    TextBlockKind::DefinitionDescription { depth: 1 },
                    "Expresses structure".to_owned(),
                ),
                (
                    TextBlockKind::DefinitionTerm { depth: 0 },
                    "Media domain".to_owned(),
                ),
                (
                    TextBlockKind::DefinitionDescription { depth: 0 },
                    "Controls presentation".to_owned(),
                ),
            ]
        );
    }

    #[test]
    fn escaped_list_markup_in_preformatted_code_is_not_parsed_as_a_list() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <pre>&lt;ol&gt;&lt;li&gt;Dogs&lt;/li&gt;&lt;/ol&gt;</pre>
            <ol><li>Dogs<ol><li>Spot</li></ol></li></ol>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let kinds = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(block) => Some(block.kind),
                Block::Quote(_)
                | Block::Table(_)
                | Block::Image(_)
                | Block::Figure(_)
                | Block::Note(_)
                | Block::Separator(_)
                | Block::LineBreak
                | Block::PageBreak => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                TextBlockKind::Preformatted,
                TextBlockKind::ListItem {
                    ordered: true,
                    ordinal: 1,
                    depth: 0,
                    marker_visible: true,
                },
                TextBlockKind::ListItem {
                    ordered: true,
                    ordinal: 1,
                    depth: 1,
                    marker_visible: true,
                },
            ]
        );
    }

    #[test]
    fn recognizes_structural_quote_without_using_class_or_id_names() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .arbitrary-wrapper { margin: 1em 0; padding: 5px; background-color: #e7e7e8; }
            .arbitrary-body { margin: 1em 2em; font-style: italic; }
            .arbitrary-tail { margin: 0 2em 2em 0; text-align: right; }
        </style></head><body>
            <div class="arbitrary-wrapper">
                <p class="arbitrary-body">Quoted prose with a <a href="#note"><sup>50</sup></a> note.</p>
                <p class="arbitrary-tail">Diane Mizrachi and Alicia Salaz</p>
            </div>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Quote(quote)] = section.blocks.as_slice() else {
            panic!("expected one semantic quote block");
        };
        assert_eq!(quote.body.len(), 1);
        assert_eq!(quote.body[0].kind, TextBlockKind::Blockquote);
        assert_eq!(
            quote.attribution.as_ref().map(|block| block.kind),
            Some(TextBlockKind::QuoteAttribution)
        );
        assert!(quote.source.is_some());
    }

    #[test]
    fn groups_sibling_verse_lines_with_a_trailing_attribution() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .verse-line {
                font-style: italic;
                line-height: 130%;
                text-align: justify;
                text-indent: 2em;
                margin: 4pt 2em;
            }
            .verse-source {
                font-size: 0.83333em;
                line-height: 130%;
                text-align: right;
                text-indent: 2em;
                margin: 0.8em 0 5pt;
            }
        </style></head><body><div>
            <p>Ordinary prose before the verse.</p>
            <br/>
            <p class="verse-line">乃生男子，</p>
            <p class="verse-line">载寝之床，</p>
            <p class="verse-line">载衣之裳。</p>
            <br/>
            <p class="verse-line">乃生女子，</p>
            <p class="verse-line">载寝之地，</p>
            <p class="verse-line">载衣之裼。</p>
            <p class="verse-source">（《诗经》第189首）</p>
            <br/>
            <p>Ordinary prose after the verse.</p>
        </div></body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let quote = section
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Quote(quote) => Some(quote),
                _ => None,
            })
            .expect("the sibling verse lines should form one quote");
        assert_eq!(quote.body.len(), 6);
        assert!(
            quote
                .body
                .iter()
                .all(|block| block.kind == TextBlockKind::Blockquote)
        );
        assert!(quote.body[2].style.hard_break_after);
        assert!(!quote.body[1].style.hard_break_after);
        assert!(!quote.body[3].style.hard_break_after);
        assert_eq!(
            section
                .blocks
                .iter()
                .filter(|block| matches!(block, Block::LineBreak))
                .count(),
            2
        );
        assert!(
            !section
                .blocks
                .iter()
                .any(|block| matches!(block, Block::PageBreak))
        );
        let attribution = quote
            .attribution
            .as_ref()
            .expect("the right-aligned source should remain attached");
        assert_eq!(attribution.kind, TextBlockKind::QuoteAttribution);
        assert!(attribution.content.iter().any(|inline| matches!(
            inline,
            Inline::Text(run) if run.text.contains("《诗经》第189首")
        )));
        assert_eq!(
            section
                .blocks
                .iter()
                .filter(|block| matches!(block, Block::Quote(_)))
                .count(),
            1
        );
    }

    #[test]
    fn groups_zero_margin_epigraph_body_with_its_marked_source() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .body-line {
                font-family: serif;
                font-size: 90%;
                text-align: justify;
                text-indent: 1.5em;
                margin: 0 1.5em;
            }
            .source-line {
                font-family: serif;
                font-size: 95%;
                text-align: right;
                text-indent: 0;
                margin: 0.5em 1.5em 2em;
            }
        </style></head><body>
            <p class="body-line">We read to dream and aspire, but also to acquire.</p>
            <p class="source-line"><i>—Carol Smith, publisher and chief revenue officer at</i> Harper’s Bazaar</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Quote(quote)] = section.blocks.as_slice() else {
            panic!("expected the epigraph and source to form one quote");
        };
        assert_eq!(quote.body.len(), 1);
        assert_eq!(quote.body[0].kind, TextBlockKind::Blockquote);
        let attribution = quote
            .attribution
            .as_ref()
            .expect("the marked source should remain attached");
        assert_eq!(attribution.kind, TextBlockKind::QuoteAttribution);
        assert!(attribution.content.iter().any(|inline| matches!(
            inline,
            Inline::Text(run) if run.text.starts_with("—Carol Smith") && run.style.italic
        )));
        assert!(quote.source.is_some());
    }

    #[test]
    fn ordinary_inset_prose_and_right_aligned_text_are_not_grouped_without_source_semantics() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .inset { text-align: justify; margin: 0 1.5em; }
            .tail { text-align: right; margin: 0.5em 1.5em 2em; }
        </style></head><body>
            <p class="inset">Ordinary inset prose.</p>
            <p class="tail">Continue reading</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        assert_eq!(section.blocks.len(), 2);
        assert!(section.blocks.iter().all(|block| matches!(
            block,
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                ..
            })
        )));
    }

    #[test]
    fn recognizes_attributed_unattributed_and_mixed_style_quotes_by_structure() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .verse-body { font-family: KaiTi, serif; margin: 4pt 2em; text-indent: 2em; }
            .center-line { font-family: KaiTi, serif; margin: 4pt 2em; text-align: center; font-weight: bold; }
            .credit { margin: 0.8em 0 5pt; text-align: right; font-size: 0.83em; }
            .isolated { font-family: KaiTi, serif; margin: 4pt 2em; text-indent: 4em; }
        </style></head><body><div>
            <p class="verse-body">A single quoted paragraph.</p>
            <p class="credit">The source</p>
            <p>Ordinary prose between quotations.</p>
            <p class="verse-body">First verse line.</p>
            <p class="center-line">Centered refrain.</p>
            <br/>
            <p class="verse-body">Last verse line.</p>
            <p>Ordinary prose after the verse.</p>
            <p class="isolated">An isolated quotation without a source.</p>
            <p>Final ordinary prose.</p>
        </div></body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let quotes = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Quote(quote) => Some(quote),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(quotes.len(), 3);
        assert_eq!(quotes[0].body.len(), 1);
        assert!(quotes[0].attribution.is_some());
        assert_eq!(quotes[1].body.len(), 3);
        assert!(quotes[1].attribution.is_none());
        assert!(quotes[1].body[1].style.hard_break_after);
        assert_eq!(quotes[2].body.len(), 1);
        assert!(quotes[2].attribution.is_none());
    }

    #[test]
    fn repeated_inset_prose_without_quote_typography_remains_prose() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .indented { margin: 1em 2em; }
        </style></head><body>
            <p class="indented">First ordinary inset paragraph.</p>
            <p class="indented">Second ordinary inset paragraph.</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        assert_eq!(section.blocks.len(), 2);
        assert!(section.blocks.iter().all(|block| matches!(
            block,
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                ..
            })
        )));
    }

    #[test]
    fn long_unattributed_quote_is_not_split_by_an_internal_block_limit() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let paragraphs = (1..=80)
            .map(|index| format!(r#"<p class="source-text">Quoted block {index}</p>"#))
            .collect::<String>();
        let xml = format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
                .source-text {{ font-family: KaiTi, serif; margin: 4pt 2em; text-indent: 2em; }}
            </style></head><body><div>{paragraphs}<p>Ordinary prose.</p></div></body></html>"#
        );

        let section = parse_section(&xml, &descriptor, |_| unreachable!()).unwrap();
        let quotes = section
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Quote(quote) => Some(quote),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].body.len(), 80);
    }

    #[test]
    fn recognizes_a_standalone_paragraph_with_the_quote_semantic_word() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .prosequote { margin: 1em 2em; text-indent: 0; }
        </style></head><body>
            <p class="prosequote">A standalone quotation without an attribution.</p>
            <p>Ordinary prose after the quotation.</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Quote(quote), Block::Text(paragraph)] = section.blocks.as_slice() else {
            panic!("expected one quote followed by ordinary prose");
        };
        assert_eq!(quote.body.len(), 1);
        assert_eq!(quote.body[0].kind, TextBlockKind::Blockquote);
        assert!(quote.attribution.is_none());
        assert_eq!(paragraph.kind, TextBlockKind::Paragraph);
    }

    #[test]
    fn structural_quote_keeps_source_when_body_class_contains_quote() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .box { margin: 1em 0; padding: 5px; background: #eee; }
            .prosequote1 { margin: 1em 2em; text-indent: 0; font-style: italic; }
            .source { margin: 0 2em 1em 0; text-align: right; }
        </style></head><body>
            <div class="box">
                <p class="prosequote1">Quoted prose.</p>
                <p class="source">Quotation source</p>
            </div>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Quote(quote)] = section.blocks.as_slice() else {
            panic!("expected one quote with an attribution");
        };
        assert_eq!(quote.body.len(), 1);
        assert_eq!(quote.body[0].kind, TextBlockKind::Blockquote);
        let attribution = quote
            .attribution
            .as_ref()
            .expect("quote source should be preserved");
        assert_eq!(attribution.kind, TextBlockKind::QuoteAttribution);
        assert!(attribution.content.iter().any(|inline| matches!(
            inline,
            Inline::Text(run) if run.text.contains("Quotation source")
        )));
    }

    #[test]
    fn structural_quote_accepts_inline_markup_inside_the_attribution() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r##"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            div.box { margin: 1em 0; padding: 5px; background-color: #e7e7e8; }
            .prosequote1 { margin: 1em 2em; text-indent: 0; font-style: italic; }
            .source { margin: 0 2em 2em 0; text-align: right; }
        </style></head><body>
            <div class="box">
                <p class="prosequote1">“Will letter writing become a proceeding of the past?”</p>
                <p class="source"><em>Scientific American</em> 1877<a href="#note"><sup>8</sup></a></p>
            </div>
        </body></html>"##;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Quote(quote)] = section.blocks.as_slice() else {
            panic!("expected inline attribution markup to remain in the structural quote");
        };
        let attribution = quote
            .attribution
            .as_ref()
            .expect("quote attribution should be recognized");
        assert_eq!(attribution.kind, TextBlockKind::QuoteAttribution);
        assert!(attribution.content.iter().any(|inline| matches!(
            inline,
            Inline::Text(run) if run.text == "Scientific American" && run.style.italic
        )));
        assert!(attribution.content.iter().any(|inline| matches!(
            inline,
            Inline::Text(run) if run.text == "8" && run.style.baseline == TextBaseline::Superscript
        )));
    }

    #[test]
    fn quote_semantic_word_without_quote_layout_remains_a_paragraph() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .quote-status { margin: 1em 0; text-indent: 0; }
            .quote-aside { margin: 1em 0 1em 2em; text-indent: 0; }
        </style></head><body>
            <p class="quote-status">A status message about quotations.</p>
            <p class="quote-aside">An asymmetrically indented aside.</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        assert_eq!(section.blocks.len(), 2);
        assert!(section.blocks.iter().all(|block| matches!(
            block,
            Block::Text(TextBlock {
                kind: TextBlockKind::Paragraph,
                ..
            })
        )));
    }

    #[test]
    fn visually_bounded_text_card_without_quote_role_difference_is_not_a_quote() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .card { padding: 5px; background: #eee; }
            .tail { text-align: right; }
        </style></head><body>
            <div class="card"><p>Ordinary card content.</p><p class="tail">Continue reading</p></div>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        assert_eq!(section.blocks.len(), 2);
        assert!(
            section
                .blocks
                .iter()
                .all(|block| matches!(block, Block::Text(_)))
        );
    }

    #[test]
    fn semantic_blockquote_keeps_direct_cite_as_attribution() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <blockquote><p>Quoted prose.</p><cite>The source</cite></blockquote>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Quote(quote)] = section.blocks.as_slice() else {
            panic!("expected one semantic quote");
        };
        assert_eq!(quote.body.len(), 1);
        assert_close(quote.body[0].style.margin_start, 24.0);
        assert_close(quote.body[0].style.indent, 0.0);
        assert_eq!(
            quote.attribution.as_ref().map(|block| block.kind),
            Some(TextBlockKind::QuoteAttribution)
        );
        let attribution = quote.attribution.as_ref().unwrap();
        let citation = attribution.content.iter().find_map(|inline| match inline {
            Inline::Text(run) if run.text.contains("The source") => Some(run),
            _ => None,
        });
        assert!(citation.is_some_and(|run| run.style.citation && run.style.italic));
    }

    #[test]
    fn inline_cite_keeps_citation_semantics_separate_from_ordinary_emphasis() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p>Fred Woodward designed <cite>Rolling Stone</cite>, <em>other work</em>, and <i>technical terms</i>.</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Text(block)] = section.blocks.as_slice() else {
            panic!("expected one text block");
        };
        let citation = block.content.iter().find_map(|inline| match inline {
            Inline::Text(run) if run.text == "Rolling Stone" => Some(run),
            _ => None,
        });
        let emphasis = block.content.iter().find_map(|inline| match inline {
            Inline::Text(run) if run.text == "other work" => Some(run),
            _ => None,
        });
        let alternate_voice = block.content.iter().find_map(|inline| match inline {
            Inline::Text(run) if run.text == "technical terms" => Some(run),
            _ => None,
        });

        assert!(citation.is_some_and(|run| run.style.citation && run.style.italic));
        assert!(emphasis.is_some_and(|run| {
            run.style.emphasis
                && !run.style.alternate_voice
                && !run.style.citation
                && run.style.italic
        }));
        assert!(alternate_voice.is_some_and(|run| {
            run.style.alternate_voice
                && !run.style.emphasis
                && !run.style.citation
                && run.style.italic
        }));
    }

    #[test]
    fn nested_blockquote_keeps_authored_margin_without_adding_first_line_indent() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
            .outer { margin-left: 2em; }
            .body { margin-left: 2em; text-indent: 2em; }
        </style></head><body>
            <blockquote class="outer"><blockquote class="body">Quoted prose.</blockquote></blockquote>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Quote(quote)] = section.blocks.as_slice() else {
            panic!("expected one nested semantic quote");
        };
        assert_eq!(quote.body.len(), 1);
        assert_close(quote.body[0].style.margin_start, 64.0);
        assert_close(quote.body[0].style.indent, 32.0);
    }

    #[test]
    fn body_unicode_spacing_paragraphs_become_semantic_separators() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p>Before.</p>
            <p class="ideographic">&#x3000;</p>
            <p class="nbsp">&#160;</p>
            <p class="break"><br/></p>
            <p>After.</p>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        assert_eq!(section.blocks.len(), 5);
        assert!(matches!(section.blocks[0], Block::Text(_)));
        for block in &section.blocks[1..4] {
            let Block::Separator(separator) = block else {
                panic!("expected semantic spacing separator");
            };
            assert_eq!(separator.kind, rebook_publication::SeparatorKind::Spacing);
            assert!(!separator.in_quote);
        }
        assert!(matches!(section.blocks[4], Block::Text(_)));
    }

    #[test]
    fn body_separator_recognition_does_not_rewrite_quote_contents() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <blockquote>
                <p>First quoted paragraph.</p>
                <p>&#x3000;</p>
                <hr/>
                <p>Second quoted paragraph.</p>
            </blockquote>
        </body></html>"#;

        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let [Block::Quote(quote)] = section.blocks.as_slice() else {
            panic!("expected one quote without promoted separators");
        };
        assert_eq!(quote.body.len(), 2);
    }

    #[test]
    fn symbol_separators_preserve_their_source_text_and_require_safe_context() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html><body>
          <p id="stars" style="text-align:center">* <span>*</span>&#160;*</p>
          <p><span style="display:block;text-align:center">※※※</span></p>
          <div>• &#x200b;• •</div>
          <p>A sufficiently long ordinary paragraph before the section break.</p>
          <p style="text-align:center;margin:1em 0">————</p>
          <p>A sufficiently long ordinary paragraph following the section break.</p>
          <p style="text-align:center;margin:2em 0">▲</p>
          <p>Another sufficiently long paragraph describing the next scene.</p>
          <p>*</p><p>……</p><p>“……”</p><p>!!!</p><p>—</p>
          <p style="text-align:center">.<br/>.<br/>.</p>
          <blockquote><p>* * *</p></blockquote>
          <ul><li><p>* * *</p></li></ul>
          <p><a href='#note'>***</a></p>
        </body></html>"#;
        let section = parse_section(xml, &descriptor, |_| unreachable!()).unwrap();
        let separators = section
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Separator(s) if s.kind == rebook_publication::SeparatorKind::Symbols => {
                    Some(s)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(separators.len(), 5);
        let original = separators[0].text.as_ref().unwrap();
        assert_eq!(original.style.align, TextAlignment::Center);
        assert!(original.source.is_some());
        assert!(
            section
                .anchors
                .iter()
                .any(|a| a.fragment == "stars"
                    && a.source == original.source.as_ref().unwrap().start)
        );
        assert!(section.blocks.iter().any(
            |b| matches!(b,Block::Quote(q) if q.body.iter().any(|p| text_block_text(p)=="* * *"))
        ));
        for expected in ["*", "……", "“……”", "!!!", "—"] {
            assert!(
                section
                    .blocks
                    .iter()
                    .any(|b| matches!(b,Block::Text(t) if text_block_text(t)==expected)),
                "lost {expected}"
            );
        }
    }

    #[test]
    fn image_classifier_only_promotes_uncaptioned_body_images() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <p><img src="rule.png" alt="image"/></p>
            <figure><img src="rule.png" alt="image"/><figcaption>A real figure.</figcaption></figure>
        </body></html>"#;

        let section = parse_section_with_image_classifier(
            xml,
            &descriptor,
            |_| unreachable!(),
            |href| href.path().ends_with("rule.png"),
        )
        .unwrap();
        let [Block::Separator(separator), Block::Figure(_)] = section.blocks.as_slice() else {
            panic!("expected one ornament separator followed by one figure");
        };
        assert_eq!(separator.kind, rebook_publication::SeparatorKind::Ornament);
        assert_eq!(
            separator.image.as_ref().map(|image| image.href.path()),
            Some("OPS/rule.png")
        );
    }

    #[test]
    fn rejects_documents_larger_than_the_parser_budget() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let xml = "x".repeat(64 * 1024 * 1024 + 1);

        let error = parse_section(&xml, &descriptor, |_| None).unwrap_err();

        assert!(matches!(
            error,
            HtmlError::InvalidDocument { message, .. }
                if message.contains("byte limit")
        ));
    }

    #[test]
    fn rejects_documents_with_excessive_dom_depth() {
        let descriptor = SpineItem {
            id: SpineItemId::new("chapter").unwrap(),
            href: PublicationUrl::parse("OPS/chapter.xhtml").unwrap(),
            media_type: "application/xhtml+xml".into(),
            linear: true,
            properties: Vec::new(),
        };
        let mut xml = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>".to_owned();
        for _ in 0..300 {
            xml.push_str("<div>");
        }
        xml.push_str("deep");
        for _ in 0..300 {
            xml.push_str("</div>");
        }
        xml.push_str("</body></html>");

        let error = parse_section(&xml, &descriptor, |_| None).unwrap_err();

        assert!(matches!(
            error,
            HtmlError::InvalidDocument { message, .. }
                if message.contains("depth limit")
        ));
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }
