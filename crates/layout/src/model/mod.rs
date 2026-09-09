#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextBrush {
    pub color: Rgba,
    pub underline: bool,
    pub baseline: TextBaseline,
    pub footnote_reference: bool,
    /// Stable identifier for all glyph runs produced by one semantic footnote marker.
    ///
    /// Font fallback can split a marker such as `【3】` into several glyph runs. The
    /// renderer uses this identifier to collapse those runs back into one icon.
    pub footnote_reference_group: u32,
}

impl TextBrush {
    fn new(
        color: Rgba,
        underline: bool,
        baseline: TextBaseline,
        footnote_reference_group: u32,
    ) -> Self {
        Self {
            color,
            underline,
            baseline,
            footnote_reference: footnote_reference_group != 0,
            footnote_reference_group,
        }
    }
}

/// Shared font bytes registered in both the native reader and the Xilem UI.
pub type ReaderFontBlob = parley::fontique::Blob<u8>;

/// One immutable paginated section.
pub struct SectionLayout {
    pub pages: Vec<PageLayout>,
    pub visible_pages: usize,
    pub continuation_offset_x: f32,
}

/// Renderer-independent display data for one page.
pub struct PageLayout {
    pub viewport: LayoutViewport,
    pub background: Rgba,
    /// Semantic spacing that preceded the first block before pagination moved
    /// it onto this page. Paginated views discard it at the physical page edge,
    /// while continuous views can restore it when stitching pages together.
    pub leading_gap: f32,
    pub items: Vec<PageItem>,
}

/// Positioned page content.
pub enum PageItem {
    Text(TextPlacement),
    Quote(QuotePlacement),
    Table(TablePlacement),
    Image(ImagePlacement),
    Separator(SeparatorPlacement),
}

/// Unified-typesetting decoration for one page slice of a semantic quotation.
pub struct QuotePlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub continued_before: bool,
    pub continued_after: bool,
    pub fill: Rgba,
    pub accent: Rgba,
    pub sources: Vec<SourceRange>,
}

/// A line slice from a shaped paragraph.
#[derive(Clone)]
pub struct TextPlacement {
    pub layout: Arc<Layout<TextBrush>>,
    /// UTF-8 text shaped by Parley. Kept alongside the layout so retained
    /// renderers can map pointer hit tests back to durable source offsets.
    pub text: Arc<str>,
    /// Byte length of synthetic display text (for example a list marker) that
    /// precedes the authored source text.
    pub source_text_start: usize,
    pub lines: Range<usize>,
    pub origin_x: f32,
    pub origin_y: f32,
    /// Full horizontal measure available to this shaped block.
    pub available_width: f32,
    pub source: Option<SourceRange>,
    /// Formula rasters positioned by Parley inline boxes in this text layout.
    pub inline_images: Arc<[InlineImage]>,
}

/// One positioned table chunk. Large tables can produce one chunk per page.
pub struct TablePlacement {
    pub cells: Vec<TableCellPlacement>,
    pub y: f32,
    pub height: f32,
    pub border: Rgba,
    pub header_fill: Rgba,
}

/// One positioned table cell with selectable text content.
pub struct TableCellPlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub header: bool,
    pub text: Option<TextPlacement>,
}

/// One raster painted at a matching Parley inline-box position.
#[derive(Clone)]
pub struct InlineImage {
    pub id: u64,
    pub image: RasterImage,
    pub width: f32,
    pub height: f32,
    /// Paint offset relative to Parley's baseline-aligned inline box.
    pub offset_y: f32,
}

/// Decoded RGBA image ready for upload by the renderer.
#[derive(Clone)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>,
}

/// Positioned raster image.
pub struct ImagePlacement {
    pub image: RasterImage,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub source: Option<SourceRange>,
    pub text_layer: Option<FixedPageTextLayer>,
    pub replacement: Option<FixedPageTextReplacementPlacement>,
}

/// Translated text repainted inside the original fixed-layout page image.
pub struct FixedPageTextReplacementPlacement {
    pub segments: Vec<FixedPageTextReplacementSegmentPlacement>,
}

/// One shaped translated fragment inside a fixed-page replacement overlay.
pub struct FixedPageTextReplacementSegmentPlacement {
    pub rect: FixedPageTextRect,
    pub text: TextPlacement,
}

/// Positioned thematic break.
pub struct SeparatorPlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
}

/// Stateful layout engine. Font discovery and shaping caches live for the reader session.
pub struct LayoutEngine {
    font_context: FontContext,
    layout_context: LayoutContext<TextBrush>,
    svg_options: resvg::usvg::Options<'static>,
    publication_languages: Vec<String>,
}
