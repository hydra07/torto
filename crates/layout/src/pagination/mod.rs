struct Paginator {
    viewport: LayoutViewport,
    background: Rgba,
    left: f32,
    top: f32,
    width: f32,
    bottom: f32,
    column_has_content: bool,
    cursor_y: f32,
    pages: Vec<PageLayout>,
    items: Vec<PageItem>,
    center_standalone_image: bool,
    minimum_paragraph_gap: f32,
    previous_block_was_paragraph: bool,
    media_start_offset: f32,
    leading_gap: f32,
    pending_leading_gap: f32,
    forced_page_break: bool,
    active_quote: Option<ActiveQuote>,
}

struct ActiveQuote {
    sources: Vec<SourceRange>,
    fill: Rgba,
    accent: Rgba,
    outer_gap: f32,
    decoration_index: Option<usize>,
    has_started: bool,
}

#[derive(Clone, Copy)]
struct PageGeometry {
    left: f32,
    top: f32,
    width: f32,
    bottom: f32,
    visible_pages: usize,
    continuation_offset_x: f32,
}

impl Paginator {
    fn new(
        viewport: LayoutViewport,
        background: Rgba,
        geometry: PageGeometry,
        center_standalone_image: bool,
        minimum_paragraph_gap: f32,
    ) -> Self {
        Self {
            viewport,
            background,
            left: geometry.left,
            top: geometry.top,
            width: geometry.width,
            bottom: geometry.bottom,
            column_has_content: false,
            cursor_y: geometry.top,
            pages: Vec::new(),
            items: Vec::new(),
            center_standalone_image,
            minimum_paragraph_gap: minimum_paragraph_gap.max(0.0),
            previous_block_was_paragraph: false,
            media_start_offset: 0.0,
            leading_gap: 0.0,
            pending_leading_gap: 0.0,
            forced_page_break: false,
            active_quote: None,
        }
    }

    fn begin_quote(&mut self, sources: Vec<SourceRange>, foreground: Rgba, outer_gap: f32) {
        self.previous_block_was_paragraph = false;
        self.ensure_minimum_spacing(outer_gap);
        self.active_quote = Some(ActiveQuote {
            sources,
            fill: Rgba {
                alpha: 0,
                ..foreground
            },
            accent: quote_accent_for_foreground(foreground),
            outer_gap,
            decoration_index: None,
            has_started: false,
        });
    }

    fn ensure_quote_decoration(&mut self) {
        let Some(active) = self.active_quote.as_ref() else {
            return;
        };
        if active.decoration_index.is_some() {
            return;
        }
        let continued_before = active.has_started;
        let sources = active.sources.clone();
        let fill = active.fill;
        let accent = active.accent;
        let index = self.items.len();
        self.items.push(PageItem::Quote(QuotePlacement {
            x: self.column_left(),
            y: self.cursor_y,
            width: self.width,
            height: QUOTE_VERTICAL_PADDING,
            continued_before,
            continued_after: false,
            fill,
            accent,
            sources,
        }));
        self.cursor_y = (self.cursor_y + QUOTE_VERTICAL_PADDING).min(self.bottom);
        if let Some(active) = self.active_quote.as_mut() {
            active.decoration_index = Some(index);
            active.has_started = true;
        }
    }

    fn pending_quote_padding(&self) -> f32 {
        self.active_quote.as_ref().map_or(0.0, |active| {
            if active.decoration_index.is_none() {
                QUOTE_VERTICAL_PADDING
            } else {
                0.0
            }
        })
    }

    fn update_quote_decoration(&mut self) {
        let Some(index) = self
            .active_quote
            .as_ref()
            .and_then(|active| active.decoration_index)
        else {
            return;
        };
        if let Some(PageItem::Quote(quote)) = self.items.get_mut(index) {
            quote.height = (self.cursor_y - quote.y).max(QUOTE_VERTICAL_PADDING);
        }
    }

    fn end_quote(&mut self) {
        if self.active_quote.is_none() {
            return;
        }
        let outer_gap = self
            .active_quote
            .as_ref()
            .map_or(QUOTE_VERTICAL_PADDING, |active| active.outer_gap);
        let decoration_index = self
            .active_quote
            .as_ref()
            .and_then(|active| active.decoration_index);
        if let Some(index) = decoration_index {
            self.cursor_y = (self.cursor_y + QUOTE_VERTICAL_PADDING).min(self.bottom);
            self.update_quote_decoration();
            if let Some(PageItem::Quote(quote)) = self.items.get_mut(index) {
                quote.continued_after = false;
            }
        } else if let Some(PageItem::Quote(quote)) = self.pages.last_mut().and_then(|page| {
            page.items
                .iter_mut()
                .rev()
                .find(|item| matches!(item, PageItem::Quote(_)))
        }) {
            quote.continued_after = false;
        }
        self.active_quote = None;
        self.previous_block_was_paragraph = false;
        self.add_preserved_spacing(outer_gap);
    }

    fn push_text(&mut self, prepared: &PreparedText, block: &TextBlock) -> Result<(), LayoutError> {
        self.forced_page_break = false;
        let is_paragraph = matches!(block.kind, TextBlockKind::Paragraph);
        if is_paragraph && self.previous_block_was_paragraph {
            self.ensure_minimum_spacing(self.minimum_paragraph_gap);
        }
        self.add_preserved_spacing(block.style.margin_before);
        let mut line_start = 0;
        while line_start < prepared.layout.len() {
            let first = prepared
                .layout
                .get(line_start)
                .ok_or(LayoutError::InvalidLayout)?;
            let first_top = first.metrics().block_min_coord;
            let mut line_end = line_start;
            let mut slice_height = 0.0;
            while line_end < prepared.layout.len() {
                let line = prepared
                    .layout
                    .get(line_end)
                    .ok_or(LayoutError::InvalidLayout)?;
                let candidate_height = line.metrics().block_max_coord - first_top;
                let remaining = self.bottom - self.cursor_y - self.pending_quote_padding();
                if candidate_height > remaining && line_end > line_start {
                    break;
                }
                if candidate_height > remaining && self.column_has_content {
                    self.advance_column();
                    break;
                }
                slice_height = candidate_height.max(line.metrics().line_height);
                line_end += 1;
            }
            if line_end == line_start {
                continue;
            }
            // Do not start a quote decoration until its first text line fits on
            // this column. Otherwise a page boundary can retain an orphaned
            // accent bar above the actual quotation.
            self.ensure_quote_decoration();
            let origin_x = self.column_left() + prepared.start_offset;
            let origin_y = self.cursor_y - first_top;
            self.items.push(PageItem::Text(TextPlacement {
                layout: Arc::clone(&prepared.layout),
                text: Arc::clone(&prepared.text),
                source_text_start: prepared.source_text_start,
                lines: line_start..line_end,
                origin_x,
                origin_y,
                available_width: prepared.available_width,
                source: block.source.clone(),
                inline_images: Arc::clone(&prepared.inline_images),
            }));
            for hyphen in prepared
                .hyphens
                .iter()
                .filter(|hyphen| (line_start..line_end).contains(&hyphen.line_index))
            {
                let line = prepared
                    .layout
                    .get(hyphen.line_index)
                    .ok_or(LayoutError::InvalidLayout)?;
                let glyph_line = hyphen
                    .glyph
                    .layout
                    .get(0)
                    .ok_or(LayoutError::InvalidLayout)?;
                self.items.push(PageItem::Text(TextPlacement {
                    layout: Arc::clone(&hyphen.glyph.layout),
                    text: Arc::clone(&hyphen.glyph.text),
                    source_text_start: 0,
                    lines: 0..1,
                    origin_x: origin_x + positioned_line_content_end(line),
                    origin_y: origin_y + line.metrics().baseline - glyph_line.metrics().baseline,
                    available_width: hyphen.glyph.width,
                    source: None,
                    inline_images: Arc::from([]),
                }));
            }
            self.pending_leading_gap = 0.0;
            self.column_has_content = true;
            self.cursor_y += slice_height;
            self.update_quote_decoration();
            line_start = line_end;
            if line_start < prepared.layout.len() {
                self.advance_column();
            }
        }
        // Parley's glyph block bounds can be shorter than the complete line
        // box, especially after translation switches to a CJK fallback font.
        // Selection/highlight geometry covers the line box, so anchor the
        // authored paragraph margin after that same box before advancing.
        self.ensure_minimum_spacing(0.0);
        self.add_preserved_spacing(block.style.margin_after);
        self.update_quote_decoration();
        self.previous_block_was_paragraph = is_paragraph;
        Ok(())
    }

    fn push_table(&mut self, table: &PreparedTable) {
        self.forced_page_break = false;
        self.previous_block_was_paragraph = false;
        if table.row_heights.is_empty() || table.column_widths.is_empty() {
            return;
        }
        self.ensure_minimum_spacing(table.block_gap);
        let mut row_start = 0;
        while row_start < table.row_heights.len() {
            let remaining = self.bottom - self.cursor_y;
            let mut height = 0.0;
            let mut last_safe_break = None;
            for row_end in row_start + 1..=table.row_heights.len() {
                let candidate = height + table.row_heights[row_end - 1];
                if candidate > remaining && row_end > row_start + 1 {
                    break;
                }
                height = candidate;
                if table_break_is_safe(table, row_end) {
                    last_safe_break = Some((row_end, height));
                }
                if candidate > remaining {
                    break;
                }
            }
            let Some((row_end, chunk_height)) = last_safe_break else {
                if self.column_has_content {
                    self.advance_column();
                    continue;
                }
                let row_end = next_safe_table_break(table, row_start);
                let chunk_height = table.row_heights[row_start..row_end].iter().sum();
                self.push_table_chunk(table, row_start, row_end, chunk_height);
                row_start = row_end;
                if row_start < table.row_heights.len() {
                    self.advance_column();
                }
                continue;
            };
            if chunk_height > remaining && self.column_has_content {
                self.advance_column();
                continue;
            }
            self.push_table_chunk(table, row_start, row_end, chunk_height);
            row_start = row_end;
            if row_start < table.row_heights.len() {
                self.advance_column();
            }
        }
        self.add_preserved_spacing(table.block_gap);
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "resolved grid coordinates come from bounded table spans and row content"
    )]
    fn push_table_chunk(
        &mut self,
        table: &PreparedTable,
        row_start: usize,
        row_end: usize,
        height: f32,
    ) {
        let table_y = self.cursor_y;
        let mut row_offsets = Vec::with_capacity(row_end - row_start + 1);
        row_offsets.push(0.0);
        for row_height in &table.row_heights[row_start..row_end] {
            row_offsets.push(row_offsets.last().copied().unwrap_or(0.0) + row_height);
        }
        let cells = table
            .cells
            .iter()
            .filter(|cell| cell.row >= row_start && cell.row + cell.row_span <= row_end)
            .map(|cell| {
                let local_row = cell.row - row_start;
                let cell_x = self.column_left()
                    + self.media_start_offset
                    + table.horizontal_offset
                    + table.column_widths[..cell.column].iter().sum::<f32>();
                let cell_y = table_y + row_offsets[local_row];
                let cell_width = table.column_widths[cell.column..cell.column + cell.column_span]
                    .iter()
                    .sum::<f32>();
                let cell_height = row_offsets[local_row + cell.row_span] - row_offsets[local_row];
                let text = cell.text.layout.get(0).map(|first| {
                    let top_padding = if table.center_content {
                        ((cell_height - prepared_text_height(&cell.text)) / 2.0).max(0.0)
                    } else {
                        table.cell_padding
                    };
                    TextPlacement {
                        layout: Arc::clone(&cell.text.layout),
                        text: Arc::clone(&cell.text.text),
                        source_text_start: cell.text.source_text_start,
                        lines: 0..cell.text.layout.len(),
                        origin_x: cell_x + table.cell_padding + cell.text.start_offset,
                        origin_y: cell_y + top_padding - first.metrics().block_min_coord,
                        available_width: cell.text.available_width,
                        source: cell.source.clone(),
                        inline_images: Arc::clone(&cell.text.inline_images),
                    }
                });
                TableCellPlacement {
                    x: cell_x,
                    y: cell_y,
                    width: cell_width,
                    height: cell_height,
                    header: cell.header,
                    text,
                }
            })
            .collect();
        self.items.push(PageItem::Table(TablePlacement {
            cells,
            y: table_y,
            height,
            border: table.border,
            header_fill: table.header_fill,
        }));
        self.pending_leading_gap = 0.0;
        self.column_has_content = true;
        self.cursor_y += height;
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "decoded image dimensions are bounded by publication resource limits"
    )]
    fn image_display_size(&self, image: &RasterImage, style: ImageStyle) -> (f32, f32) {
        let intrinsic_width = image.width.max(1) as f32;
        let intrinsic_height = image.height.max(1) as f32;
        let aspect_ratio = intrinsic_width / intrinsic_height;
        let content_height = self.bottom - self.top;
        let media_width = (self.width - self.media_start_offset).max(1.0);
        let requested_height = style.height.map(|height| height.resolve(content_height));
        let requested_width = style
            .width
            .map(|width| width.resolve(media_width))
            .or_else(|| requested_height.map(|height| height * aspect_ratio))
            .unwrap_or(intrinsic_width)
            .max(1.0);
        let requested_height = requested_height
            .unwrap_or(requested_width / aspect_ratio)
            .max(1.0);
        let max_width = style
            .max_width
            .map_or(media_width, |width| width.resolve(media_width))
            .clamp(1.0, media_width);
        let max_height = style
            .max_height
            .map_or(content_height, |height| height.resolve(content_height))
            .clamp(1.0, content_height);
        let scale = (max_width / requested_width)
            .min(max_height / requested_height)
            .min(1.0);
        (requested_width * scale, requested_height * scale)
    }

    fn prepare_group(&mut self, content_height: f32, outer_gap: f32) {
        self.pending_leading_gap = outer_gap.max(0.0);
        self.ensure_minimum_spacing(outer_gap);
        let full_height = self.bottom - self.top;
        if content_height <= full_height
            && self.cursor_y + content_height > self.bottom
            && self.column_has_content
        {
            self.advance_column();
            self.leading_gap = self.leading_gap.max(outer_gap.max(0.0));
        }
    }

    fn keep_together_if_fits(&mut self, content_height: f32) {
        let content_height = content_height.max(0.0);
        let full_height = self.bottom - self.top;
        if content_height <= full_height
            && self.cursor_y + content_height > self.bottom
            && self.column_has_content
        {
            self.advance_column();
        }
    }

    fn push_image(
        &mut self,
        image: RasterImage,
        style: ImageStyle,
        source: Option<SourceRange>,
        text_layer: Option<FixedPageTextLayer>,
    ) -> Vec<FixedPageReplacementRequest> {
        self.push_image_with_gaps(
            image,
            style,
            source,
            text_layer,
            IMAGE_BLOCK_GAP,
            IMAGE_BLOCK_GAP,
        )
    }

    fn push_image_with_gaps(
        &mut self,
        image: RasterImage,
        style: ImageStyle,
        source: Option<SourceRange>,
        text_layer: Option<FixedPageTextLayer>,
        minimum_before: f32,
        minimum_after: f32,
    ) -> Vec<FixedPageReplacementRequest> {
        let restore_gap_on_empty_page = !self.column_has_content
            && self.items.is_empty()
            && !self.pages.is_empty()
            && !self.forced_page_break;
        self.forced_page_break = false;
        self.previous_block_was_paragraph = false;
        let (width, height) = self.image_display_size(&image, style);
        let media_width = (self.width - self.media_start_offset).max(1.0);
        let block_gap = style.margin_before.max(minimum_before);
        let page_count_before_spacing = self.pages.len();
        self.ensure_minimum_spacing(block_gap);
        if self.cursor_y + height > self.bottom && self.column_has_content {
            self.advance_column();
        }
        if restore_gap_on_empty_page || self.pages.len() > page_count_before_spacing {
            self.leading_gap = self.leading_gap.max(block_gap);
        }
        let x = self.column_left() + self.media_start_offset + (media_width - width) / 2.0;
        let replacements = text_layer.as_ref().map_or_else(Vec::new, |layer| {
            let Some(replacement) = layer.replacement.as_ref() else {
                return Vec::new();
            };
            if layer.width <= 0.0 || layer.height <= 0.0 {
                return Vec::new();
            }
            let scale_x = width / layer.width;
            let scale_y = height / layer.height;
            replacement
                .segments
                .iter()
                .map(|segment| FixedPageReplacementRequest {
                    text: segment.text.clone(),
                    rect: FixedPageTextRect {
                        x: x + segment.rect.x * scale_x,
                        y: self.cursor_y + segment.rect.y * scale_y,
                        width: segment.rect.width * scale_x,
                        height: segment.rect.height * scale_y,
                    },
                    source: fixed_page_replacement_source(source.as_ref(), segment),
                })
                .collect()
        });
        self.items.push(PageItem::Image(ImagePlacement {
            image,
            x,
            y: self.cursor_y,
            width,
            height,
            source,
            text_layer,
            replacement: None,
        }));
        let trailing_gap = style.margin_after.max(minimum_after).max(0.0);
        self.pending_leading_gap = 0.0;
        if trailing_gap < self.bottom - self.top {
            self.pending_leading_gap = trailing_gap;
        }
        self.column_has_content = true;
        self.cursor_y += height + trailing_gap;
        replacements
    }

    fn push_fixed_page_replacement(
        &mut self,
        prepared: &PreparedText,
        request: FixedPageReplacementRequest,
    ) -> Result<(), LayoutError> {
        let Some(first) = prepared.layout.get(0) else {
            return Ok(());
        };
        let Some(PageItem::Image(image)) = self.items.last_mut() else {
            return Err(LayoutError::InvalidLayout);
        };
        let padding = 1.5;
        let segment = FixedPageTextReplacementSegmentPlacement {
            rect: request.rect,
            text: TextPlacement {
                layout: Arc::clone(&prepared.layout),
                text: Arc::clone(&prepared.text),
                source_text_start: prepared.source_text_start,
                lines: 0..prepared.layout.len(),
                origin_x: request.rect.x + padding,
                origin_y: request.rect.y + padding - first.metrics().block_min_coord,
                available_width: prepared.available_width,
                source: request.source,
                inline_images: Arc::clone(&prepared.inline_images),
            },
        };
        image
            .replacement
            .get_or_insert_with(|| FixedPageTextReplacementPlacement {
                segments: Vec::new(),
            })
            .segments
            .push(segment);
        Ok(())
    }

    fn ensure_minimum_spacing(&mut self, amount: f32) {
        let amount = amount.max(0.0);
        let Some(content_bottom) = self.current_content_bottom() else {
            if !self.pages.is_empty() && !self.forced_page_break {
                self.leading_gap = self.leading_gap.max(amount);
            }
            return;
        };
        self.pending_leading_gap = self.pending_leading_gap.max(amount);
        let target = content_bottom + amount;
        if target > self.bottom {
            self.advance_column();
            self.leading_gap = self.leading_gap.max(amount);
        } else {
            self.cursor_y = self.cursor_y.max(target);
        }
    }

    fn current_content_bottom(&self) -> Option<f32> {
        self.items.last().and_then(|item| match item {
            PageItem::Text(text) => text
                .lines
                .end
                .checked_sub(1)
                .and_then(|line| text.layout.get(line))
                .map(|line| {
                    let metrics = line.metrics();
                    let line_box_bottom = metrics.block_min_coord + metrics.line_height;
                    text.origin_y + metrics.block_max_coord.max(line_box_bottom)
                }),
            PageItem::Quote(quote) => Some(quote.y + quote.height),
            PageItem::Image(image) => Some(image.y + image.height),
            PageItem::Table(table) => Some(table.y + table.height),
            PageItem::Separator(separator) => Some(separator.y + 1.0),
        })
    }

    fn push_separator(&mut self) {
        self.forced_page_break = false;
        self.previous_block_was_paragraph = false;
        self.add_spacing(12.0);
        if self.cursor_y + 1.0 > self.bottom && self.column_has_content {
            self.advance_column();
        }
        self.items.push(PageItem::Separator(SeparatorPlacement {
            x: self.column_left() + self.width * 0.25,
            y: self.cursor_y,
            width: self.width * 0.5,
        }));
        self.pending_leading_gap = 0.0;
        self.column_has_content = true;
        self.cursor_y += 13.0;
    }

    fn add_spacing(&mut self, amount: f32) {
        let amount = amount.max(0.0);
        if self.cursor_y + amount > self.bottom && self.column_has_content {
            self.advance_column();
        } else {
            self.cursor_y += amount;
        }
    }

    fn add_preserved_spacing(&mut self, amount: f32) {
        let amount = amount.max(0.0);
        let page_height = self.bottom - self.top;
        let preserved_amount = if amount < page_height { amount } else { 0.0 };
        if self.cursor_y + amount > self.bottom && self.column_has_content {
            self.pending_leading_gap += preserved_amount;
            self.advance_column();
        } else {
            self.cursor_y += amount;
            if !self.column_has_content
                && self.items.is_empty()
                && !self.pages.is_empty()
                && !self.forced_page_break
            {
                self.leading_gap += preserved_amount;
            } else if self.column_has_content {
                self.pending_leading_gap += preserved_amount;
            }
        }
    }

    fn add_semantic_spacing(&mut self, amount: f32) {
        self.add_preserved_spacing(amount);
    }

    fn force_page(&mut self) {
        self.pending_leading_gap = 0.0;
        self.forced_page_break = true;
        if self.column_has_content || !self.items.is_empty() {
            self.advance_column();
        }
    }

    fn column_left(&self) -> f32 {
        self.left
    }

    fn advance_column(&mut self) {
        self.update_quote_decoration();
        if let Some(index) = self
            .active_quote
            .as_ref()
            .and_then(|active| active.decoration_index)
            && let Some(PageItem::Quote(quote)) = self.items.get_mut(index)
        {
            quote.continued_after = true;
            quote.height = (self.bottom - quote.y).max(quote.height);
        }
        let pending_leading_gap = self.pending_leading_gap;
        self.commit_page();
        if let Some(active) = self.active_quote.as_mut() {
            active.decoration_index = None;
        }
        self.leading_gap = self.leading_gap.max(pending_leading_gap);
    }

    fn commit_page(&mut self) {
        if self.items.is_empty() {
            self.cursor_y = self.top;
            return;
        }
        if self.center_standalone_image
            && let [PageItem::Image(image)] = self.items.as_mut_slice()
        {
            let available_height = self.bottom - self.top;
            let centered_y = self.top + ((available_height - image.height) / 2.0).max(0.0);
            let offset_y = centered_y - image.y;
            image.y = centered_y;
            if let Some(replacement) = image.replacement.as_mut() {
                for segment in &mut replacement.segments {
                    segment.rect.y += offset_y;
                    segment.text.origin_y += offset_y;
                }
            }
        }
        self.pages.push(PageLayout {
            viewport: self.viewport,
            background: self.background,
            leading_gap: std::mem::take(&mut self.leading_gap),
            items: std::mem::take(&mut self.items),
        });
        self.column_has_content = false;
        self.cursor_y = self.top;
    }

    fn finish(mut self) -> Vec<PageLayout> {
        self.commit_page();
        if self.pages.is_empty() {
            self.pages.push(PageLayout {
                viewport: self.viewport,
                background: self.background,
                leading_gap: 0.0,
                items: Vec::new(),
            });
        }
        self.pages
    }
}

fn fixed_page_replacement_source(
    source: Option<&SourceRange>,
    segment: &rebook_publication::FixedPageTextReplacementSegment,
) -> Option<SourceRange> {
    let mut source = source?.clone();
    let start = source
        .start
        .text_offset
        .saturating_add(segment.source_offset);
    source.start.text_offset = start;
    source.end.spine = source.start.spine.clone();
    source.end.node.clone_from(&source.start.node);
    source.end.text_offset =
        start.saturating_add(u64::try_from(segment.text.chars().count()).unwrap_or(u64::MAX));
    Some(source)
}

/// Native layout errors.
#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("viewport dimensions must be positive")]
    InvalidViewport,
    #[error("text layout produced inconsistent line metrics")]
    InvalidLayout,
    #[error(transparent)]
    Publication(#[from] PublicationError),
    #[error("image decode failed: {0}")]
    Image(#[from] ImageError),
    #[error("layout resource limit exceeded: {0}")]
    ResourceLimit(String),
}
