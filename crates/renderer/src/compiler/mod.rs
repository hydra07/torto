enum DisplayCommand {
    Glyphs(GlyphCommand),
    Image(ImageCommand),
    FillRect(FillRectCommand),
    FillRoundedRect(FillRoundedRectCommand),
    Rule(RuleCommand),
}

impl DisplayCommand {
    fn paints_below_source_overlays(&self) -> bool {
        matches!(
            self,
            Self::Image(_) | Self::FillRect(_) | Self::FillRoundedRect(_)
        )
    }

    fn paint(&self, scene: &mut impl PaintScene, page_transform: Affine) {
        match self {
            Self::Glyphs(command) => scene.draw_glyphs(
                &command.font,
                command.font_size,
                true,
                &command.normalized_coords,
                command.embolden,
                Fill::NonZero,
                command.color,
                1.0,
                page_transform * command.transform,
                command.glyph_transform,
                command.glyphs.iter().copied(),
            ),
            Self::Image(command) => {
                scene.draw_image(command.image.as_ref(), page_transform * command.transform);
            }
            Self::FillRect(command) => scene.fill(
                Fill::NonZero,
                page_transform,
                command.color,
                None,
                &command.rect,
            ),
            Self::FillRoundedRect(command) => scene.fill(
                Fill::NonZero,
                page_transform,
                command.color,
                None,
                &command.rect,
            ),
            Self::Rule(command) => scene.stroke(
                &Stroke::new(command.width),
                page_transform,
                command.color,
                None,
                &Line::new(command.start, command.end),
            ),
        }
    }
}

struct GlyphCommand {
    font: FontData,
    font_size: f32,
    normalized_coords: Arc<[NormalizedCoord]>,
    embolden: Vec2,
    color: Color,
    transform: Affine,
    glyph_transform: Option<Affine>,
    glyphs: Arc<[Glyph]>,
}

struct ImageCommand {
    image: ImageBrush,
    transform: Affine,
    bounds: Rect,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
    interactive: bool,
    source: Option<SourceRange>,
}

struct FillRectCommand {
    rect: Rect,
    color: Color,
}

struct FillRoundedRectCommand {
    rect: RoundedRect,
    color: Color,
}

struct RuleCommand {
    start: (f64, f64),
    end: (f64, f64),
    width: f64,
    color: Color,
}

/// Stateless compiler from layout IR to retained paint commands.
#[derive(Debug, Default)]
pub struct DisplayListCompiler;

impl DisplayListCompiler {
    pub fn compile(&self, page: &PageLayout) -> PageDisplayList {
        let content_top = page.items.iter().filter_map(page_item_top).reduce(f32::min);
        let content_bottom = page
            .items
            .iter()
            .filter_map(page_item_bottom)
            .reduce(f32::max);
        let mut commands = Vec::new();
        let mut text_regions = Vec::new();
        let mut inline_content_regions = Vec::new();
        let mut table_regions = Vec::new();
        let mut quote_regions = Vec::new();
        let mut footnote_regions = Vec::new();
        for item in &page.items {
            match item {
                PageItem::Text(text) => {
                    if let Some(region) = text_region(text) {
                        text_regions.push(region);
                    }
                    compile_text_commands(
                        &mut commands,
                        &mut inline_content_regions,
                        &mut footnote_regions,
                        text,
                    );
                }
                PageItem::Quote(quote) => {
                    let bounds = quote_bounds(quote);
                    quote_regions.push(QuoteRegion {
                        bounds,
                        sources: quote.sources.clone(),
                        continued_before: quote.continued_before,
                        continued_after: quote.continued_after,
                        accent_x: quote.x + 6.0,
                        accent_width: 4.0,
                        accent: color(quote.accent),
                    });
                    if quote.fill.alpha > 0 {
                        commands.push(DisplayCommand::FillRoundedRect(FillRoundedRectCommand {
                            rect: RoundedRect::from_rect(bounds, 7.0),
                            color: color(quote.fill),
                        }));
                    }
                    let accent_inset = 8.0_f64.min(bounds.height() * 0.2);
                    let accent_top = if quote.continued_before {
                        bounds.y0
                    } else {
                        bounds.y0 + accent_inset
                    };
                    let accent_bottom = if quote.continued_after {
                        bounds.y1
                    } else {
                        bounds.y1 - accent_inset
                    };
                    commands.push(DisplayCommand::FillRoundedRect(FillRoundedRectCommand {
                        rect: RoundedRect::from_rect(
                            Rect::new(bounds.x0 + 6.0, accent_top, bounds.x0 + 10.0, accent_bottom),
                            2.0,
                        ),
                        color: color(quote.accent),
                    }));
                }
                PageItem::Table(table) => {
                    if let Some(region) = table_region(table) {
                        table_regions.push(region);
                    }
                    compile_table_commands(
                        &mut commands,
                        &mut text_regions,
                        &mut inline_content_regions,
                        &mut footnote_regions,
                        table,
                    );
                }
                PageItem::Image(image) => {
                    let data = ImageData {
                        data: Blob::new(Arc::new(image.image.pixels.clone())),
                        format: ImageFormat::Rgba8,
                        alpha_type: ImageAlphaType::Alpha,
                        width: image.image.width,
                        height: image.image.height,
                    };
                    let transform = Affine::translate((f64::from(image.x), f64::from(image.y)))
                        * Affine::scale_non_uniform(
                            f64::from(image.width) / f64::from(image.image.width.max(1)),
                            f64::from(image.height) / f64::from(image.image.height.max(1)),
                        );
                    commands.push(DisplayCommand::Image(ImageCommand {
                        image: ImageBrush::new(data),
                        transform,
                        bounds: Rect::new(
                            f64::from(image.x),
                            f64::from(image.y),
                            f64::from(image.x + image.width),
                            f64::from(image.y + image.height),
                        ),
                        width: image.image.width,
                        height: image.image.height,
                        pixels: Arc::clone(&image.image.pixels),
                        interactive: true,
                        source: image.source.clone(),
                    }));
                    if let Some(replacement) = &image.replacement {
                        for segment in &replacement.segments {
                            commands.push(DisplayCommand::FillRect(FillRectCommand {
                                rect: Rect::new(
                                    f64::from(segment.rect.x),
                                    f64::from(segment.rect.y),
                                    f64::from(segment.rect.x + segment.rect.width),
                                    f64::from(segment.rect.y + segment.rect.height),
                                ),
                                color: fixed_page_mask_color(image, segment.rect),
                            }));
                            if let Some(region) = text_region(&segment.text) {
                                text_regions.push(region);
                            }
                            compile_text_commands(
                                &mut commands,
                                &mut inline_content_regions,
                                &mut footnote_regions,
                                &segment.text,
                            );
                        }
                    } else if let Some(region) = fixed_text_region(image) {
                        text_regions.push(region);
                    }
                }
                PageItem::Separator(separator) => {
                    commands.push(DisplayCommand::Rule(RuleCommand {
                        start: (f64::from(separator.x), f64::from(separator.y)),
                        end: (
                            f64::from(separator.x + separator.width),
                            f64::from(separator.y),
                        ),
                        width: 1.0,
                        color: Color::from_rgba8(120, 116, 108, 160),
                    }));
                }
            }
        }

        PageDisplayList {
            width: page.viewport.width,
            height: page.viewport.height,
            content_top,
            content_bottom,
            leading_gap: page.leading_gap.max(0.0),
            background: color(page.background),
            commands,
            text_regions,
            inline_content_regions,
            table_regions,
            quote_regions,
            footnote_regions,
        }
    }
}

fn quote_bounds(quote: &QuotePlacement) -> Rect {
    Rect::new(
        f64::from(quote.x),
        f64::from(quote.y),
        f64::from(quote.x + quote.width),
        f64::from(quote.y + quote.height),
    )
}

fn table_region(table: &TablePlacement) -> Option<TableRegion> {
    let bounds = table
        .cells
        .iter()
        .map(|cell| {
            Rect::new(
                f64::from(cell.x),
                f64::from(cell.y),
                f64::from(cell.x + cell.width),
                f64::from(cell.y + cell.height),
            )
        })
        .reduce(|current, next| current.union(next))?;
    let sources = table
        .cells
        .iter()
        .filter_map(|cell| cell.text.as_ref()?.source.clone())
        .collect();
    Some(TableRegion { bounds, sources })
}

fn compile_table_commands(
    commands: &mut Vec<DisplayCommand>,
    text_regions: &mut Vec<TextRegion>,
    inline_content_regions: &mut Vec<InlineContentRegion>,
    footnote_regions: &mut Vec<FootnoteRegion>,
    table: &TablePlacement,
) {
    for cell in &table.cells {
        if cell.header {
            commands.push(DisplayCommand::FillRect(FillRectCommand {
                rect: Rect::new(
                    f64::from(cell.x),
                    f64::from(cell.y),
                    f64::from(cell.x + cell.width),
                    f64::from(cell.y + cell.height),
                ),
                color: color(table.header_fill),
            }));
        }
    }
    for cell in &table.cells {
        if let Some(text) = &cell.text {
            if let Some(region) = text_region(text) {
                text_regions.push(region);
            }
            compile_text_commands(commands, inline_content_regions, footnote_regions, text);
        }
    }
    for cell in &table.cells {
        let left = f64::from(cell.x);
        let top = f64::from(cell.y);
        let right = f64::from(cell.x + cell.width);
        let bottom = f64::from(cell.y + cell.height);
        let border = color(table.border);
        for (start, end) in [
            ((left, top), (right, top)),
            ((right, top), (right, bottom)),
            ((right, bottom), (left, bottom)),
            ((left, bottom), (left, top)),
        ] {
            commands.push(DisplayCommand::Rule(RuleCommand {
                start,
                end,
                width: 1.0,
                color: border,
            }));
        }
    }
}

fn page_item_top(item: &PageItem) -> Option<f32> {
    match item {
        PageItem::Text(text) => text
            .layout
            .get(text.lines.start)
            .map(|line| text.origin_y + line.metrics().block_min_coord),
        PageItem::Quote(quote) => (!quote.continued_before).then_some(quote.y),
        PageItem::Image(image) => Some(image.y),
        PageItem::Table(table) => Some(table.y),
        PageItem::Separator(separator) => Some(separator.y),
    }
}

fn page_item_bottom(item: &PageItem) -> Option<f32> {
    match item {
        PageItem::Text(text) => text
            .lines
            .end
            .checked_sub(1)
            .and_then(|line| text.layout.get(line))
            .map(|line| text.origin_y + line.metrics().block_max_coord),
        PageItem::Quote(quote) => (!quote.continued_after).then_some(quote.y + quote.height),
        PageItem::Image(image) => Some(image.y + image.height),
        PageItem::Table(table) => Some(table.y + table.height),
        PageItem::Separator(separator) => Some(separator.y + 1.0),
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "fixed-page raster coordinates are clamped to bounded image dimensions before indexing"
)]
fn fixed_page_mask_color(
    image: &ImagePlacement,
    rect: rebook_publication::FixedPageTextRect,
) -> Color {
    let raster_width = image.image.width as usize;
    let raster_height = image.image.height as usize;
    if raster_width == 0
        || raster_height == 0
        || image.width <= 0.0
        || image.height <= 0.0
        || image.image.pixels.len() < raster_width.saturating_mul(raster_height).saturating_mul(4)
    {
        return Color::from_rgba8(255, 255, 255, 255);
    }
    let to_x = |x: f32| {
        (((x - image.x) / image.width) * image.image.width as f32)
            .floor()
            .clamp(0.0, (raster_width - 1) as f32) as usize
    };
    let to_y = |y: f32| {
        (((y - image.y) / image.height) * image.image.height as f32)
            .floor()
            .clamp(0.0, (raster_height - 1) as f32) as usize
    };
    let x0 = to_x(rect.x);
    let x1 = to_x(rect.x + rect.width).max(x0);
    let y0 = to_y(rect.y);
    let y1 = to_y(rect.y + rect.height).max(y0);
    let mut samples = Vec::<[u8; 4]>::new();
    let sample = |samples: &mut Vec<[u8; 4]>, x: usize, y: usize| {
        let offset = (y * raster_width + x) * 4;
        samples.push([
            image.image.pixels[offset],
            image.image.pixels[offset + 1],
            image.image.pixels[offset + 2],
            image.image.pixels[offset + 3],
        ]);
    };
    let steps = 12_usize;
    for step in 0..=steps {
        let x = x0 + (x1 - x0) * step / steps;
        let y = y0 + (y1 - y0) * step / steps;
        sample(&mut samples, x, y0);
        sample(&mut samples, x, y1);
        sample(&mut samples, x0, y);
        sample(&mut samples, x1, y);
    }
    let median = |channel: usize| {
        let mut values = samples
            .iter()
            .map(|sample| sample[channel])
            .collect::<Vec<_>>();
        values.sort_unstable();
        values[values.len() / 2]
    };
    Color::from_rgba8(median(0), median(1), median(2), median(3))
}

fn text_region(text: &TextPlacement) -> Option<TextRegion> {
    Some(TextRegion::Shaped(ShapedTextRegion {
        layout: Arc::clone(&text.layout),
        text: Arc::clone(&text.text),
        source_text_start: text.source_text_start,
        lines: text.lines.clone(),
        origin_x: text.origin_x,
        origin_y: text.origin_y,
        available_width: text.available_width,
        source: text.source.clone()?,
    }))
}

fn fixed_text_region(image: &ImagePlacement) -> Option<TextRegion> {
    let layer = image.text_layer.as_ref()?;
    let source = image.source.clone()?;
    if layer.text.is_empty() || layer.spans.is_empty() || layer.width <= 0.0 || layer.height <= 0.0
    {
        return None;
    }
    let scale_x = f64::from(image.width / layer.width);
    let scale_y = f64::from(image.height / layer.height);
    let spans = layer
        .spans
        .iter()
        .filter_map(|span| {
            let start_chars = usize::try_from(span.char_range.start).ok()?;
            let end_chars = usize::try_from(span.char_range.end).ok()?;
            let byte_start = byte_index_for_char_offset(&layer.text, start_chars);
            let byte_end = byte_index_for_char_offset(&layer.text, end_chars);
            if byte_end <= byte_start {
                return None;
            }
            let rect = &span.rect;
            Some(FixedTextSpan {
                byte_range: byte_start..byte_end,
                rect: Rect::new(
                    f64::from(image.x) + f64::from(rect.x) * scale_x,
                    f64::from(image.y) + f64::from(rect.y) * scale_y,
                    f64::from(image.x) + f64::from(rect.x + rect.width) * scale_x,
                    f64::from(image.y) + f64::from(rect.y + rect.height) * scale_y,
                ),
            })
        })
        .collect::<Vec<_>>();
    (!spans.is_empty()).then(|| {
        TextRegion::Fixed(FixedTextRegion {
            text: Arc::from(layer.text.as_str()),
            spans: spans.into(),
            source,
        })
    })
}

fn compile_text_commands(
    commands: &mut Vec<DisplayCommand>,
    inline_content_regions: &mut Vec<InlineContentRegion>,
    footnote_regions: &mut Vec<FootnoteRegion>,
    text: &TextPlacement,
) {
    let transform = Affine::translate((f64::from(text.origin_x), f64::from(text.origin_y)));
    let mut compiled_footnote_groups = Vec::<u32>::new();
    for line in text
        .layout
        .lines()
        .skip(text.lines.start)
        .take(text.lines.len())
    {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                let PositionedLayoutItem::InlineBox(inline_box) = item else {
                    continue;
                };
                let Some(image) = text
                    .inline_images
                    .iter()
                    .find(|image| image.id == inline_box.id)
                else {
                    continue;
                };
                let x = text.origin_x + inline_box.x;
                let y = text.origin_y + inline_box.y + image.offset_y;
                let image_transform = Affine::translate((f64::from(x), f64::from(y)))
                    * Affine::scale_non_uniform(
                        f64::from(image.width) / f64::from(image.image.width.max(1)),
                        f64::from(image.height) / f64::from(image.image.height.max(1)),
                    );
                let data = ImageData {
                    data: Blob::new(Arc::new(image.image.pixels.clone())),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width: image.image.width,
                    height: image.image.height,
                };
                commands.push(DisplayCommand::Image(ImageCommand {
                    image: ImageBrush::new(data),
                    transform: image_transform,
                    bounds: Rect::new(
                        f64::from(x),
                        f64::from(y),
                        f64::from(x + image.width),
                        f64::from(y + image.height),
                    ),
                    width: image.image.width,
                    height: image.image.height,
                    pixels: Arc::clone(&image.image.pixels),
                    interactive: false,
                    source: None,
                }));
                if let Some(source) = &text.source {
                    inline_content_regions.push(InlineContentRegion {
                        bounds: Rect::new(
                            f64::from(x),
                            f64::from(y),
                            f64::from(x + image.width),
                            f64::from(y + image.height),
                        ),
                        source: source.clone(),
                    });
                }
                continue;
            };
            let run = glyph_run.run();
            let brush = glyph_run.style().brush;
            let baseline_offset = match brush.baseline {
                TextBaseline::Normal => 0.0,
                TextBaseline::Superscript => -run.font_size() * 0.35,
                TextBaseline::Subscript => run.font_size() * 0.2,
            };
            if brush.footnote_reference {
                if compiled_footnote_groups.contains(&brush.footnote_reference_group) {
                    continue;
                }
                if let Some(source) = text.source.clone() {
                    let center_x = text.origin_x + glyph_run.offset() + glyph_run.advance() / 2.0;
                    let bounds = footnote_icon_bounds(
                        center_x,
                        text.origin_y + glyph_run.baseline(),
                        run.font_size(),
                    );
                    footnote_regions.push(FootnoteRegion { bounds, source });
                    compiled_footnote_groups.push(brush.footnote_reference_group);
                }
                continue;
            }
            let synthesis = run.synthesis();
            let embolden = synthetic_embolden(synthesis.embolden(), run.font_size());
            let glyph_transform = synthesis
                .skew()
                .map(|angle| Affine::skew(f64::from(angle.to_radians().tan()), 0.0));
            let glyphs = glyph_run
                .positioned_glyphs()
                .map(|glyph| Glyph {
                    id: glyph.id,
                    x: glyph.x,
                    y: glyph.y + baseline_offset,
                })
                .collect::<Vec<_>>()
                .into();
            commands.push(DisplayCommand::Glyphs(GlyphCommand {
                font: run.font().clone(),
                font_size: run.font_size(),
                normalized_coords: run.normalized_coords().to_vec().into(),
                embolden,
                color: color(brush.color),
                transform,
                glyph_transform,
                glyphs,
            }));

            if brush.underline {
                let metrics = run.metrics();
                let y = f64::from(
                    glyph_run.baseline() + baseline_offset - metrics.underline_offset
                        + metrics.underline_size / 2.0,
                ) + f64::from(text.origin_y);
                let x = f64::from(glyph_run.offset() + text.origin_x);
                commands.push(DisplayCommand::Rule(RuleCommand {
                    start: (x, y),
                    end: (x + f64::from(glyph_run.advance()), y),
                    width: f64::from(metrics.underline_size.max(1.0)),
                    color: color(brush.color),
                }));
            }
        }
    }
}

fn synthetic_embolden(enabled: bool, font_size: f32) -> Vec2 {
    if !enabled || !font_size.is_finite() || font_size <= 0.0 {
        return Vec2::ZERO;
    }
    let amount = f64::from(font_size) * SYNTHETIC_EMBOLDEN_EM;
    Vec2::new(amount, amount)
}

fn color(value: Rgba) -> Color {
    Color::from_rgba8(value.red, value.green, value.blue, value.alpha)
}
