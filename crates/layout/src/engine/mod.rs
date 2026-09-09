fn should_layout_flow_block(block: &Block, reader_style: &ReaderStyle) -> bool {
    !reader_style.focus_footnote_icons || !block.is_footnote_definition()
}

fn collect_layout_blocks<'a>(
    blocks: &'a [Block],
    reader_style: &ReaderStyle,
    output: &mut Vec<&'a Block>,
) {
    for block in blocks {
        if let Block::Note(note) = block {
            let hidden = match note.kind {
                NoteBlockKind::Definition => reader_style.focus_footnote_icons,
                NoteBlockKind::Section => reader_style.typesetting.mode == TypesettingMode::Unified,
            };
            if !hidden {
                collect_layout_blocks(&note.blocks, reader_style, output);
            }
            continue;
        }
        if should_layout_flow_block(block, reader_style) {
            output.push(block);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReaderFontFamilies {
    pub all: Vec<String>,
    pub serif: Vec<String>,
    pub sans_serif: Vec<String>,
    pub other: Vec<String>,
    pub monospace: Vec<String>,
    pub chinese: Vec<String>,
}

impl ReaderFontFamilies {
    pub fn include_configured(&mut self, typography: &ReaderTypography) {
        include_available_family(&self.all, &mut self.serif, &typography.serif_font);
        include_available_family(&self.all, &mut self.sans_serif, &typography.sans_serif_font);
        include_available_family(&self.all, &mut self.other, &typography.other_font);
        include_available_family(&self.all, &mut self.monospace, &typography.monospace_font);
        if let Some(choice) = &typography.cjk_default_font {
            let category = match choice.category {
                ReaderDefaultFont::Serif => &mut self.serif,
                ReaderDefaultFont::SansSerif => &mut self.sans_serif,
                ReaderDefaultFont::Other => &mut self.other,
            };
            include_available_family(&self.all, category, &choice.family);
        }
        if let Some(family) = &typography.latin_cjk_font {
            include_available_family(&self.all, &mut self.chinese, family);
        }
    }

    /// Replaces persisted reader families that the native renderer cannot
    /// safely paint with the matching bundled default (or a validated fallback).
    pub fn repair_typography(&self, typography: &mut ReaderTypography) -> bool {
        typography.normalize();
        let defaults = ReaderTypography::default();
        let mut repaired = repair_available_family(
            &self.chinese,
            &mut typography.default_cjk_font,
            &defaults.default_cjk_font,
        ) | repair_available_family(
            &self.serif,
            &mut typography.serif_font,
            &defaults.serif_font,
        ) | repair_available_family(
            &self.sans_serif,
            &mut typography.sans_serif_font,
            &defaults.sans_serif_font,
        ) | repair_optional_family(&self.other, &mut typography.other_font)
            | repair_available_family(
                &self.monospace,
                &mut typography.monospace_font,
                &defaults.monospace_font,
            );
        if typography.default_font == ReaderDefaultFont::Other && typography.other_font.is_empty() {
            typography.default_font = ReaderDefaultFont::Serif;
            repaired = true;
        }
        let cjk_default_repaired = typography.cjk_default_font.as_mut().is_some_and(|choice| {
            let available = match choice.category {
                ReaderDefaultFont::Serif => &self.serif,
                ReaderDefaultFont::SansSerif => &self.sans_serif,
                ReaderDefaultFont::Other => &self.other,
            };
            repair_optional_family(available, &mut choice.family)
        });
        if typography
            .cjk_default_font
            .as_ref()
            .is_some_and(|choice| choice.family.is_empty())
        {
            typography.cjk_default_font = None;
        }
        let latin_cjk_repaired =
            repair_optional_family_option(&self.chinese, &mut typography.latin_cjk_font);
        repaired | cjk_default_repaired | latin_cjk_repaired
    }
}

fn repair_optional_family(available: &[String], current: &mut String) -> bool {
    let Some(matching) = available
        .iter()
        .find(|family| family.eq_ignore_ascii_case(current))
    else {
        let repaired = !current.is_empty();
        current.clear();
        return repaired;
    };
    if matching == current {
        false
    } else {
        current.clone_from(matching);
        true
    }
}

fn repair_optional_family_option(available: &[String], current: &mut Option<String>) -> bool {
    let Some(family) = current else {
        return false;
    };
    let repaired = repair_optional_family(available, family);
    if family.is_empty() {
        *current = None;
    }
    repaired
}

fn repair_available_family(available: &[String], current: &mut String, default: &str) -> bool {
    let replacement = available
        .iter()
        .find(|family| family.eq_ignore_ascii_case(current))
        .or_else(|| {
            available
                .iter()
                .find(|family| family.eq_ignore_ascii_case(default))
        })
        .or_else(|| available.first());
    let Some(replacement) = replacement else {
        return false;
    };
    if replacement == current {
        return false;
    }
    current.clone_from(replacement);
    true
}

fn include_available_family(all: &[String], category: &mut Vec<String>, family: &str) {
    let Some(available) = all
        .iter()
        .find(|available| available.eq_ignore_ascii_case(family))
    else {
        return;
    };
    if !category
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(available))
    {
        category.push(available.clone());
        category.sort_by_key(|family| family.to_lowercase());
    }
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    pub fn new() -> Self {
        let mut svg_options = resvg::usvg::Options::default();
        svg_options.fontdb_mut().load_system_fonts();
        Self {
            font_context: FontContext::new(),
            layout_context: LayoutContext::new(),
            svg_options,
            publication_languages: Vec::new(),
        }
    }

    pub fn with_fonts(fonts: impl IntoIterator<Item = ReaderFontBlob>) -> Self {
        let mut engine = Self::new();
        for font in fonts {
            engine.font_context.collection.register_fonts(font, None);
        }
        engine
    }

    pub fn available_font_families(&mut self) -> Vec<String> {
        let mut families = self
            .font_context
            .collection
            .family_names()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        families.sort_by_key(|family| family.to_lowercase());
        families.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        families
    }

    pub fn available_reader_font_families(&mut self) -> ReaderFontFamilies {
        let discovered = self.available_font_families();
        let mut families = ReaderFontFamilies::default();
        for family_name in &discovered {
            let font_info = self
                .font_context
                .collection
                .family_by_name(family_name)
                .and_then(|family| family.default_font().cloned());
            let Some(font_info) = font_info else {
                continue;
            };
            let Some(data) = font_info.load(None) else {
                continue;
            };
            let charmap = font_info.charmap_index().charmap(data.as_ref());
            let supports_chinese = charmap
                .as_ref()
                .is_some_and(|charmap| supports_common_chinese(charmap));
            let supports_latin = charmap
                .as_ref()
                .is_some_and(|charmap| supports_common_latin(charmap));
            let Ok(font) = FontRef::from_index(data.as_ref(), font_info.index()) else {
                continue;
            };
            // Vello 0.10 prefers a matching embedded bitmap strike over the
            // outline. Several Windows fonts (notably SimSun/宋体) expose EBDT
            // masks that Vello cannot paint and does not fall back from,
            // leaving matching glyphs blank. Keep such families out of every
            // reader selector until the renderer supports that bitmap format.
            if has_embedded_bitmap_glyphs(&font) {
                continue;
            }
            families.all.push(family_name.clone());
            if supports_chinese {
                families.chinese.push(family_name.clone());
            }
            let fixed_pitch = font
                .post()
                .ok()
                .is_some_and(|post| post.is_fixed_pitch() != 0);
            let os2 = font.os2().ok();
            let panose = os2.as_ref().map(|os2| os2.panose_10());
            let family_class = os2.as_ref().map(|os2| os2.s_family_class());
            let mut classification = classify_reader_font(panose, family_class, fixed_pitch);
            if !classification.serif && !classification.sans_serif && !classification.monospace {
                classification = infer_reader_font_classification(family_name);
            }
            if classification.monospace {
                families.monospace.push(family_name.clone());
            } else if classification.serif {
                families.serif.push(family_name.clone());
            } else if classification.sans_serif {
                families.sans_serif.push(family_name.clone());
            } else if supports_latin
                && !supports_chinese
                && !is_symbolic_reader_font(family_name, panose, family_class)
            {
                families.other.push(family_name.clone());
            }
        }
        families
    }

    pub fn layout_section(
        &mut self,
        source: &dyn BookSource,
        section: &Section,
        viewport: LayoutViewport,
        reader_style: &ReaderStyle,
    ) -> Result<SectionLayout, LayoutError> {
        self.layout_blocks(source, &section.blocks, viewport, reader_style)
    }

    /// Lays out one viewport-independent slice of a reflowable section. The
    /// reader uses this entry point for bounded fragment compilation without
    /// manufacturing synthetic authored sections.
    pub fn layout_blocks(
        &mut self,
        source: &dyn BookSource,
        blocks: &[Block],
        viewport: LayoutViewport,
        reader_style: &ReaderStyle,
    ) -> Result<SectionLayout, LayoutError> {
        self.layout_fragments(source, &[blocks], viewport, reader_style)
    }

    /// Builds a fixed-page layout with the exact geometry of the eventual
    /// raster while retaining only a single white pixel. This lets continuous
    /// PDF views reserve every physical page without decoding all page images.
    #[allow(
        clippy::cast_precision_loss,
        reason = "fixed page dimensions are bounded by the PDF raster budget"
    )]
    pub fn layout_fixed_page_placeholder(
        &mut self,
        dimensions: FixedPageDimensions,
        viewport: LayoutViewport,
        reader_style: &ReaderStyle,
    ) -> SectionLayout {
        let page_width = viewport.width as f32;
        let page_height = viewport.height as f32;
        let geometry = resolve_page_geometry(page_width, page_height, reader_style);
        let visible_pages = geometry.visible_pages;
        let continuation_offset_x = geometry.continuation_offset_x;
        let mut paginator = Paginator::new(
            viewport,
            reader_style.background,
            geometry,
            true,
            reader_style.minimum_paragraph_gap,
        );
        paginator.push_image(
            RasterImage {
                width: dimensions.width.max(1),
                height: dimensions.height.max(1),
                pixels: Arc::from([255_u8, 255, 255, 255]),
            },
            ImageStyle::default(),
            None,
            None,
        );
        let mut pages = paginator.finish();
        for page in &mut pages {
            for item in &mut page.items {
                if let PageItem::Image(image) = item {
                    image.image = RasterImage {
                        width: 1,
                        height: 1,
                        pixels: Arc::from([255_u8, 255, 255, 255]),
                    };
                }
            }
        }
        SectionLayout {
            pages,
            visible_pages,
            continuation_offset_x,
        }
    }

    /// Continuously paginates several stable content fragments as one bounded
    /// layout segment. Fragment boundaries do not commit the partial page; the
    /// caller controls random-access cost by choosing the segment size.
    #[allow(
        clippy::cast_precision_loss,
        reason = "reader viewport dimensions are bounded far below f32's exact integer range"
    )]
    pub fn layout_fragments(
        &mut self,
        source: &dyn BookSource,
        fragments: &[&[Block]],
        viewport: LayoutViewport,
        reader_style: &ReaderStyle,
    ) -> Result<SectionLayout, LayoutError> {
        self.publication_languages
            .clone_from(&source.book().metadata.languages);
        let page_width = viewport.width as f32;
        let page_height = viewport.height as f32;
        let geometry = resolve_page_geometry(page_width, page_height, reader_style);
        let content_width = geometry.width;
        let visible_pages = geometry.visible_pages;
        let continuation_offset_x = geometry.continuation_offset_x;

        let center_standalone_image = source.book().metadata.layout
            == RenditionLayout::PrePaginated
            || fragments_are_standalone_cover(fragments, source.book().cover.as_ref());
        let unified_reflow = reader_style.typesetting.mode == TypesettingMode::Unified
            && source.book().metadata.layout != RenditionLayout::PrePaginated;
        let media_start_offset = if unified_reflow {
            0.0
        } else {
            dominant_paragraph_start_offset(fragments, content_width)
        };
        let mut paginator = Paginator::new(
            viewport,
            reader_style.background,
            geometry,
            center_standalone_image,
            reader_style.minimum_paragraph_gap,
        );
        paginator.media_start_offset = media_start_offset;

        let mut layout_blocks = Vec::new();
        for blocks in fragments {
            collect_layout_blocks(blocks, reader_style, &mut layout_blocks);
        }
        let mut block_index = 0;
        while block_index < layout_blocks.len() {
            if unified_reflow
                && let (Some(Block::Image(image)), Some(Block::Text(caption))) = (
                    layout_blocks.get(block_index).copied(),
                    layout_blocks.get(block_index + 1).copied(),
                )
                && caption.kind == TextBlockKind::Caption
            {
                self.push_figure(
                    &mut paginator,
                    source,
                    std::slice::from_ref(image),
                    std::slice::from_ref(caption),
                    CaptionPosition::After,
                    BlockStyle::default(),
                    reader_style,
                    content_width,
                    media_start_offset,
                    true,
                )?;
                block_index += 2;
                continue;
            }

            if unified_reflow
                && let Some(Block::Text(ordinal)) = layout_blocks.get(block_index).copied()
                && let TextBlockKind::HeadingOrdinal(level) = ordinal.kind
                && let Some(Block::Text(title)) = layout_blocks.get(block_index + 1).copied()
                && title.kind.heading_level() == Some(level)
            {
                let ordinal = resolve_text_block(ordinal, reader_style, TextContext::Flow);
                let title = resolve_text_block(title, reader_style, TextContext::Flow);
                let ordinal_text = self.shape_text(&ordinal, reader_style, content_width);
                let title_text = self.shape_text(&title, reader_style, content_width);
                paginator.keep_together_if_fits(
                    prepared_flow_height(&ordinal_text)
                        + ordinal.style.margin_before.max(0.0)
                        + ordinal.style.margin_after.max(0.0)
                        + prepared_flow_height(&title_text)
                        + title.style.margin_before.max(0.0)
                        + title.style.margin_after.max(0.0),
                );
            }

            let block = layout_blocks[block_index];
            match block {
                Block::Text(block) => {
                    let resolved = resolve_text_block(block, reader_style, TextContext::Flow);
                    let prepared = self.shape_text_from_source(
                        source,
                        &resolved,
                        reader_style,
                        content_width,
                    )?;
                    paginator.push_text(&prepared, &resolved)?;
                }
                Block::Quote(quote) => {
                    let quote_horizontal_padding = if unified_reflow {
                        reader_style.typography.font_size
                            * paragraph_indent_em(
                                &reader_style.typesetting,
                                reader_style.writing_system,
                            )
                    } else {
                        0.0
                    };
                    let quote_width = if unified_reflow {
                        (content_width - quote_horizontal_padding * 2.0).max(40.0)
                    } else {
                        content_width
                    };
                    let mut prepared_body = Vec::with_capacity(quote.body.len());
                    for (index, body) in quote.body.iter().enumerate() {
                        let mut resolved =
                            resolve_text_block(body, reader_style, TextContext::Flow);
                        if unified_reflow
                            && quote.attribution.is_none()
                            && index + 1 == quote.body.len()
                        {
                            // The card already owns its bottom padding. Keeping the unified
                            // paragraph gap after the final body paragraph would make a quote
                            // without an attribution visibly bottom-heavy.
                            resolved.to_mut().style.margin_after = 0.0;
                        }
                        let mut prepared = self.shape_text(&resolved, reader_style, quote_width);
                        if unified_reflow {
                            prepared.start_offset += quote_horizontal_padding;
                        }
                        prepared_body.push((prepared, resolved));
                    }
                    let prepared_attribution = quote.attribution.as_ref().map(|attribution| {
                        let resolved =
                            resolve_text_block(attribution, reader_style, TextContext::Flow);
                        let mut prepared = self.shape_text(&resolved, reader_style, quote_width);
                        if unified_reflow {
                            prepared.start_offset += quote_horizontal_padding;
                        }
                        (prepared, resolved)
                    });
                    if unified_reflow {
                        let content_height = prepared_body
                            .iter()
                            .map(|(prepared, resolved)| {
                                prepared_flow_height(prepared)
                                    + resolved.style.margin_before.max(0.0)
                                    + resolved.style.margin_after.max(0.0)
                            })
                            .chain(prepared_attribution.iter().map(|(prepared, resolved)| {
                                prepared_flow_height(prepared)
                                    + resolved.style.margin_before.max(0.0)
                                    + resolved.style.margin_after.max(0.0)
                            }))
                            .sum::<f32>();
                        // Keep a short quote card intact. The leading outer
                        // gap and both internal paddings must fit before its
                        // trailing attribution; the final outer gap may flow
                        // naturally after the completed card.
                        paginator
                            .keep_together_if_fits(content_height + QUOTE_VERTICAL_PADDING * 3.0);
                        let sources = quote
                            .body
                            .iter()
                            .filter_map(|block| block.source.clone())
                            .chain(
                                quote
                                    .attribution
                                    .iter()
                                    .filter_map(|block| block.source.clone()),
                            )
                            .collect();
                        let outer_gap = (reader_style.typography.font_size
                            * reader_style.typesetting.paragraph_gap_em)
                            .max(QUOTE_VERTICAL_PADDING);
                        paginator.begin_quote(sources, reader_style.foreground, outer_gap);
                    }
                    for (prepared, resolved) in &prepared_body {
                        paginator.push_text(prepared, resolved.as_ref())?;
                    }
                    if let Some((prepared, resolved)) = &prepared_attribution {
                        paginator.push_text(prepared, resolved.as_ref())?;
                    }
                    if unified_reflow {
                        paginator.end_quote();
                    }
                }
                Block::Table(table) => {
                    let prepared = self.shape_table(
                        table,
                        reader_style,
                        (content_width - media_start_offset).max(1.0),
                    );
                    paginator.push_table(&prepared);
                }
                Block::Image(image) => {
                    let raster = load_raster_image(source, image)?;
                    let mut image_style = image.style;
                    if unified_reflow {
                        let gap = reader_style.typography.font_size
                            * reader_style.typesetting.media_gap_em;
                        image_style.margin_before = gap;
                        image_style.margin_after = gap;
                    }
                    let replacements = paginator.push_image(
                        raster,
                        image_style,
                        image.source.clone(),
                        image.text_layer.clone(),
                    );
                    for replacement in replacements {
                        let prepared =
                            self.shape_fixed_page_replacement(&replacement, reader_style);
                        paginator.push_fixed_page_replacement(&prepared, replacement)?;
                    }
                }
                Block::Figure(figure) => self.push_figure(
                    &mut paginator,
                    source,
                    &figure.images,
                    &figure.captions,
                    figure.caption_position,
                    figure.style,
                    reader_style,
                    content_width,
                    media_start_offset,
                    unified_reflow,
                )?,
                Block::Separator(separator) => {
                    if !unified_reflow || separator.in_quote {
                        match separator.kind {
                            SeparatorKind::Symbols => {
                                if let Some(text) = &separator.text {
                                    let prepared = self.shape_text_from_source(
                                        source,
                                        text,
                                        reader_style,
                                        content_width,
                                    )?;
                                    paginator.push_text(&prepared, text)?;
                                }
                            }
                            SeparatorKind::Spacing => paginator.ensure_minimum_spacing(
                                separator.style.margin_before.max(0.0)
                                    + reader_style.typography.font_size
                                        * separator.style.line_height.max(1.0)
                                    + separator.style.margin_after.max(0.0),
                            ),
                            SeparatorKind::Rule => paginator.push_separator(),
                            SeparatorKind::Ornament => {
                                if let Some(image) = &separator.image {
                                    let raster = load_raster_image(source, image)?;
                                    let replacements = paginator.push_image(
                                        raster,
                                        image.style,
                                        image.source.clone(),
                                        image.text_layer.clone(),
                                    );
                                    for replacement in replacements {
                                        let prepared = self.shape_fixed_page_replacement(
                                            &replacement,
                                            reader_style,
                                        );
                                        paginator
                                            .push_fixed_page_replacement(&prepared, replacement)?;
                                    }
                                }
                            }
                        }
                    }
                }
                Block::LineBreak => paginator.ensure_minimum_spacing(
                    reader_style.typography.font_size
                        * if unified_reflow {
                            unified_body_line_height(
                                reader_style.writing_system,
                                &reader_style.typesetting,
                            )
                        } else {
                            reader_style.typesetting.body_line_height
                        },
                ),
                Block::PageBreak => paginator.force_page(),
                Block::Note(_) => unreachable!("note blocks are flattened before layout"),
            }
            block_index += 1;
        }

        Ok(SectionLayout {
            pages: paginator.finish(),
            visible_pages,
            continuation_offset_x,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn push_figure(
        &mut self,
        paginator: &mut Paginator,
        source: &dyn BookSource,
        figure_images: &[ImageBlock],
        figure_captions: &[TextBlock],
        caption_position: CaptionPosition,
        figure_style: BlockStyle,
        reader_style: &ReaderStyle,
        content_width: f32,
        media_start_offset: f32,
        unified_reflow: bool,
    ) -> Result<(), LayoutError> {
        let authored_outer_gap = figure_images
            .iter()
            .map(|image| image.style.margin_before.max(image.style.margin_after))
            .fold(0.0, f32::max)
            .max(figure_style.margin_before)
            .max(figure_style.margin_after);
        let mut images = Vec::with_capacity(figure_images.len());
        for image in figure_images {
            let mut style = image.style;
            // The figure owns its outer spacing. Keeping authored margins on each
            // child would double the gap between an image and its caption.
            style.margin_before = 0.0;
            style.margin_after = 0.0;
            images.push((load_raster_image(source, image)?, style, image));
        }
        let captions = figure_captions
            .iter()
            .map(|caption| {
                self.shape_figure_caption(
                    caption,
                    reader_style,
                    (content_width - media_start_offset).max(1.0),
                    unified_reflow,
                )
            })
            .collect::<Vec<_>>();
        let outer_gap = if unified_reflow {
            reader_style.typography.font_size * reader_style.typesetting.media_gap_em
        } else {
            authored_outer_gap.max(IMAGE_BLOCK_GAP)
        };
        let caption_gap = if unified_reflow {
            reader_style.typography.font_size * reader_style.typesetting.caption_gap_em
        } else {
            6.0
        };
        let image_height = images
            .iter()
            .map(|(raster, style, _)| paginator.image_display_size(raster, *style).1)
            .sum::<f32>();
        let caption_height = captions
            .iter()
            .map(|(prepared, resolved)| {
                prepared_text_height(prepared)
                    + resolved.style.margin_before.max(0.0)
                    + resolved.style.margin_after.max(0.0)
            })
            .sum::<f32>();
        let internal_image_gaps = caption_gap * images.len().saturating_sub(1) as f32;
        let image_caption_gap = if images.is_empty() || captions.is_empty() {
            0.0
        } else {
            caption_gap
        };
        paginator.prepare_group(
            image_height + caption_height + internal_image_gaps + image_caption_gap,
            outer_gap,
        );

        let push_images = |paginator: &mut Paginator,
                           engine: &mut LayoutEngine|
         -> Result<(), LayoutError> {
            for (index, (raster, style, image)) in images.iter().enumerate() {
                if index > 0 {
                    paginator.add_semantic_spacing(caption_gap);
                }
                let replacements = paginator.push_image_with_gaps(
                    raster.clone(),
                    *style,
                    image.source.clone(),
                    image.text_layer.clone(),
                    0.0,
                    0.0,
                );
                for replacement in replacements {
                    let prepared = engine.shape_fixed_page_replacement(&replacement, reader_style);
                    paginator.push_fixed_page_replacement(&prepared, replacement)?;
                }
            }
            Ok(())
        };
        let push_captions = |paginator: &mut Paginator| -> Result<(), LayoutError> {
            for (prepared, resolved) in &captions {
                paginator.push_text(prepared, resolved.as_ref())?;
            }
            Ok(())
        };
        match caption_position {
            CaptionPosition::Before => {
                push_captions(paginator)?;
                if !captions.is_empty() && !images.is_empty() {
                    paginator.add_semantic_spacing(caption_gap);
                }
                push_images(paginator, self)?;
            }
            CaptionPosition::After => {
                push_images(paginator, self)?;
                if !captions.is_empty() && !images.is_empty() {
                    paginator.add_semantic_spacing(caption_gap);
                }
                push_captions(paginator)?;
            }
        }
        paginator.ensure_minimum_spacing(outer_gap);
        Ok(())
    }

    fn shape_text(
        &mut self,
        block: &TextBlock,
        reader_style: &ReaderStyle,
        content_width: f32,
    ) -> PreparedText {
        self.shape_text_with_min_width(block, reader_style, content_width, 40.0)
    }

    fn shape_text_from_source(
        &mut self,
        source: &dyn BookSource,
        block: &TextBlock,
        reader_style: &ReaderStyle,
        content_width: f32,
    ) -> Result<PreparedText, LayoutError> {
        let mut rasters = Vec::with_capacity(block.content.len());
        for inline in &block.content {
            rasters.push(match inline {
                Inline::Image(run) => Some(load_raster_image(source, &run.image)?),
                Inline::Text(_) | Inline::Math(_) | Inline::Break => None,
            });
        }
        Ok(self.shape_text_with_min_width_and_rasters(
            block,
            reader_style,
            content_width,
            40.0,
            &rasters,
        ))
    }

    fn shape_figure_caption<'a>(
        &mut self,
        caption: &'a TextBlock,
        reader_style: &ReaderStyle,
        content_width: f32,
        unified_reflow: bool,
    ) -> (PreparedText, Cow<'a, TextBlock>) {
        let mut resolved = resolve_text_block(caption, reader_style, TextContext::Flow);
        let mut prepared = self.shape_text(&resolved, reader_style, content_width);
        if unified_reflow && prepared.layout.len() > 1 {
            resolved.to_mut().style.align = TextAlignment::Start;
            prepared = self.shape_text(&resolved, reader_style, content_width);
        }
        (prepared, resolved)
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "table spans are clamped to 64 and publication rows are bounded by input limits"
    )]
    fn shape_table(
        &mut self,
        table: &TableBlock,
        reader_style: &ReaderStyle,
        content_width: f32,
    ) -> PreparedTable {
        let row_count = table.rows.len();
        let mut occupied = vec![Vec::<bool>::new(); row_count];
        let mut grid_cells = Vec::new();
        let mut column_count = 0;
        for (row_index, row) in table.rows.iter().enumerate() {
            let mut column = 0;
            for cell in &row.cells {
                while occupied[row_index].get(column).copied().unwrap_or(false) {
                    column += 1;
                }
                let column_span = usize::from(cell.column_span.max(1));
                let row_span = usize::from(cell.row_span.max(1)).min(row_count - row_index);
                let end_column = column.saturating_add(column_span);
                for row in occupied.iter_mut().skip(row_index).take(row_span) {
                    row.resize(row.len().max(end_column), false);
                    row[column..end_column].fill(true);
                }
                column_count = column_count.max(end_column);
                grid_cells.push((row_index, row_span, column, column_span, cell));
                column = end_column;
            }
        }
        if column_count == 0 || row_count == 0 {
            return PreparedTable::default();
        }
        let table_metrics = resolve_table_metrics(reader_style);
        let unified = reader_style.typesetting.mode == TypesettingMode::Unified;
        let equal_column_width = content_width / column_count as f32;
        let column_widths = if unified {
            self.adaptive_table_column_widths(
                &grid_cells,
                column_count,
                content_width,
                reader_style,
                table_metrics,
            )
        } else {
            vec![equal_column_width; column_count]
        };
        let minimum_row_height = (reader_style.typography.font_size * table_metrics.font_scale)
            .mul_add(table_metrics.line_height, table_metrics.cell_padding * 2.0);
        let mut row_heights = vec![minimum_row_height; row_count];
        let mut cells = Vec::with_capacity(grid_cells.len());
        for (row, row_span, column, column_span, cell) in grid_cells {
            let block = table_cell_text_block(cell);
            let block = resolve_text_block(&block, reader_style, TextContext::Table).into_owned();
            let cell_width = column_widths[column..column + column_span]
                .iter()
                .sum::<f32>();
            let text_width = (cell_width - table_metrics.cell_padding * 2.0).max(20.0);
            let text = self.shape_text_with_min_width(&block, reader_style, text_width, 8.0);
            let required_height = prepared_text_height(&text) + table_metrics.cell_padding * 2.0;
            if row_span == 1 {
                row_heights[row] = row_heights[row].max(required_height);
            }
            cells.push(PreparedTableCell {
                row,
                row_span,
                column,
                column_span,
                header: cell.header,
                source: block.source.clone(),
                text,
                required_height,
            });
        }
        for cell in cells.iter().filter(|cell| cell.row_span > 1) {
            let current = row_heights[cell.row..cell.row + cell.row_span]
                .iter()
                .sum::<f32>();
            if cell.required_height > current {
                let addition = (cell.required_height - current) / cell.row_span as f32;
                for height in &mut row_heights[cell.row..cell.row + cell.row_span] {
                    *height += addition;
                }
            }
        }
        PreparedTable {
            horizontal_offset: centered_table_offset(unified, content_width, &column_widths),
            column_widths,
            row_heights,
            cells,
            cell_padding: table_metrics.cell_padding,
            center_content: unified,
            block_gap: table_metrics.block_gap,
            border: Rgba {
                alpha: 96,
                ..reader_style.foreground
            },
            header_fill: Rgba {
                alpha: 22,
                ..reader_style.foreground
            },
        }
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "table spans are clamped to 64 and publication rows are bounded by input limits"
    )]
    fn adaptive_table_column_widths(
        &mut self,
        grid_cells: &[(usize, usize, usize, usize, &TableCell)],
        column_count: usize,
        content_width: f32,
        reader_style: &ReaderStyle,
        table_metrics: ResolvedTableMetrics,
    ) -> Vec<f32> {
        let equal_column_width = content_width / column_count as f32;
        let minimum_column_width = (reader_style.typography.font_size * 3.0)
            .min(equal_column_width)
            .max(1.0);
        let mut preferred_widths = vec![minimum_column_width; column_count];
        for (_, _, column, column_span, cell) in grid_cells {
            let block = table_cell_text_block(cell);
            let block = resolve_text_block(&block, reader_style, TextContext::Table).into_owned();
            let unwrapped = self.shape_text_with_min_width(&block, reader_style, 16_384.0, 8.0);
            let inline_slack = reader_style.typography.font_size * table_metrics.font_scale * 0.5;
            let preferred = (unwrapped.layout.full_width().ceil()
                + table_metrics.cell_padding * 2.0
                + inline_slack)
                .clamp(minimum_column_width, content_width);
            let range = *column..(*column + *column_span);
            let current = preferred_widths[range.clone()].iter().sum::<f32>();
            if preferred > current {
                let addition = (preferred - current) / *column_span as f32;
                for width in &mut preferred_widths[range] {
                    *width += addition;
                }
            }
        }
        fit_adaptive_column_widths(&preferred_widths, minimum_column_width, content_width)
    }

    fn shape_text_with_min_width(
        &mut self,
        block: &TextBlock,
        reader_style: &ReaderStyle,
        content_width: f32,
        minimum_width: f32,
    ) -> PreparedText {
        self.shape_text_with_min_width_and_rasters(
            block,
            reader_style,
            content_width,
            minimum_width,
            &[],
        )
    }

    #[allow(clippy::too_many_lines)]
    fn shape_text_with_min_width_and_rasters(
        &mut self,
        block: &TextBlock,
        reader_style: &ReaderStyle,
        content_width: f32,
        minimum_width: f32,
        inline_rasters: &[Option<RasterImage>],
    ) -> PreparedText {
        let (start_offset, available_width, first_line_indent) =
            resolve_text_measure(block, content_width, minimum_width);
        let typography = &reader_style.typography;
        let (text, spans, inline_images, source_text_start) = prepare_inline_content(
            block,
            reader_style.foreground,
            typography,
            available_width,
            &self.svg_options,
            reader_style.focus_footnote_icons,
            inline_rasters,
        );
        let font_stack = if block.kind == TextBlockKind::Preformatted {
            typography.monospace_stack()
        } else {
            typography.default_stack_for(reader_style.writing_system)
        };
        let mut layout = self.build_text_layout(
            &text,
            &spans,
            &inline_images,
            &font_stack,
            typography,
            block.style.line_height,
            reader_style.foreground,
            &[],
        );
        self.apply_text_indents(
            &mut layout,
            block,
            &text[..source_text_start],
            typography,
            &font_stack,
            first_line_indent,
        );
        let should_optimize = reader_style.typesetting.line_break_strategy
            == LineBreakStrategy::Optimized
            && matches!(
                block.style.align,
                TextAlignment::Start | TextAlignment::Justify
            )
            && matches!(
                block.kind,
                TextBlockKind::Paragraph
                    | TextBlockKind::Blockquote
                    | TextBlockKind::Caption
                    | TextBlockKind::ListItem { .. }
                    | TextBlockKind::DefinitionDescription { .. }
            );
        let candidate_hyphens = if should_optimize {
            self.prepare_hyphen_candidates(
                &text,
                &spans,
                &inline_images,
                &font_stack,
                typography,
                block.style.line_height,
                reader_style.foreground,
            )
        } else {
            HashMap::new()
        };
        let hyphen_widths = candidate_hyphens
            .iter()
            .map(|(offset, glyph)| (*offset, glyph.width))
            .collect::<HashMap<_, _>>();
        let mut selected_hyphens = Vec::new();
        let continuation_indent = if matches!(block.kind, TextBlockKind::ListItem { .. }) {
            self.measure_list_marker_width(&text[..source_text_start], &font_stack, typography)
        } else {
            0.0
        };
        let mut optimized = should_optimize
            && linebreak::parley::plan_optimized_with_hanging_indent(
                &mut layout,
                &text,
                available_width,
                first_line_indent,
                continuation_indent,
                typography.font_size,
                &hyphen_widths,
            )
            .and_then(|plan| {
                let mut adjusted = self.build_text_layout(
                    &text,
                    &spans,
                    &inline_images,
                    &font_stack,
                    typography,
                    block.style.line_height,
                    reader_style.foreground,
                    &plan.adjustments,
                );
                self.apply_text_indents(
                    &mut adjusted,
                    block,
                    &text[..source_text_start],
                    typography,
                    &font_stack,
                    first_line_indent,
                );
                linebreak::parley::apply_breaks(&mut adjusted, &plan.lines, available_width)?;
                selected_hyphens = plan
                    .hyphen_offsets
                    .iter()
                    .enumerate()
                    .filter_map(|(line_index, offset)| {
                        let glyph = candidate_hyphens.get(offset.as_ref()?)?.clone();
                        Some(PreparedHyphen { line_index, glyph })
                    })
                    .collect();
                layout = adjusted;
                Some(())
            })
            .is_some();
        if !optimized {
            layout.break_all_lines(Some(available_width));
            if block.style.align == TextAlignment::Justify
                && let Some(plan) = linebreak::parley::plan_wrapped_justification(
                    &mut layout,
                    &text,
                    available_width,
                )
            {
                let mut adjusted = self.build_text_layout(
                    &text,
                    &spans,
                    &inline_images,
                    &font_stack,
                    typography,
                    block.style.line_height,
                    reader_style.foreground,
                    &plan.adjustments,
                );
                self.apply_text_indents(
                    &mut adjusted,
                    block,
                    &text[..source_text_start],
                    typography,
                    &font_stack,
                    first_line_indent,
                );
                if linebreak::parley::apply_breaks(&mut adjusted, &plan.lines, available_width)
                    .is_some()
                {
                    layout = adjusted;
                    optimized = true;
                }
            }
        }
        let alignment = if optimized {
            Alignment::Start
        } else {
            text_alignment(block.style.align)
        };
        layout.align(alignment, AlignmentOptions::default());
        PreparedText {
            layout: Arc::new(layout),
            text: text.into(),
            source_text_start,
            start_offset,
            available_width,
            inline_images: inline_images
                .into_iter()
                .map(|image| InlineImage {
                    id: image.id,
                    image: image.image,
                    width: image.width,
                    height: image.height,
                    offset_y: image.offset_y,
                })
                .collect::<Vec<_>>()
                .into(),
            hyphens: selected_hyphens.into(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_hyphen_candidates(
        &mut self,
        text: &str,
        spans: &[StyledRange],
        inline_images: &[PreparedInlineImage],
        font_stack: &str,
        typography: &ReaderTypography,
        line_height: f32,
        foreground: Rgba,
    ) -> HashMap<usize, PreparedHyphenGlyph> {
        let hyphenation_spans = spans
            .iter()
            .filter(|span| !span.range.is_empty())
            .map(|span| linebreak::hyphenation::HyphenationSpan {
                range: span.range.clone(),
                language: span.style.language,
                mode: span.style.hyphenation,
                suppress: span.hyphenation_suppressed,
            })
            .collect::<Vec<_>>();
        let blockers = inline_images
            .iter()
            .map(|image| image.index)
            .collect::<Vec<_>>();
        let mut opportunities = linebreak::hyphenation::break_opportunities(
            text,
            &hyphenation_spans,
            &self.publication_languages,
            &blockers,
        )
        .into_iter()
        .collect::<Vec<_>>();
        opportunities.sort_unstable();

        let hyphen_text: Arc<str> = Arc::from("\u{2010}");
        let mut style_cache = Vec::<(TextStyle, PreparedHyphenGlyph)>::new();
        let mut prepared = HashMap::new();
        for offset in opportunities {
            let Some(style) = spans
                .iter()
                .find(|span| {
                    span.range.start < offset
                        && offset <= span.range.end
                        && !span.hyphenation_suppressed
                })
                .map(|span| span.style)
            else {
                continue;
            };
            if let Some((_, glyph)) = style_cache.iter().find(|(cached, _)| *cached == style) {
                prepared.insert(offset, glyph.clone());
                continue;
            }
            let hyphen_span = StyledRange {
                range: 0..hyphen_text.len(),
                style,
                footnote_reference_group: 0,
                hyphenation_suppressed: true,
            };
            let mut layout = self.build_text_layout(
                &hyphen_text,
                std::slice::from_ref(&hyphen_span),
                &[],
                font_stack,
                typography,
                line_height,
                foreground,
                &[],
            );
            layout.break_all_lines(None);
            let Some(line) = layout.get(0) else {
                continue;
            };
            let width = positioned_line_content_end(line);
            if !width.is_finite() || width <= 0.0 {
                continue;
            }
            let glyph = PreparedHyphenGlyph {
                layout: Arc::new(layout),
                text: Arc::clone(&hyphen_text),
                width,
            };
            style_cache.push((style, glyph.clone()));
            prepared.insert(offset, glyph);
        }
        prepared
    }

    #[allow(clippy::too_many_arguments)]
    fn build_text_layout(
        &mut self,
        text: &str,
        spans: &[StyledRange],
        inline_images: &[PreparedInlineImage],
        font_stack: &str,
        typography: &ReaderTypography,
        line_height: f32,
        foreground: Rgba,
        spacing: &[linebreak::parley::SpacingAdjustment],
    ) -> Layout<TextBrush> {
        let mut builder =
            self.layout_context
                .ranged_builder(&mut self.font_context, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::from(font_stack)));
        builder.push_default(StyleProperty::FontSize(typography.font_size));
        let default_variations = optical_size_variations(typography.font_size);
        builder.push_default(StyleProperty::FontVariations(FontVariations::from(
            &default_variations,
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(
            typography.font_weight,
        ))));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            line_height,
        )));
        let default_brush = TextBrush::new(foreground, false, TextBaseline::Normal, 0);
        builder.push_default(StyleProperty::Brush(default_brush));

        for span in spans {
            let size = (typography.font_size * span.style.size_scale.clamp(0.5, 3.0))
                .max(typography.minimum_font_size);
            builder.push(StyleProperty::FontSize(size), span.range.clone());
            let variations = optical_size_variations(size);
            builder.push(
                StyleProperty::FontVariations(FontVariations::from(&variations)),
                span.range.clone(),
            );
            builder.push(
                StyleProperty::Brush(TextBrush::new(
                    span.style.color,
                    span.style.underline,
                    span.style.baseline,
                    span.footnote_reference_group,
                )),
                span.range.clone(),
            );
            if span.style.bold {
                builder.push(
                    StyleProperty::FontWeight(FontWeight::new(
                        f32::from(typography.font_weight).max(FontWeight::BOLD.value()),
                    )),
                    span.range.clone(),
                );
            }
            if span.style.italic {
                builder.push(
                    StyleProperty::FontStyle(FontStyle::Italic),
                    span.range.clone(),
                );
            }
            if span.style.underline {
                builder.push(StyleProperty::Underline(true), span.range.clone());
            }
        }

        for adjustment in spacing {
            builder.push(
                StyleProperty::LetterSpacing(adjustment.amount),
                adjustment.range.clone(),
            );
        }
        for image in inline_images {
            builder.push_inline_box(ParleyInlineBox {
                id: image.id,
                kind: InlineBoxKind::InFlow,
                index: image.index,
                width: image.width,
                height: image.box_height,
            });
        }
        builder.build(text)
    }

    fn apply_text_indents(
        &mut self,
        layout: &mut Layout<TextBrush>,
        block: &TextBlock,
        marker: &str,
        typography: &ReaderTypography,
        font_stack: &str,
        first_line_indent: f32,
    ) {
        if first_line_indent.abs() > f32::EPSILON {
            layout.set_text_indent(
                first_line_indent,
                IndentOptions {
                    each_line: block.style.subparagraph_gap_em.is_some(),
                    ..IndentOptions::default()
                },
            );
        }
        self.apply_list_indent(layout, block.kind, marker, typography, font_stack);
    }

    fn measure_list_marker_width(
        &mut self,
        marker: &str,
        font_stack: &str,
        typography: &ReaderTypography,
    ) -> f32 {
        let mut builder =
            self.layout_context
                .ranged_builder(&mut self.font_context, marker, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::from(font_stack)));
        builder.push_default(StyleProperty::FontSize(typography.font_size));
        let variations = optical_size_variations(typography.font_size);
        builder.push_default(StyleProperty::FontVariations(FontVariations::from(
            &variations,
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(
            typography.font_weight,
        ))));
        let mut layout = builder.build(marker);
        layout.break_all_lines(None);
        layout.full_width()
    }

    fn apply_list_indent(
        &mut self,
        layout: &mut Layout<TextBrush>,
        kind: TextBlockKind,
        marker: &str,
        typography: &ReaderTypography,
        font_stack: &str,
    ) {
        if marker.is_empty() {
            return;
        }
        let marker_width = self.measure_list_marker_width(marker, font_stack, typography);
        apply_list_hanging_indent(layout, kind, marker_width);
    }

    fn shape_fixed_page_replacement(
        &mut self,
        request: &FixedPageReplacementRequest,
        reader_style: &ReaderStyle,
    ) -> PreparedText {
        let block = fixed_page_replacement_block(&request.text, request.source.clone());
        let mut style = reader_style.clone();
        style.typography.font_size = style.typography.font_size.min(14.0);
        style.typography.minimum_font_size = 5.0;
        let available_width = (request.rect.width - 3.0).max(2.0);
        let available_height = (request.rect.height - 3.0).max(2.0);

        loop {
            let prepared = self.shape_text_with_min_width(&block, &style, available_width, 2.0);
            let height = prepared_text_height(&prepared);
            if height <= available_height || style.typography.font_size <= 5.0 {
                return prepared;
            }
            let next_size = (style.typography.font_size * available_height / height)
                .clamp(5.0, style.typography.font_size - 0.5);
            style.typography.font_size = next_size;
        }
    }
}
