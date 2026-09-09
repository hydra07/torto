// Vello expands already-positioned outlines in display units, so this does not
// alter Parley's advances or chosen line breaks.
const SYNTHETIC_EMBOLDEN_EM: f64 = 0.025;

/// Pointer hit inside one retained text placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTextHit {
    pub region_index: usize,
    /// Caret boundary nearest to the pointer, used to determine drag direction.
    pub byte_index: usize,
    /// Logical byte range of the shaped cluster or fixed-page text span under the pointer.
    pub cluster_start: usize,
    pub cluster_end: usize,
}

/// One durable, single-block piece of a visual text selection.
#[derive(Debug, Clone)]
pub struct PageSelectionFragment {
    pub range: SourceRange,
    pub quote: String,
    pub rects: Vec<Rect>,
}

/// Original raster content for the top-most image under a page coordinate.
#[derive(Clone)]
pub struct PageImageHit {
    pub bounds: Rect,
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>,
}

/// Paint style for connecting one semantic quote across continuously stitched pages.
#[derive(Clone, Copy)]
pub struct PageQuoteBridge {
    pub x: f32,
    pub width: f32,
    pub color: Color,
}

/// Retained drawing commands for one page. No parsing, shaping, or pagination
/// occurs while this list is replayed.
pub struct PageDisplayList {
    width: u32,
    height: u32,
    content_top: Option<f32>,
    content_bottom: Option<f32>,
    leading_gap: f32,
    background: Color,
    commands: Vec<DisplayCommand>,
    text_regions: Vec<TextRegion>,
    inline_content_regions: Vec<InlineContentRegion>,
    table_regions: Vec<TableRegion>,
    quote_regions: Vec<QuoteRegion>,
    footnote_regions: Vec<FootnoteRegion>,
}

struct InlineContentRegion {
    bounds: Rect,
    source: SourceRange,
}

struct TableRegion {
    bounds: Rect,
    sources: Vec<SourceRange>,
}

struct QuoteRegion {
    bounds: Rect,
    sources: Vec<SourceRange>,
    continued_before: bool,
    continued_after: bool,
    accent_x: f32,
    accent_width: f32,
    accent: Color,
}

struct FootnoteRegion {
    bounds: Rect,
    source: SourceRange,
}

const FOOTNOTE_ICON_CENTER_ABOVE_BASELINE: f32 = 0.68;

fn footnote_icon_bounds(center_x: f32, baseline: f32, font_size: f32) -> Rect {
    let size = (font_size * 0.78).clamp(8.0, 12.0);
    // Footnote sources are not uniform: some books use a superscript link while
    // others embed a normal-baseline inline note. The replacement icon should
    // occupy one stable optical superscript position regardless of that source
    // encoding. A center slightly over two thirds of its diameter above the
    // text baseline aligns with the upper half of both CJK em boxes and Latin
    // cap height without changing the placement of ordinary superscript text.
    let center_y = baseline - size * FOOTNOTE_ICON_CENTER_ABOVE_BASELINE;
    Rect::new(
        f64::from(center_x - size / 2.0),
        f64::from(center_y - size / 2.0),
        f64::from(center_x + size / 2.0),
        f64::from(center_y + size / 2.0),
    )
}

fn paint_footnote_region(
    scene: &mut impl PaintScene,
    footnote: &FootnoteRegion,
    color: Color,
    transform: Affine,
) {
    let bounds = footnote.bounds;
    let circle = Circle::new(bounds.center(), bounds.width().min(bounds.height()) * 0.5);
    scene.stroke(&Stroke::new(1.15), transform, color, None, &circle);
    let center_x = bounds.center().x;
    let dot = Rect::new(
        center_x - 0.7,
        bounds.y0 + 2.0,
        center_x + 0.7,
        bounds.y0 + 3.4,
    );
    scene.fill(Fill::NonZero, transform, color, None, &dot);
    scene.stroke(
        &Stroke::new(1.2),
        transform,
        color,
        None,
        &Line::new((center_x, bounds.y0 + 5.0), (center_x, bounds.y1 - 2.0)),
    );
}

const HIGHLIGHT_VERTICAL_OVERLAP: f64 = 0.5;

fn source_range_highlight_path(rects: impl IntoIterator<Item = Rect>) -> BezPath {
    let mut path = BezPath::new();
    for rect in rects {
        let rect = Rect::new(
            rect.x0,
            rect.y0 - HIGHLIGHT_VERTICAL_OVERLAP,
            rect.x1,
            rect.y1 + HIGHLIGHT_VERTICAL_OVERLAP,
        );
        path.extend(rect.path_elements(0.0));
    }
    path
}

impl PageDisplayList {
    /// Logical width of the compiled page.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Logical height of the compiled page.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Top edge of retained page content in logical page coordinates.
    pub fn content_top(&self) -> Option<f32> {
        self.content_top
    }

    /// Bottom edge of the retained page content in logical page coordinates.
    ///
    /// Unlike the page height, this excludes unused pagination space after the
    /// final text line or image.
    pub fn content_bottom(&self) -> Option<f32> {
        self.content_bottom
    }

    /// Semantic spacing removed when the first block was moved to this page.
    pub fn leading_gap(&self) -> f32 {
        self.leading_gap
    }

    /// Number of retained commands, useful for diagnostics.
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    /// Number of source-backed text placements on this logical page.
    pub fn text_region_count(&self) -> usize {
        self.text_regions.len()
    }

    /// Bounds of retained raster page content in logical page coordinates.
    pub fn image_bounds(&self) -> Option<Rect> {
        self.commands
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::Image(command) => Some(command.bounds),
                DisplayCommand::Glyphs(_)
                | DisplayCommand::FillRect(_)
                | DisplayCommand::FillRoundedRect(_)
                | DisplayCommand::Rule(_) => None,
            })
            .reduce(|bounds, next| bounds.union(next))
    }

    /// Raster resources referenced by this retained page.
    ///
    /// The desktop GPU renderer uses these handles to refresh Vello's image
    /// atlas before replaying a scene. Cloning an [`ImageData`] is cheap and
    /// preserves the blob identity encoded into the Vello scene.
    pub fn image_data(&self) -> impl Iterator<Item = &ImageData> {
        self.commands.iter().filter_map(|command| match command {
            DisplayCommand::Image(command) => Some(&command.image.image),
            DisplayCommand::Glyphs(_)
            | DisplayCommand::FillRect(_)
            | DisplayCommand::FillRoundedRect(_)
            | DisplayCommand::Rule(_) => None,
        })
    }

    /// Returns the top-most retained raster image under the given page coordinate.
    pub fn image_at(&self, x: f32, y: f32) -> Option<PageImageHit> {
        let point = kurbo::Point::new(f64::from(x), f64::from(y));
        self.commands
            .iter()
            .rev()
            .find_map(|command| match command {
                DisplayCommand::Image(command)
                    if command.interactive && command.bounds.contains(point) =>
                {
                    Some(PageImageHit {
                        bounds: command.bounds,
                        width: command.width,
                        height: command.height,
                        pixels: Arc::clone(&command.pixels),
                    })
                }
                DisplayCommand::Glyphs(_)
                | DisplayCommand::Image(_)
                | DisplayCommand::FillRect(_)
                | DisplayCommand::FillRoundedRect(_)
                | DisplayCommand::Rule(_) => None,
            })
    }

    /// Resolves source-backed block images to page-coordinate rectangles.
    pub fn image_source_rects(&self, ranges: &[SourceRange]) -> Vec<Rect> {
        self.commands
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::Image(command)
                    if command
                        .source
                        .as_ref()
                        .is_some_and(|source| ranges.iter().any(|range| range == source)) =>
                {
                    Some(command.bounds)
                }
                DisplayCommand::Glyphs(_)
                | DisplayCommand::Image(_)
                | DisplayCommand::FillRect(_)
                | DisplayCommand::FillRoundedRect(_)
                | DisplayCommand::Rule(_) => None,
            })
            .collect()
    }

    /// Visible UTF-8 byte range for a retained text placement.
    pub fn text_region_visible_range(&self, region_index: usize) -> Option<Range<usize>> {
        self.text_regions
            .get(region_index)
            .and_then(TextRegion::visible_byte_range)
    }

    /// Full shaped text retained for a source-backed region.
    pub fn text_region_text(&self, region_index: usize) -> Option<&str> {
        self.text_regions.get(region_index).map(TextRegion::text)
    }

    /// Full selectable byte range, including text outside this logical page's
    /// visible line slice when a paragraph continues onto another page.
    pub fn text_region_selectable_range(&self, region_index: usize) -> Option<Range<usize>> {
        self.text_regions
            .get(region_index)
            .map(TextRegion::selectable_byte_range)
    }

    /// Maps a byte range in retained text to its durable authored source range.
    pub fn text_region_source_range(
        &self,
        region_index: usize,
        byte_range: Range<usize>,
    ) -> Option<SourceRange> {
        self.text_regions
            .get(region_index)?
            .source_range_for_bytes(byte_range)
    }

    /// Returns the visible byte intersection of a durable source range in one
    /// retained text region.
    pub fn text_region_byte_range_for_source(
        &self,
        region_index: usize,
        range: &SourceRange,
    ) -> Option<Range<usize>> {
        self.text_regions
            .get(region_index)?
            .byte_range_for_source(range)
    }

    /// Returns the first durable source range visible on this page.
    ///
    /// This is used as the primary reading-position anchor. Unlike page numbers,
    /// the source range survives viewport, font, and pagination changes.
    pub fn leading_source_range(&self) -> Option<SourceRange> {
        self.text_regions
            .iter()
            .find_map(TextRegion::visible_source_range)
    }

    /// Returns the source-backed block nearest a vertical page coordinate.
    /// Continuous readers use this to persist the paragraph, table, or image at
    /// the top of the viewport instead of falling back to the page's first block.
    pub fn source_range_nearest_y(&self, y: f32) -> Option<SourceRange> {
        let mut nearest: Option<(f32, SourceRange)> = None;
        let mut consider = |distance: f32, range: SourceRange| {
            if nearest
                .as_ref()
                .is_none_or(|(current, _)| distance < *current)
            {
                nearest = Some((distance, range));
            }
        };
        for region in &self.text_regions {
            if let Some(range) = region.visible_source_range() {
                consider(region.vertical_distance(y), range);
            }
        }
        for table in &self.table_regions {
            if let Some(range) = table.sources.first() {
                consider(vertical_rect_distance(table.bounds, y), range.clone());
            }
        }
        for command in &self.commands {
            if let DisplayCommand::Image(image) = command
                && let Some(source) = &image.source
            {
                consider(vertical_rect_distance(image.bounds, y), source.clone());
            }
        }
        nearest.map(|(_, range)| range)
    }

    /// Hit-tests source-backed text. Exact mode is used when a drag starts;
    /// nearest mode lets a drag extend naturally through line/column whitespace.
    pub fn hit_test_text(&self, x: f32, y: f32, exact: bool) -> Option<PageTextHit> {
        if exact {
            return self
                .text_regions
                .iter()
                .enumerate()
                .find_map(|(index, region)| {
                    region.hit_test(x, y, true).map(|hit| PageTextHit {
                        region_index: index,
                        byte_index: hit.byte_index,
                        cluster_start: hit.cluster_start,
                        cluster_end: hit.cluster_end,
                    })
                });
        }

        self.text_regions
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                left.vertical_distance(y)
                    .total_cmp(&right.vertical_distance(y))
            })
            .and_then(|(index, region)| {
                region.hit_test(x, y, false).map(|hit| PageTextHit {
                    region_index: index,
                    byte_index: hit.byte_index,
                    cluster_start: hit.cluster_start,
                    cluster_end: hit.cluster_end,
                })
            })
    }

    /// Resolves a byte range in one retained placement to source anchors,
    /// selected text, and page-coordinate rectangles.
    pub fn selection_fragment(
        &self,
        region_index: usize,
        byte_range: Range<usize>,
    ) -> Option<PageSelectionFragment> {
        self.text_regions
            .get(region_index)?
            .selection_fragment(byte_range)
    }

    /// Resolves durable source ranges to page-coordinate highlight rectangles.
    pub fn source_rects(&self, ranges: &[SourceRange]) -> Vec<Rect> {
        let mut rects = self
            .text_regions
            .iter()
            .flat_map(|region| {
                ranges
                    .iter()
                    .filter_map(|range| region.byte_range_for_source(range))
                    .flat_map(|range| region.selection_rects(range))
            })
            .chain(
                self.inline_content_regions
                    .iter()
                    .filter(|region| ranges.iter().any(|range| range == &region.source))
                    .map(|region| region.bounds),
            )
            .collect::<Vec<_>>();
        rects.sort_by(|left, right| {
            left.y0
                .total_cmp(&right.y0)
                .then_with(|| left.x0.total_cmp(&right.x0))
        });
        rects
    }

    /// Resolves table cell source ranges to their complete table-chunk bounds.
    pub fn source_table_bounds(&self, ranges: &[SourceRange]) -> Vec<Rect> {
        self.table_regions
            .iter()
            .filter(|table| {
                table
                    .sources
                    .iter()
                    .any(|source| ranges.iter().any(|range| range == source))
            })
            .map(|table| table.bounds)
            .collect()
    }

    /// Resolves quote child ranges to their complete semantic card bounds.
    pub fn source_quote_bounds(&self, ranges: &[SourceRange]) -> Vec<Rect> {
        self.quote_regions
            .iter()
            .filter(|quote| {
                quote
                    .sources
                    .iter()
                    .any(|source| ranges.iter().any(|range| range == source))
            })
            .map(|quote| quote.bounds)
            .collect()
    }

    /// Resolves a semantic quote or preformatted text range to one rectangular
    /// block suitable for focus-mode activation painting.
    pub fn source_block_bounds(&self, ranges: &[SourceRange]) -> Option<Rect> {
        let quote = self.source_quote_bounds(ranges);
        if !quote.is_empty() {
            return quote.into_iter().reduce(|bounds, next| bounds.union(next));
        }
        self.text_regions
            .iter()
            .filter_map(|region| {
                ranges
                    .iter()
                    .find_map(|range| region.block_bounds_for_source(range))
            })
            .reduce(|bounds, next| bounds.union(next))
            .map(|bounds| {
                Rect::new(
                    bounds.x0 - 8.0,
                    bounds.y0 - 6.0,
                    bounds.x1 + 8.0,
                    bounds.y1 + 6.0,
                )
            })
    }

    /// Paints one opaque rounded-rectangle activation fill below page text.
    pub fn paint_source_block_background(
        &self,
        scene: &mut impl PaintScene,
        ranges: &[SourceRange],
        color: Color,
        offset_x: f32,
    ) {
        let Some(bounds) = self.source_block_bounds(ranges) else {
            return;
        };
        let background = RoundedRect::from_rect(bounds, 7.0);
        scene.fill(
            Fill::NonZero,
            Affine::translate((f64::from(offset_x), 0.0)),
            color,
            None,
            &background,
        );
    }

    /// Returns the accent style when this page and `next` are consecutive
    /// slices of the same semantic quotation.
    pub fn quote_bridge_to(&self, next: &Self) -> Option<PageQuoteBridge> {
        let trailing = self
            .quote_regions
            .iter()
            .rev()
            .find(|quote| quote.continued_after)?;
        let leading = next
            .quote_regions
            .iter()
            .find(|quote| quote.continued_before)?;
        if trailing.sources.is_empty()
            || trailing.sources != leading.sources
            || (trailing.accent_x - leading.accent_x).abs() > 0.5
            || (trailing.accent_width - leading.accent_width).abs() > 0.5
        {
            return None;
        }
        Some(PageQuoteBridge {
            x: trailing.accent_x,
            width: trailing.accent_width,
            color: trailing.accent,
        })
    }

    /// Returns the union of text, image, and table geometry belonging to the
    /// supplied semantic source ranges.
    pub fn source_content_bounds(&self, ranges: &[SourceRange]) -> Option<Rect> {
        self.source_rects(ranges)
            .into_iter()
            .chain(self.image_source_rects(ranges))
            .chain(self.source_table_bounds(ranges))
            .chain(self.source_quote_bounds(ranges))
            .reduce(|bounds, next| bounds.union(next))
    }

    pub fn contains_source_anchor(&self, anchor: &SourceAnchor) -> bool {
        self.text_regions
            .iter()
            .any(|region| region.contains_source_anchor(anchor))
    }

    pub fn source_ranges_contain_point(&self, ranges: &[SourceRange], x: f32, y: f32) -> bool {
        self.source_rects(ranges)
            .iter()
            .any(|rect| rect.contains(kurbo::Point::new(f64::from(x), f64::from(y))))
    }

    /// Replays this page into any `AnyRender` backend, including Vello GPU and CPU.
    pub fn paint(&self, scene: &mut impl PaintScene) {
        self.paint_scaled(scene, 1.0);
    }

    /// Replays logical page coordinates at the window's device scale.
    pub fn paint_scaled(&self, scene: &mut impl PaintScene, scale_factor: f32) {
        self.paint_scaled_at(scene, scale_factor, 0.0, 0.0);
    }

    /// Replays the page at a logical offset, used to compose reader chrome and
    /// the book surface without re-compiling either display list.
    pub fn paint_scaled_at(
        &self,
        scene: &mut impl PaintScene,
        scale_factor: f32,
        offset_x: f32,
        offset_y: f32,
    ) {
        let scale = Affine::scale(f64::from(scale_factor.max(0.1)))
            * Affine::translate((f64::from(offset_x), f64::from(offset_y)));
        self.paint_background_with_transform(scene, scale);
        self.paint_content_with_transform(scene, scale);
    }

    /// Paints only the page background. Spread composition paints this once,
    /// then overlays one or two logical page display lists.
    pub fn paint_background(&self, scene: &mut impl PaintScene) {
        self.paint_background_with_transform(scene, Affine::IDENTITY);
    }

    /// Paints only the page background at a horizontal spread/transition offset.
    pub fn paint_background_at(&self, scene: &mut impl PaintScene, offset_x: f32) {
        self.paint_background_with_transform(scene, Affine::translate((f64::from(offset_x), 0.0)));
    }

    /// Paints retained page content without covering content already composed
    /// into the same spread.
    pub fn paint_content_at(&self, scene: &mut impl PaintScene, offset_x: f32) {
        self.paint_content_with_transform(scene, Affine::translate((f64::from(offset_x), 0.0)));
    }

    /// Paints fixed-page raster content below source range overlays.
    pub fn paint_images_at(&self, scene: &mut impl PaintScene, offset_x: f32) {
        let transform = Affine::translate((f64::from(offset_x), 0.0));
        for command in &self.commands {
            if command.paints_below_source_overlays() {
                command.paint(scene, transform);
            }
        }
    }

    /// Paints text and rules above source range overlays.
    pub fn paint_non_image_content_at(&self, scene: &mut impl PaintScene, offset_x: f32) {
        let transform = Affine::translate((f64::from(offset_x), 0.0));
        for command in &self.commands {
            if matches!(command, DisplayCommand::Glyphs(_) | DisplayCommand::Rule(_)) {
                command.paint(scene, transform);
            }
        }
    }

    /// Paints translucent source-backed marks below page content.
    pub fn paint_source_ranges(
        &self,
        scene: &mut impl PaintScene,
        ranges: &[SourceRange],
        color: Color,
        offset_x: f32,
    ) {
        let transform = Affine::translate((f64::from(offset_x), 0.0));
        let path = source_range_highlight_path(self.source_rects(ranges));
        if !path.is_empty() {
            // Separate translucent AA rectangles can expose one-pixel conflation
            // seams at shared line edges in Vello. Paint one slightly-overlapped
            // non-zero path so adjacent rows are composited exactly once:
            // https://github.com/linebender/vello/issues/49
            // https://github.com/linebender/vello/issues/417
            scene.fill(Fill::NonZero, transform, color, None, &path);
        }
    }

    /// Paints compact footnote icons for the active focus-mode source ranges.
    pub fn paint_footnote_icons(
        &self,
        scene: &mut impl PaintScene,
        ranges: &[SourceRange],
        color: Color,
        offset_x: f32,
    ) {
        let transform = Affine::translate((f64::from(offset_x), 0.0));
        for footnote in &self.footnote_regions {
            if !ranges.iter().any(|range| range == &footnote.source) {
                continue;
            }
            paint_footnote_region(scene, footnote, color, transform);
        }
    }

    /// Paints every semantic footnote icon on a classic-mode page.
    pub fn paint_all_footnote_icons(
        &self,
        scene: &mut impl PaintScene,
        color: Color,
        offset_x: f32,
    ) {
        let transform = Affine::translate((f64::from(offset_x), 0.0));
        for footnote in &self.footnote_regions {
            paint_footnote_region(scene, footnote, color, transform);
        }
    }

    /// Returns the source-backed paragraph owning a semantic footnote icon.
    pub fn footnote_source_at(&self, x: f32, y: f32) -> Option<SourceRange> {
        let point = Point::new(f64::from(x), f64::from(y));
        self.footnote_regions
            .iter()
            .rev()
            .find(|footnote| footnote.bounds.contains(point))
            .map(|footnote| footnote.source.clone())
    }

    /// Paints block-level outlines for table chunks containing any requested source range.
    pub fn paint_source_table_borders(
        &self,
        scene: &mut impl PaintScene,
        ranges: &[SourceRange],
        color: Color,
        offset_x: f32,
    ) {
        let transform = Affine::translate((f64::from(offset_x), 0.0));
        let first = ranges.first();
        let last = ranges.last();
        for table in &self.table_regions {
            if table
                .sources
                .iter()
                .any(|source| ranges.iter().any(|range| range == source))
            {
                let left = table.bounds.x0;
                let top = table.bounds.y0;
                let right = table.bounds.x1;
                let bottom = table.bounds.y1;
                let contains = |range: Option<&SourceRange>| {
                    range.is_some_and(|range| table.sources.iter().any(|source| source == range))
                };
                let mut edges = vec![
                    Line::new((left, top), (left, bottom)),
                    Line::new((right, top), (right, bottom)),
                ];
                if contains(first) {
                    edges.push(Line::new((left, top), (right, top)));
                }
                if contains(last) {
                    edges.push(Line::new((left, bottom), (right, bottom)));
                }
                for edge in edges {
                    scene.stroke(&Stroke::new(2.0), transform, color, None, &edge);
                }
            }
        }
    }

    fn paint_background_with_transform(&self, scene: &mut impl PaintScene, transform: Affine) {
        scene.fill(
            Fill::NonZero,
            transform,
            self.background,
            None,
            &Rect::new(0.0, 0.0, f64::from(self.width), f64::from(self.height)),
        );
    }

    fn paint_content_with_transform(&self, scene: &mut impl PaintScene, transform: Affine) {
        for command in &self.commands {
            command.paint(scene, transform);
        }
    }
}
