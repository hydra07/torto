#[derive(Clone, Copy, PartialEq, Eq)]
enum TextContext {
    Flow,
    Table,
}

#[derive(Clone, Copy)]
struct ResolvedTableMetrics {
    font_scale: f32,
    line_height: f32,
    cell_padding: f32,
    block_gap: f32,
}

fn resolve_table_metrics(reader_style: &ReaderStyle) -> ResolvedTableMetrics {
    if reader_style.typesetting.mode == TypesettingMode::Unified {
        ResolvedTableMetrics {
            font_scale: reader_style.typesetting.table_font_scale,
            line_height: reader_style.typesetting.table_line_height,
            cell_padding: reader_style.typography.font_size
                * reader_style.typesetting.table_cell_padding_em,
            block_gap: reader_style.typography.font_size * 0.7,
        }
    } else {
        ResolvedTableMetrics {
            font_scale: 1.0,
            line_height: 1.3,
            cell_padding: 6.0,
            block_gap: TABLE_BLOCK_GAP,
        }
    }
}

fn table_cell_text_block(cell: &TableCell) -> TextBlock {
    let mut block = cell.text.clone();
    block.style.align = cell.authored_alignment.unwrap_or(TextAlignment::Center);
    if cell.header {
        for inline in &mut block.content {
            if let Inline::Text(run) = inline {
                run.style.bold = true;
            }
        }
    }
    block
}

fn centered_table_offset(unified: bool, available_width: f32, column_widths: &[f32]) -> f32 {
    if unified {
        ((available_width - column_widths.iter().sum::<f32>()) / 2.0).max(0.0)
    } else {
        0.0
    }
}

fn unified_body_line_height(system: WritingSystem, profile: &ReaderTypesetting) -> f32 {
    let base = match system {
        WritingSystem::Cjk => 1.7,
        WritingSystem::Latin => 1.4,
        WritingSystem::Other | WritingSystem::Unknown => 1.5,
    };
    (profile.body_line_height / 1.5) * base
}

fn resolve_text_block<'a>(
    block: &'a TextBlock,
    reader_style: &ReaderStyle,
    context: TextContext,
) -> Cow<'a, TextBlock> {
    if reader_style.typesetting.mode != TypesettingMode::Unified {
        if block.style.hard_break_after {
            let mut resolved = block.clone();
            resolved.style.margin_after +=
                reader_style.typography.font_size * resolved.style.line_height.max(1.0);
            return Cow::Owned(resolved);
        }
        return Cow::Borrowed(block);
    }

    let mut resolved = block.clone();
    let typography = &reader_style.typography;
    let profile = &reader_style.typesetting;
    let base_size = typography.font_size;
    let display_system = block
        .content
        .iter()
        .find_map(|inline| match inline {
            Inline::Text(run) => run.style.display_writing_system,
            _ => None,
        })
        .unwrap_or(reader_style.writing_system);
    let body_line_height = unified_body_line_height(display_system, profile);
    let (scale, line_height, margin_after) = match context {
        TextContext::Table => (profile.table_font_scale, profile.table_line_height, 0.0),
        TextContext::Flow => match block.kind {
            TextBlockKind::Heading(level) => (
                unified_heading_scale(profile.heading_scale, level),
                1.3,
                base_size * profile.heading_body_gap_em,
            ),
            TextBlockKind::HeadingOrdinal(level) => (
                unified_heading_scale(profile.heading_scale, level) * 0.72,
                1.15,
                base_size * 0.25,
            ),
            TextBlockKind::Caption => (profile.caption_font_scale, 1.4, 0.0),
            TextBlockKind::Preformatted => (0.9, 1.45, base_size * profile.paragraph_gap_em),
            TextBlockKind::Blockquote => {
                (0.95, body_line_height, base_size * profile.paragraph_gap_em)
            }
            TextBlockKind::QuoteAttribution => (0.88, 1.4, 0.0),
            TextBlockKind::Paragraph
            | TextBlockKind::FootnoteDefinition
            | TextBlockKind::ListItem { .. }
            | TextBlockKind::DefinitionDescription { .. } => {
                (1.0, body_line_height, base_size * profile.paragraph_gap_em)
            }
            TextBlockKind::DefinitionTerm { .. } => (
                1.0,
                body_line_height,
                base_size * profile.paragraph_gap_em.min(0.25),
            ),
        },
    };
    let margin_after = margin_after
        + if block.style.hard_break_after {
            base_size * line_height
        } else {
            0.0
        };

    if context == TextContext::Flow {
        let prose = matches!(
            block.kind,
            TextBlockKind::Paragraph | TextBlockKind::Blockquote
        );
        resolved.style.align = if prose
            && let Some(authored_alignment) = block.style.authored_alignment
            && authored_alignment != TextAlignment::Start
        {
            authored_alignment
        } else if prose
            || matches!(block.kind, TextBlockKind::ListItem { .. })
                && text_block_supports_space_justification(block)
        {
            TextAlignment::Justify
        } else {
            TextAlignment::Start
        };
    }
    resolved.style.margin_before = 0.0;
    resolved.style.margin_after = margin_after;
    resolved.style.indent = if context == TextContext::Flow {
        match block.kind {
            TextBlockKind::Paragraph => {
                base_size * paragraph_indent_em(profile, reader_style.writing_system)
            }
            TextBlockKind::Blockquote if block.style.indent > f32::EPSILON => base_size * 2.0,
            TextBlockKind::Blockquote => 0.0,
            _ => 0.0,
        }
    } else {
        0.0
    };
    resolved.style.line_height = line_height;
    match context {
        TextContext::Table => {
            resolved.style.margin_start = 0.0;
            resolved.style.margin_start_fraction = 0.0;
        }
        TextContext::Flow => match block.kind {
            TextBlockKind::Caption => {
                resolved.style.align = TextAlignment::Center;
                resolved.style.margin_start = 0.0;
                resolved.style.margin_start_fraction = 0.0;
            }
            TextBlockKind::ListItem { depth, .. } => {
                resolved.style.margin_start =
                    base_size * profile.list_indent_em * (f32::from(depth) + 1.0);
                resolved.style.margin_start_fraction = 0.0;
            }
            TextBlockKind::DefinitionTerm { depth } => {
                resolved.style.margin_start = base_size * profile.list_indent_em * f32::from(depth);
                resolved.style.margin_start_fraction = 0.0;
            }
            TextBlockKind::DefinitionDescription { depth } => {
                resolved.style.margin_start =
                    base_size * profile.list_indent_em * (f32::from(depth) + 1.0);
                resolved.style.margin_start_fraction = 0.0;
            }
            TextBlockKind::Blockquote => {
                resolved.style.margin_start = 0.0;
                resolved.style.margin_start_fraction = 0.0;
            }
            TextBlockKind::QuoteAttribution => {
                resolved.style.align = TextAlignment::End;
                resolved.style.margin_start = 0.0;
                resolved.style.margin_start_fraction = 0.0;
            }
            _ => {
                resolved.style.margin_start = 0.0;
                resolved.style.margin_start_fraction = 0.0;
            }
        },
    }

    for inline in &mut resolved.content {
        match inline {
            Inline::Text(run) => {
                run.style.color = Rgba::BLACK;
                // Unified typesetting starts from a neutral decoration layer.
                // Semantic/application decorations can opt back in explicitly.
                run.style.underline = false;
                run.style.size_scale = match run.style.baseline {
                    TextBaseline::Normal => scale,
                    TextBaseline::Superscript => scale * 0.70,
                    TextBaseline::Subscript => scale * 0.75,
                };
                if block.kind.is_heading()
                    || matches!(block.kind, TextBlockKind::DefinitionTerm { .. })
                {
                    run.style.bold = true;
                }
                if block.kind.is_heading() {
                    // Unified/focus typesetting owns heading presentation.
                    // Preserve inline emphasis in prose, but do not carry an
                    // authored block-level italic heading into focus mode.
                    run.style.italic = false;
                }
                if matches!(
                    block.kind,
                    TextBlockKind::Blockquote | TextBlockKind::QuoteAttribution
                ) {
                    run.style.bold = false;
                    run.style.italic = false;
                }
                if block.kind == TextBlockKind::Caption {
                    // Unified captions use one neutral presentation regardless of
                    // publisher CSS or semantic tags that would recreate bold or
                    // italic styling in the later script-aware pass.
                    run.style.bold = false;
                    run.style.italic = false;
                    run.style.emphasis = false;
                    run.style.alternate_voice = false;
                    run.style.citation = false;
                }
            }
            Inline::Math(run) => run.size_scale = scale,
            Inline::Image(run) => run.size_scale = scale,
            Inline::Break => {}
        }
    }
    resolve_semantic_inline_presentation(&mut resolved.content, reader_style.writing_system);
    Cow::Owned(resolved)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SemanticScriptClass {
    Cjk,
    ItalicFriendly,
    Neutral,
}

fn resolve_semantic_inline_presentation(
    content: &mut Vec<Inline>,
    fallback_writing_system: WritingSystem,
) {
    let original = std::mem::take(content);
    for inline in original {
        let Inline::Text(run) = inline else {
            content.push(inline);
            continue;
        };
        if !run.style.emphasis && !run.style.alternate_voice && !run.style.citation {
            content.push(Inline::Text(run));
            continue;
        }
        let spans = semantic_script_spans(&run.text, fallback_writing_system);
        if spans.is_empty() {
            content.push(Inline::Text(run));
            continue;
        }
        for (range, script) in spans {
            let mut style = run.style;
            let emphasized = style.emphasis || style.alternate_voice;
            match script {
                SemanticScriptClass::Cjk => {
                    if emphasized {
                        style.bold = true;
                    }
                    style.italic = false;
                }
                SemanticScriptClass::ItalicFriendly | SemanticScriptClass::Neutral => {
                    if emphasized || style.citation {
                        style.italic = true;
                    }
                }
            }
            content.push(Inline::Text(TextRun {
                text: run.text[range].to_owned(),
                style,
                link: run.link.clone(),
            }));
        }
    }
}

fn semantic_script_spans(
    text: &str,
    fallback_writing_system: WritingSystem,
) -> Vec<(Range<usize>, SemanticScriptClass)> {
    let mut clusters = text
        .grapheme_indices(true)
        .map(|(start, grapheme)| {
            (
                start..start + grapheme.len(),
                semantic_grapheme_script(grapheme),
                grapheme.chars().next(),
            )
        })
        .collect::<Vec<_>>();
    if clusters.is_empty() {
        return Vec::new();
    }
    let fallback = if fallback_writing_system == WritingSystem::Cjk {
        SemanticScriptClass::Cjk
    } else {
        SemanticScriptClass::ItalicFriendly
    };
    let mut next_strong = vec![None; clusters.len()];
    let mut next = None;
    for index in (0..clusters.len()).rev() {
        next_strong[index] = next;
        if clusters[index].1 != SemanticScriptClass::Neutral {
            next = Some(clusters[index].1);
        }
    }
    let mut previous = None;
    for (index, (_, script, first)) in clusters.iter_mut().enumerate() {
        if *script == SemanticScriptClass::Neutral {
            let right = next_strong[index];
            *script = if first.is_some_and(is_semantic_opening_punctuation) {
                right.or(previous).unwrap_or(fallback)
            } else if first.is_some_and(is_semantic_closing_punctuation) {
                previous.or(right).unwrap_or(fallback)
            } else {
                previous.or(right).unwrap_or(fallback)
            };
        }
        previous = Some(*script);
    }

    let mut spans: Vec<(Range<usize>, SemanticScriptClass)> = Vec::new();
    for (range, script, _) in clusters {
        if let Some((previous_range, previous_script)) = spans.last_mut()
            && *previous_script == script
            && previous_range.end == range.start
        {
            previous_range.end = range.end;
        } else {
            spans.push((range, script));
        }
    }
    spans
}

fn semantic_grapheme_script(grapheme: &str) -> SemanticScriptClass {
    for character in grapheme.chars() {
        match character.script() {
            Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul => {
                return SemanticScriptClass::Cjk;
            }
            Script::Latin | Script::Greek | Script::Cyrillic => {
                return SemanticScriptClass::ItalicFriendly;
            }
            _ => {}
        }
    }
    SemanticScriptClass::Neutral
}

fn is_semantic_opening_punctuation(character: char) -> bool {
    matches!(
        character,
        '《' | '〈' | '（' | '(' | '【' | '[' | '「' | '『' | '“' | '‘'
    )
}

fn is_semantic_closing_punctuation(character: char) -> bool {
    matches!(
        character,
        '》' | '〉' | '）' | ')' | '】' | ']' | '」' | '』' | '”' | '’'
    )
}

fn text_block_supports_space_justification(block: &TextBlock) -> bool {
    block.content.iter().all(|inline| match inline {
        Inline::Text(run) => run
            .text
            .chars()
            .all(|character| character != '\u{00a0}' && !linebreak::parley::is_cjk(character)),
        Inline::Math(_) | Inline::Image(_) | Inline::Break => true,
    })
}

fn paragraph_indent_em(profile: &ReaderTypesetting, writing_system: WritingSystem) -> f32 {
    if profile.paragraph_indent_mode == ParagraphIndentMode::Custom {
        return profile.paragraph_indent_em;
    }

    match writing_system {
        WritingSystem::Cjk => 2.0,
        WritingSystem::Latin => 1.5,
        WritingSystem::Other | WritingSystem::Unknown => profile.paragraph_indent_em,
    }
}

fn unified_heading_scale(h1_scale: f32, level: u8) -> f32 {
    let emphasis = (h1_scale - 1.0).max(0.0);
    1.0 + emphasis
        * match level {
            1 => 1.0,
            2 => 0.72,
            3 => 0.45,
            4 => 0.25,
            5 => 0.12,
            _ => 0.05,
        }
}

fn load_raster_image(
    source: &dyn BookSource,
    image: &ImageBlock,
) -> Result<RasterImage, LayoutError> {
    if let Some(raster) = source.raster_resource(&image.href)? {
        return Ok(RasterImage {
            width: raster.width,
            height: raster.height,
            pixels: raster.pixels,
        });
    }
    let resource = source.resource(&image.href)?;
    let decoded = image::load_from_memory(&resource.bytes)?.to_rgba8();
    Ok(RasterImage {
        width: decoded.width(),
        height: decoded.height(),
        pixels: decoded.into_raw().into(),
    })
}

fn dominant_paragraph_start_offset(fragments: &[&[Block]], content_width: f32) -> f32 {
    let mut offsets = fragments
        .iter()
        .flat_map(|blocks| blocks.iter())
        .filter_map(|block| match block {
            Block::Text(block) if block.kind == TextBlockKind::Paragraph => Some(
                (block.style.margin_start + content_width * block.style.margin_start_fraction)
                    .clamp(0.0, (content_width - 40.0).max(0.0)),
            ),
            Block::Text(_)
            | Block::Quote(_)
            | Block::Table(_)
            | Block::Image(_)
            | Block::Figure(_)
            | Block::Note(_)
            | Block::Separator(_)
            | Block::LineBreak
            | Block::PageBreak => None,
        })
        .collect::<Vec<_>>();
    if offsets.is_empty() {
        return 0.0;
    }
    offsets.sort_by(f32::total_cmp);
    offsets[(offsets.len() - 1) / 2]
}

fn apply_list_hanging_indent(
    layout: &mut Layout<TextBrush>,
    kind: TextBlockKind,
    marker_width: f32,
) {
    let TextBlockKind::ListItem { .. } = kind else {
        return;
    };
    // Keep wrapped list-item lines aligned with the text after the marker. The
    // marker remains in the leading area while continuation lines are indented.
    layout.set_text_indent(
        marker_width,
        IndentOptions {
            hanging: true,
            ..IndentOptions::default()
        },
    );
}

fn text_alignment(alignment: TextAlignment) -> Alignment {
    match alignment {
        TextAlignment::Start => Alignment::Start,
        TextAlignment::Center => Alignment::Center,
        TextAlignment::End => Alignment::End,
        TextAlignment::Justify => Alignment::Justify,
    }
}

fn fragments_are_standalone_cover(fragments: &[&[Block]], cover: Option<&PublicationUrl>) -> bool {
    let Some(cover) = cover else {
        return false;
    };
    let mut visible_blocks = fragments
        .iter()
        .flat_map(|blocks| blocks.iter())
        .filter(|block| !matches!(block, Block::LineBreak | Block::PageBreak));
    matches!(visible_blocks.next(), Some(Block::Image(image)) if &image.href == cover)
        && visible_blocks.next().is_none()
}

fn resolve_page_geometry(
    page_width: f32,
    page_height: f32,
    reader_style: &ReaderStyle,
) -> PageGeometry {
    let (content_left, content_width, column_count, continuation_offset_x) =
        resolve_horizontal_page_geometry(page_width, reader_style);
    let max_vertical_margin = page_height.mul_add(0.2, -8.0).max(20.0);
    let top_margin = reader_style.top_margin.min(max_vertical_margin);
    let bottom_margin = reader_style.bottom_margin.min(max_vertical_margin);
    let content_bottom = (page_height - bottom_margin).max(top_margin + 40.0);

    PageGeometry {
        left: content_left,
        top: top_margin,
        width: content_width,
        bottom: content_bottom,
        visible_pages: column_count,
        continuation_offset_x,
    }
}

fn resolve_horizontal_page_geometry(
    page_width: f32,
    reader_style: &ReaderStyle,
) -> (f32, f32, usize, f32) {
    let horizontal_margin = reader_style
        .horizontal_margin
        .min(page_width.mul_add(0.2, -8.0).max(20.0));
    let configured_column_gap = reader_style.column_gap.max(0.0);
    let double_available = page_width - horizontal_margin * 2.0 - configured_column_gap;
    let column_count = if reader_style.spread == SpreadMode::Double
        && double_available >= MIN_COLUMN_WIDTH * 2.0
    {
        2
    } else {
        1
    };
    let column_gap = if column_count == 2 {
        configured_column_gap
    } else {
        0.0
    };
    let column_divisor = if column_count == 2 { 2.0 } else { 1.0 };
    let content_width = ((page_width - horizontal_margin * 2.0 - column_gap) / column_divisor)
        .clamp(80.0, MAX_COLUMN_WIDTH);
    let spread_width = content_width * column_divisor + column_gap;
    let content_left = ((page_width - spread_width) / 2.0).max(horizontal_margin);
    (
        content_left,
        content_width,
        column_count,
        content_width + column_gap,
    )
}

/// Returns the horizontal start of the reading content for a viewport.
///
/// Reader chrome uses this to align its title with the exact same centered
/// single- or double-column geometry used by pagination.
pub fn reading_content_left(page_width: f32, reader_style: &ReaderStyle) -> f32 {
    resolve_horizontal_page_geometry(page_width, reader_style).0
}

/// Returns the width of one reading column for a viewport.
pub fn reading_content_width(page_width: f32, reader_style: &ReaderStyle) -> f32 {
    resolve_horizontal_page_geometry(page_width, reader_style).1
}

struct StyledRange {
    range: Range<usize>,
    style: TextStyle,
    footnote_reference_group: u32,
    hyphenation_suppressed: bool,
}

struct PreparedText {
    layout: Arc<Layout<TextBrush>>,
    text: Arc<str>,
    source_text_start: usize,
    start_offset: f32,
    available_width: f32,
    inline_images: Arc<[InlineImage]>,
    hyphens: Arc<[PreparedHyphen]>,
}

#[derive(Clone)]
struct PreparedHyphenGlyph {
    layout: Arc<Layout<TextBrush>>,
    text: Arc<str>,
    width: f32,
}

#[derive(Clone)]
struct PreparedHyphen {
    line_index: usize,
    glyph: PreparedHyphenGlyph,
}

struct PreparedTable {
    horizontal_offset: f32,
    column_widths: Vec<f32>,
    row_heights: Vec<f32>,
    cells: Vec<PreparedTableCell>,
    cell_padding: f32,
    center_content: bool,
    block_gap: f32,
    border: Rgba,
    header_fill: Rgba,
}

impl Default for PreparedTable {
    fn default() -> Self {
        Self {
            horizontal_offset: 0.0,
            column_widths: Vec::new(),
            row_heights: Vec::new(),
            cells: Vec::new(),
            cell_padding: 6.0,
            center_content: false,
            block_gap: TABLE_BLOCK_GAP,
            border: Rgba::BLACK,
            header_fill: Rgba {
                alpha: 0,
                ..Rgba::BLACK
            },
        }
    }
}

fn table_break_is_safe(table: &PreparedTable, row: usize) -> bool {
    row == table.row_heights.len()
        || !table
            .cells
            .iter()
            .any(|cell| cell.row < row && cell.row + cell.row_span > row)
}

fn next_safe_table_break(table: &PreparedTable, row_start: usize) -> usize {
    (row_start + 1..=table.row_heights.len())
        .find(|row| table_break_is_safe(table, *row))
        .unwrap_or(table.row_heights.len())
}

struct PreparedTableCell {
    row: usize,
    row_span: usize,
    column: usize,
    column_span: usize,
    header: bool,
    source: Option<SourceRange>,
    text: PreparedText,
    required_height: f32,
}

#[allow(
    clippy::cast_precision_loss,
    reason = "table column counts are bounded by the parsed table grid"
)]
fn fit_adaptive_column_widths(
    preferred_widths: &[f32],
    minimum_width: f32,
    available_width: f32,
) -> Vec<f32> {
    if preferred_widths.is_empty() || available_width <= 0.0 {
        return Vec::new();
    }
    let column_count = preferred_widths.len();
    let equal_width = available_width / column_count as f32;
    let minimum_width = minimum_width.min(equal_width).max(1.0);
    let minimum_total = minimum_width * column_count as f32;
    if minimum_total >= available_width {
        return vec![equal_width; column_count];
    }

    let preferred = preferred_widths
        .iter()
        .map(|width| width.max(minimum_width))
        .collect::<Vec<_>>();
    let preferred_total = preferred.iter().sum::<f32>();
    if preferred_total <= available_width {
        return preferred;
    }
    let mut fitted = {
        let available_flex = available_width - minimum_total;
        let preferred_flex = preferred
            .iter()
            .map(|width| width - minimum_width)
            .sum::<f32>();
        preferred
            .iter()
            .map(|width| {
                minimum_width
                    + available_flex * ((*width - minimum_width) / preferred_flex.max(1.0))
            })
            .collect::<Vec<_>>()
    };
    let fitted_total = fitted.iter().sum::<f32>();
    if let Some(last) = fitted.last_mut() {
        *last += available_width - fitted_total;
    }
    fitted
}

struct PreparedInlineImage {
    id: u64,
    index: usize,
    image: RasterImage,
    width: f32,
    height: f32,
    box_height: f32,
    offset_y: f32,
}

struct FixedPageReplacementRequest {
    text: String,
    rect: FixedPageTextRect,
    source: Option<SourceRange>,
}

fn fixed_page_replacement_block(text: &str, source: Option<SourceRange>) -> TextBlock {
    let mut content = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if index > 0 {
            content.push(Inline::Break);
        }
        if !line.is_empty() {
            content.push(Inline::Text(TextRun {
                text: line.to_owned(),
                style: TextStyle::default(),
                link: None,
            }));
        }
    }
    TextBlock {
        kind: TextBlockKind::Paragraph,
        content,
        style: rebook_publication::BlockStyle {
            line_height: 1.2,
            ..rebook_publication::BlockStyle::default()
        },
        source,
    }
}

fn prepared_text_height(prepared: &PreparedText) -> f32 {
    let Some(first) = prepared.layout.get(0) else {
        return 0.0;
    };
    let Some(last) = prepared.layout.get(prepared.layout.len().saturating_sub(1)) else {
        return 0.0;
    };
    (last.metrics().block_max_coord - first.metrics().block_min_coord).max(0.0)
}

fn prepared_flow_height(prepared: &PreparedText) -> f32 {
    let Some(first) = prepared.layout.get(0) else {
        return 0.0;
    };
    let Some(last) = prepared.layout.get(prepared.layout.len().saturating_sub(1)) else {
        return 0.0;
    };
    let metrics = last.metrics();
    let line_box_bottom = metrics.block_min_coord + metrics.line_height;
    (metrics.block_max_coord.max(line_box_bottom) - first.metrics().block_min_coord).max(0.0)
}

fn resolve_text_measure(
    block: &TextBlock,
    content_width: f32,
    minimum_width: f32,
) -> (f32, f32, f32) {
    let first_line_indented = matches!(
        block.kind,
        TextBlockKind::Paragraph | TextBlockKind::Blockquote
    );
    let first_line_indent = if first_line_indented {
        block.style.indent
    } else {
        0.0
    };
    let block_indent = if first_line_indented {
        0.0
    } else {
        block.style.indent
    };
    let start_offset = (block_indent
        + block.style.margin_start
        + content_width * block.style.margin_start_fraction)
        .clamp(0.0, (content_width - minimum_width).max(0.0));
    let available_width = (content_width - start_offset).max(minimum_width);
    (start_offset, available_width, first_line_indent)
}

fn prepare_inline_content(
    block: &TextBlock,
    fallback_color: Rgba,
    typography: &ReaderTypography,
    available_width: f32,
    svg_options: &resvg::usvg::Options<'_>,
    focus_footnote_icons: bool,
    inline_rasters: &[Option<RasterImage>],
) -> (String, Vec<StyledRange>, Vec<PreparedInlineImage>, usize) {
    let mut text = String::new();
    let mut spans = Vec::new();
    let mut inline_images = Vec::new();
    let mut next_footnote_reference_group = 1_u32;
    let prefix = list_marker_prefix(block.kind);
    if !prefix.is_empty() {
        let start = text.len();
        text.push_str(&prefix);
        spans.push(StyledRange {
            range: start..text.len(),
            style: TextStyle {
                color: fallback_color,
                ..TextStyle::default()
            },
            footnote_reference_group: 0,
            hyphenation_suppressed: true,
        });
    }
    let source_text_start = text.len();

    for (inline_index, inline) in block.content.iter().enumerate() {
        match inline {
            Inline::Text(run) => {
                let start = text.len();
                let mut style = run.style;
                if style.color == Rgba::BLACK {
                    style.color = fallback_color;
                }
                let linked_footnote_reference = run
                    .link
                    .as_ref()
                    .is_some_and(|target| target.fragment().is_some())
                    && (run.style.link_role == LinkRole::FootnoteReference
                        || (run.style.link_role == LinkRole::Normal
                            && run.style.baseline == TextBaseline::Superscript));
                let footnote_reference = focus_footnote_icons
                    && (run.style.inline_role == InlineRole::Footnote || linked_footnote_reference);
                if footnote_reference {
                    text.push_str(&footnote_icon_placeholder(&run.text));
                } else {
                    text.push_str(&run.text);
                }
                let footnote_reference_group = if footnote_reference {
                    let group = next_footnote_reference_group;
                    next_footnote_reference_group = next_footnote_reference_group.saturating_add(1);
                    group
                } else {
                    0
                };
                spans.push(StyledRange {
                    range: start..text.len(),
                    style,
                    footnote_reference_group,
                    hyphenation_suppressed: run.link.is_some()
                        || footnote_reference
                        || run.style.baseline != TextBaseline::Normal
                        || run.style.inline_role != InlineRole::Normal,
                });
            }
            Inline::Math(run) => {
                let id = u64::try_from(inline_images.len()).unwrap_or(u64::MAX);
                if let Ok(image) = rasterize_formula(
                    run,
                    typography,
                    fallback_color,
                    available_width,
                    svg_options,
                ) {
                    inline_images.push(PreparedInlineImage {
                        id,
                        index: text.len(),
                        width: image.1,
                        height: image.2,
                        box_height: image.2,
                        offset_y: 0.0,
                        image: image.0,
                    });
                } else {
                    let start = text.len();
                    text.push('$');
                    text.push_str(&run.latex);
                    text.push('$');
                    spans.push(StyledRange {
                        range: start..text.len(),
                        style: TextStyle {
                            size_scale: run.size_scale,
                            color: fallback_color,
                            ..TextStyle::default()
                        },
                        footnote_reference_group: 0,
                        hyphenation_suppressed: true,
                    });
                }
            }
            Inline::Image(run) => {
                let Some(image) = inline_rasters.get(inline_index).and_then(Clone::clone) else {
                    continue;
                };
                let id = u64::try_from(inline_images.len()).unwrap_or(u64::MAX);
                inline_images.push(prepare_inline_raster(
                    run,
                    image,
                    typography,
                    available_width,
                    id,
                    text.len(),
                ));
            }
            Inline::Break => {
                let compact_gap = text
                    .ends_with('\n')
                    .then_some(block.style.subparagraph_gap_em)
                    .flatten();
                if let Some(gap_em) = compact_gap {
                    let start = text.len();
                    text.push('\u{2060}');
                    spans.push(StyledRange {
                        range: start..text.len(),
                        style: TextStyle {
                            size_scale: (block.style.line_height + gap_em.clamp(0.0, 2.0))
                                / block.style.line_height.max(0.1),
                            color: fallback_color,
                            ..TextStyle::default()
                        },
                        footnote_reference_group: 0,
                        hyphenation_suppressed: true,
                    });
                } else {
                    text.push('\n');
                }
            }
        }
    }
    (text, spans, inline_images, source_text_start)
}

#[allow(clippy::cast_precision_loss)]
fn prepare_inline_raster(
    run: &rebook_publication::InlineImageRun,
    image: RasterImage,
    typography: &ReaderTypography,
    available_width: f32,
    id: u64,
    index: usize,
) -> PreparedInlineImage {
    let intrinsic_width = image.width.max(1) as f32;
    let intrinsic_height = image.height.max(1) as f32;
    let aspect_ratio = intrinsic_width / intrinsic_height;
    let surrounding_scale = run.size_scale.max(0.1);
    let large_intrinsic_illustration = run.intrinsic_sizing
        && run.image.style.width.is_none()
        && run.image.style.height.is_none()
        && !run.presentation
        && (intrinsic_width > 96.0 || intrinsic_height > 96.0);
    let authored_height = run.image.style.height.map(|height| match height {
        ImageLength::Pixels(pixels) => typography.font_size * surrounding_scale * pixels / 16.0,
        ImageLength::Fraction(fraction) => typography.font_size * surrounding_scale * fraction,
    });
    let authored_width = run.image.style.width.map(|width| match width {
        ImageLength::Pixels(pixels) => typography.font_size * surrounding_scale * pixels / 16.0,
        ImageLength::Fraction(fraction) => available_width * fraction,
    });
    let (mut requested_width, mut requested_height) = if large_intrinsic_illustration {
        (intrinsic_width, intrinsic_height)
    } else if run.intrinsic_sizing {
        if let Some(height) = authored_height {
            (height * aspect_ratio, height)
        } else if let Some(width) = authored_width {
            (width, width / aspect_ratio)
        } else {
            let height = typography.font_size * surrounding_scale * intrinsic_height / 16.0;
            (height * aspect_ratio, height)
        }
    } else {
        let height = typography.font_size * run.size_scale;
        (height * aspect_ratio, height)
    };
    let minimum_height = typography.font_size * 0.2;
    let maximum_height = if large_intrinsic_illustration {
        typography.font_size * 16.0
    } else {
        typography.font_size * 4.0
    };
    let height_scale =
        requested_height.clamp(minimum_height, maximum_height) / requested_height.max(1.0);
    requested_width *= height_scale;
    requested_height *= height_scale;
    let width_scale = (available_width / requested_width).min(1.0);
    let display_width = (requested_width * width_scale).max(1.0);
    let display_height = (requested_height * width_scale).max(1.0);
    let (box_height, offset_y) = inline_image_vertical_metrics(
        run.vertical_align,
        display_height,
        typography.font_size * surrounding_scale,
    );
    PreparedInlineImage {
        id,
        index,
        image,
        width: display_width,
        height: display_height,
        box_height,
        offset_y,
    }
}

fn inline_image_vertical_metrics(
    alignment: InlineImageAlignment,
    image_height: f32,
    surrounding_em: f32,
) -> (f32, f32) {
    let baseline_shift = match alignment {
        InlineImageAlignment::Baseline => 0.0,
        // Formula rasters in legacy EPUBs commonly use `vertical-align: middle`
        // to request optical centering in the text band. Center the image between
        // the same 0.8-em ascent and 0.2-em descent used below instead of applying
        // CSS's x-height offset, which places tightly cropped formula glyphs too low.
        InlineImageAlignment::Middle => image_height * 0.5 - surrounding_em * 0.3,
        InlineImageAlignment::TextTop | InlineImageAlignment::Top => {
            image_height - surrounding_em * 0.8
        }
        InlineImageAlignment::TextBottom
        | InlineImageAlignment::Bottom
        | InlineImageAlignment::Sub => surrounding_em * 0.2,
        InlineImageAlignment::Super => -surrounding_em * 0.35,
    };
    // Parley positions inline boxes with their bottom on the baseline. Reserve
    // the complete ascent/descent envelope, then paint the raster inside that
    // box at the authored baseline shift. This prevents a middle/sub-aligned
    // formula from visually colliding with the following line.
    let ascent = surrounding_em * 0.8;
    let descent = surrounding_em * 0.2;
    let above_baseline = (image_height - baseline_shift).max(0.0).max(ascent);
    let below_baseline = baseline_shift.max(0.0).max(descent);
    let box_height = (above_baseline + below_baseline).max(image_height);
    let paint_offset = box_height - image_height + baseline_shift;
    (box_height, paint_offset)
}

fn positioned_line_content_end(line: parley::layout::Line<'_, TextBrush>) -> f32 {
    let mut glyph_end = 0.0_f32;
    let mut inline_end = 0.0_f32;
    for item in line.items() {
        match item {
            PositionedLayoutItem::GlyphRun(run) => {
                glyph_end = glyph_end.max(run.offset() + run.advance());
            }
            PositionedLayoutItem::InlineBox(inline_box) => {
                inline_end = inline_end.max(inline_box.x + inline_box.width);
            }
        }
    }
    (glyph_end - line.metrics().trailing_whitespace)
        .max(inline_end)
        .max(0.0)
}

/// Reserves one compact glyph slot for a semantic footnote while retaining the
/// original scalar count used by source-offset mapping. Remaining scalars become
/// zero-width word joiners so markers such as `【3】` do not leave three ems of
/// blank space around the replacement icon.
fn footnote_icon_placeholder(marker: &str) -> String {
    marker
        .chars()
        .enumerate()
        .map(|(index, _)| if index == 0 { '0' } else { '\u{2060}' })
        .collect()
}

fn list_marker_prefix(kind: TextBlockKind) -> String {
    match kind {
        TextBlockKind::ListItem {
            marker_visible: false,
            ..
        } => String::new(),
        TextBlockKind::ListItem {
            ordered: true,
            ordinal,
            ..
        } => format!("{ordinal}.\u{00a0}"),
        TextBlockKind::ListItem { ordered: false, .. } => "•\u{00a0}".to_owned(),
        _ => String::new(),
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "formula dimensions are clamped to bounded reader viewport pixels"
)]
fn rasterize_formula(
    run: &MathRun,
    typography: &ReaderTypography,
    color: Rgba,
    available_width: f32,
    svg_options: &resvg::usvg::Options<'_>,
) -> Result<(RasterImage, f32, f32), String> {
    use resvg::tiny_skia::Pixmap;
    use resvg::usvg::{Transform, Tree};

    const RASTER_SCALE: f32 = 2.0;
    const PADDING: f32 = 1.5;
    let semantic_scale = if run.display { 1.12 } else { 1.0 };
    let font_size = (typography.font_size * run.size_scale.clamp(0.5, 3.0) * semantic_scale)
        .max(typography.minimum_font_size);
    let text_color = format!("#{:02x}{:02x}{:02x}", color.red, color.green, color.blue);
    let rendered = rebook_math::math::render_math(&run.latex, font_size, &text_color, run.display)?;
    let source_width = (rendered.width + PADDING * 2.0).max(1.0);
    let source_height = (rendered.ascent + rendered.descent + PADDING * 2.0).max(1.0);
    let width_scale = (available_width / source_width).min(1.0);
    let display_width = (source_width * width_scale).max(1.0);
    let display_height = (source_height * width_scale).max(1.0);
    let view_y = -rendered.ascent - PADDING;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}" width="{}" height="{}"><g transform="translate({}, 0)">{}</g></svg>"#,
        -PADDING,
        view_y,
        source_width,
        source_height,
        source_width,
        source_height,
        PADDING,
        rendered.svg_fragment
    );
    let tree = Tree::from_data(svg.as_bytes(), svg_options).map_err(|error| error.to_string())?;
    let pixel_width = (display_width * RASTER_SCALE).ceil().max(1.0) as u32;
    let pixel_height = (display_height * RASTER_SCALE).ceil().max(1.0) as u32;
    let mut pixmap = Pixmap::new(pixel_width, pixel_height)
        .ok_or_else(|| format!("failed to allocate formula raster {pixel_width}x{pixel_height}"))?;
    resvg::render(
        &tree,
        Transform::from_scale(
            pixel_width as f32 / source_width,
            pixel_height as f32 / source_height,
        ),
        &mut pixmap.as_mut(),
    );
    Ok((
        RasterImage {
            width: pixel_width,
            height: pixel_height,
            pixels: pixmap.data().to_vec().into(),
        },
        display_width,
        display_height,
    ))
}
