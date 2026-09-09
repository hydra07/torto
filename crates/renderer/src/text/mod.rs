#[allow(
    clippy::cast_possible_truncation,
    reason = "display-list coordinates are viewport-bounded f32 values stored in kurbo f64"
)]
fn vertical_rect_distance(rect: Rect, y: f32) -> f32 {
    let y = f64::from(y);
    if y < rect.y0 {
        (rect.y0 - y) as f32
    } else if y > rect.y1 {
        (y - rect.y1) as f32
    } else {
        0.0
    }
}

enum TextRegion {
    Shaped(ShapedTextRegion),
    Fixed(FixedTextRegion),
}

struct TextRegionHit {
    byte_index: usize,
    cluster_start: usize,
    cluster_end: usize,
}

impl TextRegion {
    fn text(&self) -> &str {
        match self {
            Self::Shaped(region) => &region.text,
            Self::Fixed(region) => &region.text,
        }
    }

    fn selectable_byte_range(&self) -> Range<usize> {
        match self {
            Self::Shaped(region) => region.source_text_start..region.text.len(),
            Self::Fixed(region) => 0..region.text.len(),
        }
    }

    fn visible_byte_range(&self) -> Option<Range<usize>> {
        match self {
            Self::Shaped(region) => region.visible_byte_range(),
            Self::Fixed(region) => region.visible_byte_range(),
        }
    }

    fn visible_source_range(&self) -> Option<SourceRange> {
        let visible = self.visible_byte_range()?;
        match self {
            Self::Shaped(region) => region.source_range_for_bytes(visible),
            Self::Fixed(region) => region.source_range_for_bytes(visible),
        }
    }

    fn source_range_for_bytes(&self, byte_range: Range<usize>) -> Option<SourceRange> {
        match self {
            Self::Shaped(region) => region.source_range_for_bytes(byte_range),
            Self::Fixed(region) => region.source_range_for_bytes(byte_range),
        }
    }

    fn vertical_distance(&self, y: f32) -> f32 {
        match self {
            Self::Shaped(region) => region.vertical_distance(y),
            Self::Fixed(region) => region.vertical_distance(y),
        }
    }

    fn hit_test(&self, x: f32, y: f32, exact: bool) -> Option<TextRegionHit> {
        match self {
            Self::Shaped(region) => region.hit_test(x, y, exact),
            Self::Fixed(region) => region.hit_test(x, y, exact),
        }
    }

    fn selection_fragment(&self, byte_range: Range<usize>) -> Option<PageSelectionFragment> {
        match self {
            Self::Shaped(region) => region.selection_fragment(byte_range),
            Self::Fixed(region) => region.selection_fragment(byte_range),
        }
    }

    fn selection_rects(&self, byte_range: Range<usize>) -> Vec<Rect> {
        match self {
            Self::Shaped(region) => region.selection_rects(byte_range),
            Self::Fixed(region) => region.selection_rects(byte_range),
        }
    }

    fn byte_range_for_source(&self, range: &SourceRange) -> Option<Range<usize>> {
        match self {
            Self::Shaped(region) => region.byte_range_for_source(range),
            Self::Fixed(region) => region.byte_range_for_source(range),
        }
    }

    fn block_bounds_for_source(&self, range: &SourceRange) -> Option<Rect> {
        let byte_range = self.byte_range_for_source(range)?;
        match self {
            Self::Shaped(region) => region.block_bounds(),
            Self::Fixed(region) => region
                .selection_rects(byte_range)
                .into_iter()
                .reduce(|bounds, next| bounds.union(next)),
        }
    }

    fn contains_source_anchor(&self, anchor: &SourceAnchor) -> bool {
        match self {
            Self::Shaped(region) => region.contains_source_anchor(anchor),
            Self::Fixed(region) => region.contains_source_anchor(anchor),
        }
    }
}

struct ShapedTextRegion {
    layout: Arc<Layout<TextBrush>>,
    text: Arc<str>,
    source_text_start: usize,
    lines: Range<usize>,
    origin_x: f32,
    origin_y: f32,
    available_width: f32,
    source: SourceRange,
}

impl ShapedTextRegion {
    fn visible_byte_range(&self) -> Option<Range<usize>> {
        let mut visible = self.lines.clone().filter_map(|line_index| {
            let line = self.layout.get(line_index)?;
            let range = line.text_range();
            let start = range.start.max(self.source_text_start).min(self.text.len());
            let end = range.end.max(self.source_text_start).min(self.text.len());
            (end > start).then_some(start..end)
        });
        let first = visible.next()?;
        let end = visible.last().map_or(first.end, |range| range.end);
        Some(first.start..end)
    }

    fn vertical_bounds(&self) -> Option<(f32, f32)> {
        let first = self.layout.get(self.lines.start)?;
        let last = self.layout.get(self.lines.end.checked_sub(1)?)?;
        Some((
            first.metrics().block_min_coord + self.origin_y,
            last.metrics().block_max_coord + self.origin_y,
        ))
    }

    fn block_bounds(&self) -> Option<Rect> {
        let (top, bottom) = self.vertical_bounds()?;
        Some(Rect::new(
            f64::from(self.origin_x),
            f64::from(top),
            f64::from(self.origin_x + self.available_width),
            f64::from(bottom),
        ))
    }

    fn vertical_distance(&self, y: f32) -> f32 {
        let Some((top, bottom)) = self.vertical_bounds() else {
            return f32::MAX;
        };
        if y < top {
            top - y
        } else if y > bottom {
            y - bottom
        } else {
            0.0
        }
    }

    fn hit_test(&self, x: f32, y: f32, exact: bool) -> Option<TextRegionHit> {
        let (top, bottom) = self.vertical_bounds()?;
        if exact && !(top..=bottom).contains(&y) {
            return None;
        }
        let local_x = x - self.origin_x;
        let local_y = if exact {
            y - self.origin_y
        } else {
            y.clamp(top + 0.01, bottom - 0.01) - self.origin_y
        };
        let (byte_index, cluster_start, cluster_end) = if exact {
            let (cluster, side) = Cluster::from_point_exact(&self.layout, local_x, local_y)?;
            let range = cluster.text_range();
            let byte_index = if cluster.is_rtl() {
                if side == ClusterSide::Left {
                    range.end
                } else {
                    range.start
                }
            } else if side == ClusterSide::Left {
                range.start
            } else {
                range.end
            };
            (byte_index, range.start, range.end)
        } else {
            let byte_index = Cursor::from_point(&self.layout, local_x, local_y).index();
            (byte_index, byte_index, byte_index)
        };
        let visible = self.visible_byte_range()?;
        Some(TextRegionHit {
            byte_index: byte_index.clamp(visible.start, visible.end),
            cluster_start: cluster_start.clamp(visible.start, visible.end),
            cluster_end: cluster_end.clamp(visible.start, visible.end),
        })
    }

    fn selection_fragment(&self, byte_range: Range<usize>) -> Option<PageSelectionFragment> {
        let visible = self.visible_byte_range()?;
        let start = floor_char_boundary(
            &self.text,
            byte_range.start.clamp(visible.start, visible.end),
        );
        let end = floor_char_boundary(&self.text, byte_range.end.clamp(visible.start, visible.end));
        if end <= start {
            return None;
        }
        let range = self.source_range_for_bytes(start..end)?;
        Some(PageSelectionFragment {
            range,
            quote: self.text.get(start..end)?.to_owned(),
            rects: self.selection_rects(start..end),
        })
    }

    fn selection_rects(&self, byte_range: Range<usize>) -> Vec<Rect> {
        if byte_range.end <= byte_range.start {
            return Vec::new();
        }
        let shared_wrapped_end = shared_wrapped_content_end(&self.layout);
        let selection = Selection::new(
            Cursor::from_byte_index(&self.layout, byte_range.start, Affinity::Downstream),
            Cursor::from_byte_index(&self.layout, byte_range.end, Affinity::Upstream),
        );
        let selected_text = selection.text_range();
        selection
            .geometry(&self.layout)
            .into_iter()
            .filter(|(_, line_index)| self.lines.contains(line_index))
            .map(|(rect, line_index)| {
                let mut x0 = rect.x0;
                let mut x1 = rect.x1;

                // Parley selection geometry can use a stale reported measure for fully
                // selected justified lines. Keep every soft-wrapped line on one shared
                // right edge, but derive that edge from both the reported measure and
                // the actual positioned content so the background never ends inside a
                // rendered word:
                // https://github.com/linebender/parley/issues/396
                if let Some(line) = self.layout.get(line_index) {
                    let line_text = line.text_range();
                    // Synthetic list markers precede `source_text_start` and are
                    // intentionally not selectable. A source-backed selection that
                    // starts at the first real character still covers the complete
                    // visual first line, so ignore the marker-only byte prefix when
                    // deciding whether the line should receive full-width geometry.
                    let selectable_line_start = line_text.start.max(self.source_text_start);
                    // The marker-to-text boundary can include kerning not present in
                    // the separately measured hanging indent. Extend only the first
                    // source-backed edge to that inset, never into the marker area.
                    if line_text.start < self.source_text_start
                        && selected_text.start <= self.source_text_start
                        && let Some(next) = self.layout.get(line_index + 1)
                    {
                        let inset =
                            f64::from(next.metrics().offset + next.metrics().inline_min_coord);
                        if inset
                            > f64::from(line.metrics().offset + line.metrics().inline_min_coord)
                        {
                            x0 = x0.min(inset);
                        }
                    }
                    if selected_text.start <= selectable_line_start
                        && selected_text.end >= line_text.end
                    {
                        let visual_start =
                            f64::from(line.metrics().offset + line.metrics().inline_min_coord);
                        let soft_wrapped = line.break_reason() == BreakReason::Regular;
                        let visual_end = if soft_wrapped {
                            // A soft-wrapped line occupies the full paragraph measure,
                            // even when an unbreakable Latin word leaves visible space at
                            // its end. Keep the final/explicit line content-sized, but make
                            // every wrapped line's active highlight share one right edge.
                            // The shared value is already expressed in the paragraph
                            // coordinate space. A hanging indent is carried separately
                            // by `offset`; adding it here a second time makes list
                            // highlights protrude past the paragraph's right edge.
                            f64::from(shared_wrapped_end)
                        } else {
                            let text_advance = line
                                .runs()
                                .map(|run| {
                                    run.visual_clusters()
                                        .map(|cluster| cluster.advance())
                                        .sum::<f32>()
                                })
                                .sum::<f32>();
                            let inline_box_advance = line
                                .items()
                                .filter_map(|item| match item {
                                    PositionedLayoutItem::InlineBox(inline_box) => {
                                        Some(inline_box.width)
                                    }
                                    PositionedLayoutItem::GlyphRun(_) => None,
                                })
                                .sum::<f32>();
                            visual_start + f64::from(text_advance + inline_box_advance)
                        };
                        if soft_wrapped {
                            // The first list line may start with a synthetic marker
                            // before `source_text_start`. Keep Parley's source-backed
                            // selection start so the marker is not highlighted; only
                            // extend the right edge to the shared wrapped-line limit.
                            if line_text.start >= self.source_text_start {
                                x0 = visual_start.min(visual_end);
                            }
                            x1 = visual_start.max(visual_end);
                        } else {
                            if line_text.start >= self.source_text_start {
                                x0 = x0.min(visual_start.min(visual_end));
                            }
                            x1 = x1.max(visual_start.max(visual_end));
                        }
                    }
                }

                Rect::new(
                    x0 + f64::from(self.origin_x),
                    rect.y0 + f64::from(self.origin_y),
                    x1 + f64::from(self.origin_x),
                    rect.y1 + f64::from(self.origin_y),
                )
            })
            .collect()
    }

    fn source_range_for_bytes(&self, byte_range: Range<usize>) -> Option<SourceRange> {
        if self.source.start.spine != self.source.end.spine
            || self.source.start.node != self.source.end.node
        {
            return None;
        }
        let source_start = self.source.start.text_offset;
        let source_length = self.source.end.text_offset.checked_sub(source_start)?;
        let source_text = self.text.get(self.source_text_start..)?;
        let text_length = source_text.chars().count();
        let start_chars = self
            .text
            .get(self.source_text_start..byte_range.start)?
            .chars()
            .count();
        let end_chars = self
            .text
            .get(self.source_text_start..byte_range.end)?
            .chars()
            .count();
        let start = source_start
            + scale_text_offset_to_source(start_chars, text_length, source_length, false)?;
        let end = source_start
            + scale_text_offset_to_source(end_chars, text_length, source_length, true)?;
        Some(SourceRange {
            start: SourceAnchor {
                spine: self.source.start.spine.clone(),
                node: self.source.start.node.clone(),
                text_offset: start,
            },
            end: SourceAnchor {
                spine: self.source.start.spine.clone(),
                node: self.source.start.node.clone(),
                text_offset: end,
            },
        })
    }

    fn byte_range_for_source(&self, range: &SourceRange) -> Option<Range<usize>> {
        if self.source.start.spine != self.source.end.spine
            || self.source.start.node != self.source.end.node
            || range.start.spine != range.end.spine
            || range.start.node != range.end.node
            || self.source.start.spine != range.start.spine
            || self.source.start.node != range.start.node
        {
            return None;
        }
        let start_offset = range
            .start
            .text_offset
            .max(self.source.start.text_offset)
            .min(self.source.end.text_offset);
        let end_offset = range
            .end
            .text_offset
            .max(self.source.start.text_offset)
            .min(self.source.end.text_offset);
        if end_offset <= start_offset {
            return None;
        }
        let source_text = self.text.get(self.source_text_start..)?;
        let text_length = source_text.chars().count();
        let source_length = self
            .source
            .end
            .text_offset
            .checked_sub(self.source.start.text_offset)?;
        let start_chars = scale_source_offset_to_text(
            start_offset - self.source.start.text_offset,
            source_length,
            text_length,
            false,
        )?;
        let end_chars = scale_source_offset_to_text(
            end_offset - self.source.start.text_offset,
            source_length,
            text_length,
            true,
        )?;
        let start = self.source_text_start + byte_index_for_char_offset(source_text, start_chars);
        let end = self.source_text_start + byte_index_for_char_offset(source_text, end_chars);
        let visible = self.visible_byte_range()?;
        let start = start.max(visible.start).min(visible.end);
        let end = end.max(visible.start).min(visible.end);
        (end > start).then_some(start..end)
    }

    fn contains_source_anchor(&self, anchor: &SourceAnchor) -> bool {
        self.visible_byte_range()
            .and_then(|range| self.source_range_for_bytes(range))
            .is_some_and(|range| source_range_contains(&range, anchor))
    }
}

fn positioned_line_content_end(line: parley::layout::Line<'_, TextBrush>) -> f32 {
    let mut glyph_end = 0.0_f32;
    let mut inline_end = 0.0_f32;
    for item in line.items() {
        match item {
            PositionedLayoutItem::GlyphRun(glyph_run) => {
                glyph_end = glyph_end.max(glyph_run.offset() + glyph_run.advance());
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

fn shared_wrapped_content_end(layout: &Layout<TextBrush>) -> f32 {
    layout
        .lines()
        .filter(|line| line.break_reason() == BreakReason::Regular)
        .fold(0.0_f32, |right, line| {
            right
                .max(line.metrics().inline_max_coord)
                .max(positioned_line_content_end(line))
        })
}

fn scale_text_offset_to_source(
    offset: usize,
    text_length: usize,
    source_length: u64,
    round_up: bool,
) -> Option<u64> {
    if text_length == 0 {
        return Some(0);
    }
    let numerator = u128::try_from(offset).ok()? * u128::from(source_length);
    let denominator = u128::try_from(text_length).ok()?;
    let scaled = if round_up {
        numerator.div_ceil(denominator)
    } else {
        numerator / denominator
    };
    u64::try_from(scaled).ok()
}

fn scale_source_offset_to_text(
    offset: u64,
    source_length: u64,
    text_length: usize,
    round_up: bool,
) -> Option<usize> {
    if source_length == 0 {
        return Some(0);
    }
    let numerator = u128::from(offset) * u128::try_from(text_length).ok()?;
    let denominator = u128::from(source_length);
    let scaled = if round_up {
        numerator.div_ceil(denominator)
    } else {
        numerator / denominator
    };
    usize::try_from(scaled).ok()
}

struct FixedTextRegion {
    text: Arc<str>,
    spans: Arc<[FixedTextSpan]>,
    source: SourceRange,
}

#[derive(Clone)]
struct FixedTextSpan {
    byte_range: Range<usize>,
    rect: Rect,
}

impl FixedTextRegion {
    fn visible_byte_range(&self) -> Option<Range<usize>> {
        (!self.text.is_empty() && !self.spans.is_empty()).then_some(0..self.text.len())
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "fixed page coordinates originate from bounded f32 layout dimensions"
    )]
    fn vertical_bounds(&self) -> Option<(f32, f32)> {
        let top = self
            .spans
            .iter()
            .map(|span| span.rect.y0 as f32)
            .min_by(f32::total_cmp)?;
        let bottom = self
            .spans
            .iter()
            .map(|span| span.rect.y1 as f32)
            .max_by(f32::total_cmp)?;
        Some((top, bottom))
    }

    fn vertical_distance(&self, y: f32) -> f32 {
        let Some((top, bottom)) = self.vertical_bounds() else {
            return f32::MAX;
        };
        if y < top {
            top - y
        } else if y > bottom {
            y - bottom
        } else {
            0.0
        }
    }

    fn hit_test(&self, x: f32, y: f32, exact: bool) -> Option<TextRegionHit> {
        let point = kurbo::Point::new(f64::from(x), f64::from(y));
        let span = if exact {
            self.spans.iter().find(|span| span.rect.contains(point))?
        } else {
            self.spans.iter().min_by(|left, right| {
                rect_distance_squared(left.rect, point)
                    .total_cmp(&rect_distance_squared(right.rect, point))
            })?
        };
        let vertical = span.rect.height().abs() > span.rect.width().abs() * 1.5;
        let after_middle = if vertical {
            point.y >= span.rect.center().y
        } else {
            point.x >= span.rect.center().x
        };
        let byte_index = if after_middle {
            span.byte_range.end
        } else {
            span.byte_range.start
        };
        Some(TextRegionHit {
            byte_index,
            cluster_start: span.byte_range.start,
            cluster_end: span.byte_range.end,
        })
    }

    fn selection_fragment(&self, byte_range: Range<usize>) -> Option<PageSelectionFragment> {
        let visible = self.visible_byte_range()?;
        let start = floor_char_boundary(
            &self.text,
            byte_range.start.clamp(visible.start, visible.end),
        );
        let end = floor_char_boundary(&self.text, byte_range.end.clamp(visible.start, visible.end));
        if end <= start {
            return None;
        }
        Some(PageSelectionFragment {
            range: self.source_range_for_bytes(start..end)?,
            quote: self.text.get(start..end)?.to_owned(),
            rects: self.selection_rects(start..end),
        })
    }

    fn selection_rects(&self, byte_range: Range<usize>) -> Vec<Rect> {
        let rects = self
            .spans
            .iter()
            .filter(|span| {
                span.byte_range.start < byte_range.end && span.byte_range.end > byte_range.start
            })
            .map(|span| span.rect)
            .collect::<Vec<_>>();
        merge_fixed_text_rects(rects)
    }

    fn source_range_for_bytes(&self, byte_range: Range<usize>) -> Option<SourceRange> {
        if self.source.start.spine != self.source.end.spine
            || self.source.start.node != self.source.end.node
        {
            return None;
        }
        let start = self.source.start.text_offset
            + u64::try_from(self.text.get(..byte_range.start)?.chars().count()).ok()?;
        let end = self.source.start.text_offset
            + u64::try_from(self.text.get(..byte_range.end)?.chars().count()).ok()?;
        Some(SourceRange {
            start: SourceAnchor {
                spine: self.source.start.spine.clone(),
                node: self.source.start.node.clone(),
                text_offset: start,
            },
            end: SourceAnchor {
                spine: self.source.start.spine.clone(),
                node: self.source.start.node.clone(),
                text_offset: end,
            },
        })
    }

    fn byte_range_for_source(&self, range: &SourceRange) -> Option<Range<usize>> {
        if self.source.start.spine != self.source.end.spine
            || self.source.start.node != self.source.end.node
            || range.start.spine != range.end.spine
            || range.start.node != range.end.node
            || self.source.start.spine != range.start.spine
            || self.source.start.node != range.start.node
        {
            return None;
        }
        let start_offset = range
            .start
            .text_offset
            .max(self.source.start.text_offset)
            .min(self.source.end.text_offset);
        let end_offset = range
            .end
            .text_offset
            .max(self.source.start.text_offset)
            .min(self.source.end.text_offset);
        if end_offset <= start_offset {
            return None;
        }
        let start_chars = usize::try_from(start_offset - self.source.start.text_offset).ok()?;
        let end_chars = usize::try_from(end_offset - self.source.start.text_offset).ok()?;
        let start = byte_index_for_char_offset(&self.text, start_chars);
        let end = byte_index_for_char_offset(&self.text, end_chars);
        (end > start).then_some(start..end)
    }

    fn contains_source_anchor(&self, anchor: &SourceAnchor) -> bool {
        source_range_contains(&self.source, anchor)
    }
}

fn rect_distance_squared(rect: Rect, point: kurbo::Point) -> f64 {
    let dx = if point.x < rect.x0 {
        rect.x0 - point.x
    } else if point.x > rect.x1 {
        point.x - rect.x1
    } else {
        0.0
    };
    let dy = if point.y < rect.y0 {
        rect.y0 - point.y
    } else if point.y > rect.y1 {
        point.y - rect.y1
    } else {
        0.0
    };
    dx.mul_add(dx, dy * dy)
}

fn merge_fixed_text_rects(rects: Vec<Rect>) -> Vec<Rect> {
    let mut merged: Vec<Rect> = Vec::new();
    for rect in rects {
        if let Some(previous) = merged.last_mut() {
            let same_line = (previous.center().y - rect.center().y).abs()
                <= previous.height().abs().max(rect.height().abs()) * 0.55;
            let gap = rect.x0 - previous.x1;
            let merge_gap = previous.height().abs().max(rect.height().abs()) * 0.45;
            if same_line && gap <= merge_gap && rect.x1 >= previous.x0 {
                *previous = previous.union(rect);
                continue;
            }
        }
        merged.push(rect);
    }
    merged
}

fn byte_index_for_char_offset(text: &str, offset: usize) -> usize {
    text.char_indices()
        .nth(offset)
        .map_or(text.len(), |(index, _)| index)
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn source_range_contains(range: &SourceRange, anchor: &SourceAnchor) -> bool {
    range.start.spine == anchor.spine
        && range.start.node == anchor.node
        && anchor.text_offset >= range.start.text_offset
        && (anchor.text_offset < range.end.text_offset
            || (range.start.text_offset == range.end.text_offset
                && anchor.text_offset == range.start.text_offset))
}
