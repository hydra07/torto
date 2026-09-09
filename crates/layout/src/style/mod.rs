const QUOTE_VERTICAL_PADDING: f32 = 12.0;
const LITERATA_FAMILY: &str = "Literata";
const LEGACY_YSABEAU_FAMILY: &str = "Ysabeau Office";
const OPTICAL_SIZE_TAG: Tag = Tag::new(b"opsz");
const MIN_OPTICAL_SIZE: f32 = 7.0;
const MAX_OPTICAL_SIZE: f32 = 72.0;
const CSS_PX_TO_POINTS: f32 = 0.75;

/// Shared light-theme accent used by semantic quote decorations and block activation fills.
pub const LIGHT_QUOTE_ACCENT_COLOR: Rgba = Rgba {
    red: 0xDC,
    green: 0xE2,
    blue: 0xE8,
    alpha: 255,
};

/// Dark-theme quote accent keeps light reader text legible on an active quote block.
pub const DARK_QUOTE_ACCENT_COLOR: Rgba = Rgba {
    red: 0x43,
    green: 0x48,
    blue: 0x4E,
    alpha: 255,
};

/// Returns the quote accent that matches the active reader theme.
#[must_use]
pub const fn quote_accent_color(dark: bool) -> Rgba {
    if dark {
        DARK_QUOTE_ACCENT_COLOR
    } else {
        LIGHT_QUOTE_ACCENT_COLOR
    }
}

fn quote_accent_for_foreground(foreground: Rgba) -> Rgba {
    // Reader foregrounds are dark on light pages and light on dark pages.
    // Integer Rec. 709 weights avoid float comparisons in this hot layout path.
    let luminance = u32::from(foreground.red) * 54
        + u32::from(foreground.green) * 183
        + u32::from(foreground.blue) * 19;
    quote_accent_color(luminance > 128 * 256)
}

const DEFAULT_COLUMN_GAP: f32 = 36.0;
const IMAGE_BLOCK_GAP: f32 = 14.0;
const TABLE_BLOCK_GAP: f32 = 14.0;
const MIN_COLUMN_WIDTH: f32 = 360.0;
const MAX_COLUMN_WIDTH: f32 = 800.0;
const DEFAULT_TOP_MARGIN: f32 = 0.0;
const DEFAULT_BOTTOM_MARGIN: f32 = 24.0;

/// Logical viewport in device-independent pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutViewport {
    pub width: u32,
    pub height: u32,
}

impl LayoutViewport {
    pub fn new(width: u32, height: u32) -> Result<Self, LayoutError> {
        if width == 0 || height == 0 {
            return Err(LayoutError::InvalidViewport);
        }
        Ok(Self { width, height })
    }
}

/// User-controlled values that invalidate pagination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReaderStyle {
    pub typography: ReaderTypography,
    pub typesetting: ReaderTypesetting,
    /// Publication-wide writing system used by automatic typography defaults.
    pub writing_system: WritingSystem,
    pub horizontal_margin: f32,
    pub top_margin: f32,
    pub bottom_margin: f32,
    pub column_gap: f32,
    /// Minimum visual gap between consecutive prose paragraphs. A zero value
    /// preserves the publication-authored margins exactly.
    pub minimum_paragraph_gap: f32,
    pub spread: SpreadMode,
    /// Replaces linked superscript markers with semantic footnote icon slots.
    pub focus_footnote_icons: bool,
    pub foreground: Rgba,
    pub background: Rgba,
}

/// Chooses whether reflowable content follows publication-authored metrics or
/// the reader's semantic, cross-book typesetting profile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TypesettingMode {
    #[default]
    Book,
    Unified,
}

/// Selects the paragraph breakpoint algorithm while retaining Parley for
/// shaping, line construction, alignment, and justification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineBreakStrategy {
    #[default]
    Greedy,
    Optimized,
}

/// Chooses whether paragraph indentation follows the book language or an
/// explicit reader value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ParagraphIndentMode {
    #[default]
    Auto,
    Custom,
}

/// Reader-controlled metrics applied consistently to semantic reading blocks.
/// Relative values scale with the base reading font so one font-size change
/// keeps headings, prose, and tables in proportion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReaderTypesetting {
    pub mode: TypesettingMode,
    pub line_break_strategy: LineBreakStrategy,
    pub heading_scale: f32,
    pub body_line_height: f32,
    pub paragraph_indent_mode: ParagraphIndentMode,
    pub paragraph_indent_em: f32,
    pub paragraph_gap_em: f32,
    pub heading_body_gap_em: f32,
    pub media_gap_em: f32,
    pub caption_font_scale: f32,
    pub caption_gap_em: f32,
    pub list_indent_em: f32,
    pub table_font_scale: f32,
    pub table_line_height: f32,
    pub table_cell_padding_em: f32,
}

impl ReaderTypesetting {
    pub fn unified() -> Self {
        Self {
            mode: TypesettingMode::Unified,
            line_break_strategy: LineBreakStrategy::Optimized,
            ..Self::default()
        }
    }

    /// Repairs persisted values before they participate in layout cache keys.
    pub fn normalize(&mut self) {
        self.heading_scale = finite_clamp(self.heading_scale, 1.1, 2.2, 1.6);
        self.body_line_height = finite_clamp(self.body_line_height, 1.2, 2.4, 1.5);
        self.paragraph_indent_em = finite_clamp(self.paragraph_indent_em, 0.0, 4.0, 2.0);
        self.paragraph_gap_em = finite_clamp(self.paragraph_gap_em, 0.0, 2.0, 0.5);
        self.heading_body_gap_em = finite_clamp(self.heading_body_gap_em, 0.2, 2.0, 0.7);
        self.media_gap_em = finite_clamp(self.media_gap_em, 0.5, 2.0, 1.0);
        self.caption_font_scale = finite_clamp(self.caption_font_scale, 0.7, 1.0, 0.88);
        self.caption_gap_em = finite_clamp(self.caption_gap_em, 0.2, 1.0, 0.35);
        self.list_indent_em = finite_clamp(self.list_indent_em, 0.5, 3.0, 1.5);
        self.table_font_scale = finite_clamp(self.table_font_scale, 0.7, 1.2, 0.9);
        self.table_line_height = finite_clamp(self.table_line_height, 1.1, 2.0, 1.45);
        self.table_cell_padding_em = finite_clamp(self.table_cell_padding_em, 0.2, 1.0, 0.35);
    }
}

impl Default for ReaderTypesetting {
    fn default() -> Self {
        Self {
            mode: TypesettingMode::Book,
            line_break_strategy: LineBreakStrategy::Greedy,
            heading_scale: 1.6,
            body_line_height: 1.5,
            paragraph_indent_mode: ParagraphIndentMode::Auto,
            paragraph_indent_em: 2.0,
            paragraph_gap_em: 0.5,
            heading_body_gap_em: 0.7,
            media_gap_em: 1.0,
            caption_font_scale: 0.88,
            caption_gap_em: 0.35,
            list_indent_em: 1.5,
            table_font_scale: 0.9,
            table_line_height: 1.45,
            table_cell_padding_em: 0.35,
        }
    }
}

/// Generic family used for ordinary reading text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReaderDefaultFont {
    #[default]
    Serif,
    SansSerif,
    Other,
}

/// One explicit Western font selection used by a language profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderFontChoice {
    pub category: ReaderDefaultFont,
    pub family: String,
}

/// Readest-compatible native typography preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReaderTypography {
    pub default_font: ReaderDefaultFont,
    pub default_cjk_font: String,
    pub serif_font: String,
    pub sans_serif_font: String,
    pub other_font: String,
    /// Western letters and digits used in CJK-primary books. `None` inherits
    /// the Latin profile's primary selection.
    pub cjk_default_font: Option<ReaderFontChoice>,
    /// CJK fallback used in Latin-primary books. `None` inherits the CJK
    /// profile's primary family.
    pub latin_cjk_font: Option<String>,
    pub monospace_font: String,
    pub font_size: f32,
    pub minimum_font_size: f32,
    pub font_weight: u16,
}

impl ReaderTypography {
    /// Repairs persisted or externally supplied settings before layout uses them.
    pub fn normalize(&mut self) {
        let defaults = Self::default();
        let configured_other = self.other_font.trim();
        let used_deprecated_primary = self.default_font == ReaderDefaultFont::Other
            && (configured_other.eq_ignore_ascii_case(LEGACY_YSABEAU_FAMILY)
                || configured_other.eq_ignore_ascii_case(LITERATA_FAMILY));
        if used_deprecated_primary {
            self.default_font = ReaderDefaultFont::Serif;
            self.serif_font = LITERATA_FAMILY.into();
            self.other_font.clear();
        }
        if let Some(choice) = &mut self.cjk_default_font
            && choice
                .family
                .trim()
                .eq_ignore_ascii_case(LEGACY_YSABEAU_FAMILY)
        {
            choice.category = ReaderDefaultFont::Serif;
            choice.family = LITERATA_FAMILY.into();
        } else if let Some(choice) = &mut self.cjk_default_font
            && choice.family.trim().eq_ignore_ascii_case(LITERATA_FAMILY)
        {
            choice.category = ReaderDefaultFont::Serif;
            choice.family = LITERATA_FAMILY.into();
        }
        normalize_family(&mut self.default_cjk_font, &defaults.default_cjk_font);
        if self.default_cjk_font.eq_ignore_ascii_case("LXGW WenKai") {
            self.default_cjk_font = "LXGW WenKai GB Screen".into();
        }
        normalize_family(&mut self.serif_font, &defaults.serif_font);
        normalize_family(&mut self.sans_serif_font, &defaults.sans_serif_font);
        self.other_font = self.other_font.trim().to_owned();
        if let Some(choice) = &mut self.cjk_default_font {
            choice.family = choice.family.trim().to_owned();
            if choice.family.is_empty() {
                self.cjk_default_font = None;
            }
        }
        if let Some(family) = &mut self.latin_cjk_font {
            *family = family.trim().to_owned();
            if family.is_empty() {
                self.latin_cjk_font = None;
            }
        }
        normalize_family(&mut self.monospace_font, &defaults.monospace_font);
        self.minimum_font_size = finite_clamp(self.minimum_font_size, 1.0, 120.0, 12.0);
        self.font_size = finite_clamp(self.font_size, self.minimum_font_size, 120.0, 20.0);
        self.font_weight = self.font_weight.clamp(200, 900);
    }

    #[must_use]
    pub fn default_stack(&self) -> String {
        self.default_stack_for(WritingSystem::Unknown)
    }

    #[must_use]
    pub fn default_stack_for(&self, writing_system: WritingSystem) -> String {
        match writing_system {
            WritingSystem::Cjk => {
                let (category, family) = self.cjk_default_font.as_ref().map_or_else(
                    || (self.default_font, self.default_western_family()),
                    |choice| (choice.category, choice.family.as_str()),
                );
                self.reading_stack(category, family, &self.default_cjk_font)
            }
            WritingSystem::Latin | WritingSystem::Other | WritingSystem::Unknown => self
                .reading_stack(
                    self.default_font,
                    self.default_western_family(),
                    self.latin_cjk_font
                        .as_deref()
                        .unwrap_or(&self.default_cjk_font),
                ),
        }
    }

    #[must_use]
    pub fn serif_stack(&self) -> String {
        self.reading_stack(
            ReaderDefaultFont::Serif,
            &self.serif_font,
            &self.default_cjk_font,
        )
    }

    fn default_western_family(&self) -> &str {
        match self.default_font {
            ReaderDefaultFont::Serif => &self.serif_font,
            ReaderDefaultFont::SansSerif => &self.sans_serif_font,
            ReaderDefaultFont::Other => &self.other_font,
        }
    }

    fn reading_stack(
        &self,
        category: ReaderDefaultFont,
        western_family: &str,
        cjk_family: &str,
    ) -> String {
        match category {
            ReaderDefaultFont::Serif => font_stack(
                [
                    western_family,
                    cjk_family,
                    "LXGW WenKai GB Screen",
                    "Noto Serif SC",
                    "Source Han Serif SC",
                    "Songti SC",
                    "SimSun",
                    "Georgia",
                    "Times New Roman",
                ],
                "serif",
            ),
            ReaderDefaultFont::SansSerif => font_stack(
                [
                    western_family,
                    cjk_family,
                    "LXGW WenKai GB Screen",
                    "Noto Sans SC",
                    "Source Han Sans SC",
                    "PingFang SC",
                    "Microsoft YaHei",
                    "Roboto",
                    "Arial",
                ],
                "sans-serif",
            ),
            ReaderDefaultFont::Other => font_stack(
                [
                    western_family,
                    cjk_family,
                    "LXGW WenKai GB Screen",
                    "Noto Sans SC",
                    "Source Han Sans SC",
                    "PingFang SC",
                    "Microsoft YaHei",
                    "Roboto",
                    "Arial",
                ],
                "sans-serif",
            ),
        }
    }

    #[must_use]
    pub fn sans_serif_stack(&self) -> String {
        self.reading_stack(
            ReaderDefaultFont::SansSerif,
            &self.sans_serif_font,
            &self.default_cjk_font,
        )
    }

    #[must_use]
    pub fn monospace_stack(&self) -> String {
        font_stack(
            [
                self.monospace_font.as_str(),
                self.default_cjk_font.as_str(),
                "LXGW WenKai GB Screen",
            ],
            "monospace",
        )
    }
}

impl Default for ReaderTypography {
    fn default() -> Self {
        Self {
            default_font: ReaderDefaultFont::Serif,
            default_cjk_font: "LXGW WenKai GB Screen".into(),
            serif_font: LITERATA_FAMILY.into(),
            sans_serif_font: "Arial".into(),
            other_font: String::new(),
            cjk_default_font: Some(ReaderFontChoice {
                category: ReaderDefaultFont::Serif,
                family: LITERATA_FAMILY.into(),
            }),
            latin_cjk_font: None,
            monospace_font: "Consolas".into(),
            font_size: 20.0,
            minimum_font_size: 12.0,
            font_weight: 400,
        }
    }
}

/// Maximum number of book pages shown in one viewport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpreadMode {
    /// Always paginate as one page per viewport.
    #[default]
    Single,
    /// Use a two-page spread when both columns can remain comfortably readable.
    Double,
    /// Paginate as single pages and present the active section as a vertical flow.
    Scroll,
}

impl SpreadMode {
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Single => Self::Double,
            Self::Double => Self::Scroll,
            Self::Scroll => Self::Single,
        }
    }
}

impl Default for ReaderStyle {
    fn default() -> Self {
        Self {
            typography: ReaderTypography::default(),
            typesetting: ReaderTypesetting::default(),
            writing_system: WritingSystem::Unknown,
            horizontal_margin: 44.0,
            top_margin: DEFAULT_TOP_MARGIN,
            bottom_margin: DEFAULT_BOTTOM_MARGIN,
            column_gap: DEFAULT_COLUMN_GAP,
            minimum_paragraph_gap: 0.0,
            spread: SpreadMode::Double,
            focus_footnote_icons: false,
            foreground: Rgba::BLACK,
            background: Rgba {
                red: 250,
                green: 248,
                blue: 243,
                alpha: 255,
            },
        }
    }
}

fn normalize_family(value: &mut String, fallback: &str) {
    *value = value.trim().to_owned();
    if value.is_empty() {
        value.push_str(fallback);
    }
}

fn finite_clamp(value: f32, minimum: f32, maximum: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback.clamp(minimum, maximum)
    }
}

fn optical_size_for_font(font_size: f32) -> f32 {
    (font_size * CSS_PX_TO_POINTS).clamp(MIN_OPTICAL_SIZE, MAX_OPTICAL_SIZE)
}

fn optical_size_variations(font_size: f32) -> [FontVariation; 1] {
    [FontVariation::new(
        OPTICAL_SIZE_TAG,
        optical_size_for_font(font_size),
    )]
}

fn font_stack<'a>(families: impl IntoIterator<Item = &'a str>, generic: &str) -> String {
    let mut seen = HashSet::new();
    let mut stack = families
        .into_iter()
        .map(str::trim)
        .filter(|family| !family.is_empty())
        .filter(|family| seen.insert(family.to_ascii_lowercase()))
        .map(quote_font_family)
        .collect::<Vec<_>>();
    stack.push(generic.to_owned());
    stack.join(", ")
}

fn quote_font_family(family: &str) -> String {
    format!("\"{}\"", family.replace('\\', "\\\\").replace('"', "\\\""))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ReaderFontClassification {
    serif: bool,
    sans_serif: bool,
    monospace: bool,
}

fn classify_reader_font(
    panose: Option<&[u8]>,
    family_class: Option<i16>,
    fixed_pitch: bool,
) -> ReaderFontClassification {
    let panose = panose.filter(|panose| panose.len() >= 4);
    let monospace = fixed_pitch || panose.is_some_and(|panose| panose[0] == 2 && panose[3] == 9);
    if monospace {
        return ReaderFontClassification {
            monospace: true,
            ..ReaderFontClassification::default()
        };
    }
    if let Some(panose) = panose.filter(|panose| panose[0] == 2) {
        let classification = ReaderFontClassification {
            serif: matches!(panose[1], 2..=10),
            sans_serif: matches!(panose[1], 11..=15),
            monospace: false,
        };
        if classification.serif || classification.sans_serif {
            return classification;
        }
    }
    let family_class = family_class.map(|value| value.to_be_bytes()[0]);
    ReaderFontClassification {
        serif: family_class.is_some_and(|class| matches!(class, 1..=5 | 7)),
        sans_serif: family_class == Some(8),
        monospace: false,
    }
}

fn infer_reader_font_classification(family: &str) -> ReaderFontClassification {
    let normalized = family.to_ascii_lowercase();
    let words = normalized
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let has_word = |candidate| words.contains(&candidate);
    let serif = has_word("serif")
        || has_word("roman")
        || has_word("antiqua")
        || has_word("mincho")
        || ["baskerville", "bodoni", "bookman", "literata", "sitka"]
            .iter()
            .any(|prefix| normalized.starts_with(prefix));
    let sans_serif = !serif
        && (has_word("sans")
            || has_word("gothic")
            || has_word("grotesk")
            || has_word("ui")
            || ["arial", "helvetica"]
                .iter()
                .any(|prefix| normalized.starts_with(prefix)));
    ReaderFontClassification {
        serif,
        sans_serif,
        monospace: false,
    }
}

fn is_symbolic_reader_font(family: &str, panose: Option<&[u8]>, family_class: Option<i16>) -> bool {
    let family_class = family_class.map(|value| value.to_be_bytes()[0]);
    let normalized = family.to_ascii_lowercase();
    panose.is_some_and(|panose| panose.first() == Some(&5))
        || family_class == Some(12)
        || normalized.contains("math")
        || normalized.contains("symbol")
        || normalized.contains("webdings")
        || normalized.contains("wingdings")
}

fn supports_common_chinese(charmap: &parley::fontique::Charmap<'_>) -> bool {
    const COMMON_CHINESE_PROBE: &str =
        "中文字体阅读书籍测试国家学习时间这样问题繁體國學時門風龍臺灣";
    COMMON_CHINESE_PROBE
        .chars()
        .all(|character| charmap.map(character).is_some())
}

fn supports_common_latin(charmap: &parley::fontique::Charmap<'_>) -> bool {
    const COMMON_LATIN_PROBE: &str =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    COMMON_LATIN_PROBE
        .chars()
        .all(|character| charmap.map(character).is_some())
}

fn has_embedded_bitmap_glyphs(font: &FontRef<'_>) -> bool {
    const BITMAP_TABLES: [[u8; 4]; 6] =
        [*b"EBDT", *b"EBLC", *b"CBDT", *b"CBLC", *b"bdat", *b"bloc"];
    font.table_directory()
        .table_records()
        .iter()
        .any(|record| BITMAP_TABLES.contains(&record.tag().to_be_bytes()))
}
