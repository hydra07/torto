#[derive(Clone, Copy)]
struct QuoteLayoutMetrics {
    start: f32,
    end: f32,
    before: f32,
    after: f32,
}

impl QuoteLayoutMetrics {
    fn has_symmetric_inset(self) -> bool {
        const MIN_HORIZONTAL_INSET: f32 = 4.0;
        let symmetry_tolerance = 4.0_f32.max(self.start.max(self.end) * 0.25);
        self.start >= MIN_HORIZONTAL_INSET
            && self.end >= MIN_HORIZONTAL_INSET
            && (self.start - self.end).abs() <= symmetry_tolerance
    }

    fn compatible_with(self, other: Self) -> bool {
        let horizontal_tolerance =
            4.0_f32.max(self.start.max(self.end).max(other.start).max(other.end) * 0.25);
        (self.start - other.start).abs() <= horizontal_tolerance
            && (self.end - other.end).abs() <= horizontal_tolerance
    }

    fn has_vertical_boundary(self) -> bool {
        const MIN_VERTICAL_SPACING: f32 = 0.5;
        self.before > MIN_VERTICAL_SPACING || self.after > MIN_VERTICAL_SPACING
    }
}

#[derive(Default)]
struct StyleSheet {
    rules: Vec<StyleRule>,
    next_order: usize,
}

struct StyleRule {
    selector: SimpleSelector,
    specificity: u16,
    order: usize,
    declarations: Vec<(String, String)>,
}

struct SimpleSelector {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

impl StyleSheet {
    fn from_document(
        document: &Document<'_>,
        base: &PublicationUrl,
        load_stylesheet: &mut impl FnMut(&PublicationUrl) -> Option<String>,
    ) -> Self {
        let mut sheet = Self::default();
        for node in document.descendants().filter(Node::is_element) {
            match node.tag_name().name().to_ascii_lowercase().as_str() {
                "style" => {
                    let css = node
                        .descendants()
                        .filter(Node::is_text)
                        .filter_map(|text| text.text())
                        .collect::<String>();
                    sheet.add_css(&css);
                }
                "link"
                    if attribute_local(node, "rel").is_some_and(|rel| {
                        rel.split_ascii_whitespace()
                            .any(|token| token.eq_ignore_ascii_case("stylesheet"))
                    }) =>
                {
                    let Some(href) = attribute_local(node, "href") else {
                        continue;
                    };
                    let Ok(href) = base.resolve(href).map(|url| url.resource_url()) else {
                        continue;
                    };
                    if let Some(css) = load_stylesheet(&href) {
                        sheet.add_css(&css);
                    }
                }
                _ => {}
            }
        }
        sheet
    }

    fn add_css(&mut self, css: &str) {
        let css = strip_css_comments(css);
        let mut cursor = 0;
        while let Some(relative_open) = css[cursor..].find('{') {
            let open = cursor + relative_open;
            let Some(close) = matching_brace(&css, open) else {
                break;
            };
            let prelude = css[cursor..open].trim();
            if !prelude.starts_with('@') {
                let declarations = declarations(&css[open + 1..close]).collect::<Vec<_>>();
                for raw_selector in prelude.split(',') {
                    let Some(selector) = SimpleSelector::parse(raw_selector) else {
                        continue;
                    };
                    self.rules.push(StyleRule {
                        specificity: selector.specificity(),
                        selector,
                        order: self.next_order,
                        declarations: declarations.clone(),
                    });
                }
                self.next_order = self.next_order.saturating_add(1);
            }
            cursor = close.saturating_add(1);
        }
    }

    fn block_style(&self, node: Node<'_, '_>, mut style: BlockStyle) -> BlockStyle {
        let mut ancestors = node
            .ancestors()
            .filter(Node::is_element)
            .collect::<Vec<_>>();
        ancestors.reverse();
        for ancestor in ancestors {
            let inherited_only = ancestor != node;
            let properties = self.cascaded_properties(ancestor);
            apply_block_properties(&mut style, &properties, inherited_only);
            if let Some(alignment) =
                attribute_local(ancestor, "align").and_then(parse_text_alignment)
            {
                style.align = alignment;
                style.authored_alignment = Some(alignment);
            }
        }
        style
    }

    fn table_cell_alignment(&self, cell: Node<'_, '_>) -> Option<TextAlignment> {
        self.inherited_text_alignment(cell).or_else(|| {
            cell.descendants()
                .skip(1)
                .filter(|node| {
                    node.is_element()
                        && is_block_boundary(node.tag_name().name().to_ascii_lowercase().as_str())
                })
                .find_map(|node| self.declared_text_alignment(node))
        })
    }

    fn inherited_text_alignment(&self, node: Node<'_, '_>) -> Option<TextAlignment> {
        let mut ancestors = node
            .ancestors()
            .filter(Node::is_element)
            .collect::<Vec<_>>();
        ancestors.reverse();
        ancestors.into_iter().fold(None, |alignment, ancestor| {
            self.declared_text_alignment(ancestor).or(alignment)
        })
    }

    fn declared_text_alignment(&self, node: Node<'_, '_>) -> Option<TextAlignment> {
        self.cascaded_properties(node)
            .get("text-align")
            .and_then(|value| parse_text_alignment(value))
            .or_else(|| attribute_local(node, "align").and_then(parse_text_alignment))
    }

    fn sole_content_block_alignment(&self, node: Node<'_, '_>) -> Option<TextAlignment> {
        let mut container = node;
        loop {
            let mut sole_element = None;
            for child in container.children() {
                if child.is_text() && child.text().is_some_and(|text| !text.trim().is_empty()) {
                    return None;
                }
                if child.is_element() && sole_element.replace(child).is_some() {
                    return None;
                }
            }
            let child = sole_element?;
            let properties = self.cascaded_properties(child);
            let establishes_block_box = properties.get("display").is_some_and(|display| {
                matches!(
                    display.split_ascii_whitespace().next(),
                    Some("block" | "inline-block" | "flow-root" | "list-item" | "table-cell")
                )
            });
            if establishes_block_box {
                return self.inherited_text_alignment(child);
            }
            container = child;
        }
    }

    fn has_visual_boundary(&self, node: Node<'_, '_>) -> bool {
        let properties = self.cascaded_properties(node);
        let visible_paint = |value: &str| {
            let value = value.trim();
            !value.is_empty()
                && !matches!(
                    value,
                    "none" | "transparent" | "inherit" | "initial" | "unset"
                )
                && value != "0"
        };
        [
            "background",
            "background-color",
            "border",
            "border-left",
            "border-right",
        ]
        .into_iter()
        .any(|name| {
            properties
                .get(name)
                .is_some_and(|value| visible_paint(value))
        }) || [
            "padding-left",
            "padding-right",
            "padding-top",
            "padding-bottom",
            "padding-inline-start",
            "padding-inline-end",
        ]
        .into_iter()
        .any(|name| {
            properties
                .get(name)
                .and_then(|value| css_length(value))
                .is_some_and(|value| value > 0.0)
        })
    }

    fn quote_layout_metrics(&self, node: Node<'_, '_>) -> QuoteLayoutMetrics {
        const REFERENCE_WIDTH: f32 = 1_000.0;

        let properties = self.cascaded_properties(node);
        let horizontal = |logical: &str, physical: &str| {
            properties
                .get(logical)
                .or_else(|| properties.get(physical))
                .and_then(|value| css_horizontal_length(value))
                .map(|(pixels, fraction)| fraction.mul_add(REFERENCE_WIDTH, pixels))
                .unwrap_or(0.0)
        };
        let vertical = |logical: &str, physical: &str| {
            properties
                .get(logical)
                .or_else(|| properties.get(physical))
                .and_then(|value| css_length(value))
                .filter(|value| value.is_finite())
                .unwrap_or(0.0)
        };

        let start = horizontal("margin-inline-start", "margin-left")
            + horizontal("padding-inline-start", "padding-left");
        let end = horizontal("margin-inline-end", "margin-right")
            + horizontal("padding-inline-end", "padding-right");
        let before = vertical("margin-block-start", "margin-top")
            + vertical("padding-block-start", "padding-top");
        let after = vertical("margin-block-end", "margin-bottom")
            + vertical("padding-block-end", "padding-bottom");

        QuoteLayoutMetrics {
            start,
            end,
            before,
            after,
        }
    }

    fn grouped_quote_body_layout(&self, node: Node<'_, '_>) -> Option<QuoteLayoutMetrics> {
        let layout = self.quote_layout_metrics(node);
        layout.has_symmetric_inset().then_some(layout)
    }

    fn has_sibling_quote_attribution_role(
        &self,
        node: Node<'_, '_>,
        body_layout: QuoteLayoutMetrics,
        body_text_style: TextStyle,
        body_has_vertical_boundary: bool,
    ) -> bool {
        let attribution_layout = self.quote_layout_metrics(node);
        let attribution_text_style =
            self.text_style_for_block(node, TextBlockKind::QuoteAttribution);
        let has_role = attribution_text_style.size_scale + 0.05 < body_text_style.size_scale
            || attribution_text_style.italic != body_text_style.italic
            || attribution_layout.start + 4.0 < body_layout.start
            || attribution_layout.end + 4.0 < body_layout.end
            || starts_with_attribution_marker(node)
            || first_visible_text_has_attribution_markup(node);
        let has_related_inset = attribution_layout.compatible_with(body_layout)
            || attribution_layout.start + 4.0 < body_layout.start
            || attribution_layout.end + 4.0 < body_layout.end;
        has_role
            && has_related_inset
            && (body_has_vertical_boundary || attribution_layout.has_vertical_boundary())
    }

    fn has_distinct_quote_typography(&self, node: Node<'_, '_>) -> bool {
        let properties = self.cascaded_properties(node);
        let parent_properties = node
            .parent()
            .filter(Node::is_element)
            .map(|parent| self.cascaded_properties(parent));
        let differs_from_parent = |name: &str| {
            properties.get(name).is_some_and(|value| {
                let value = value.trim();
                !value.is_empty()
                    && !matches!(value, "inherit" | "initial" | "unset" | "normal")
                    && parent_properties
                        .as_ref()
                        .and_then(|parent| parent.get(name))
                        .is_none_or(|parent| parent.trim() != value)
            })
        };
        differs_from_parent("font-family")
            || differs_from_parent("font-style")
            || differs_from_parent("font-weight")
            || self.declared_text_alignment(node) == Some(TextAlignment::Center)
    }

    fn has_standalone_quote_layout(&self, node: Node<'_, '_>) -> bool {
        const MIN_VERTICAL_SPACING: f32 = 0.5;
        let layout = self.quote_layout_metrics(node);
        layout.has_symmetric_inset()
            && layout.before > MIN_VERTICAL_SPACING
            && layout.after > MIN_VERTICAL_SPACING
    }

    fn text_style_for_block(&self, node: Node<'_, '_>, kind: TextBlockKind) -> TextStyle {
        let mut ancestors = node
            .ancestors()
            .filter(Node::is_element)
            .collect::<Vec<_>>();
        ancestors.reverse();
        let mut style = TextStyle::default();
        for ancestor in ancestors {
            let inherited_size = style.size_scale;
            if ancestor == node {
                apply_semantic_block_style(kind, &mut style, inherited_size);
            }
            self.apply_text_node(ancestor, &mut style, inherited_size);
        }
        style
    }

    fn inline_image_height_em(&self, node: Node<'_, '_>) -> Option<f32> {
        let properties = self.cascaded_properties(node);
        properties
            .get("height")
            .map(String::as_str)
            .or_else(|| attribute_local(node, "height"))
            .and_then(inline_em_length)
    }

    fn image_establishes_block_layout(&self, node: Node<'_, '_>) -> bool {
        let properties = self.cascaded_properties(node);
        let display = properties
            .get("display")
            .and_then(|value| value.split_ascii_whitespace().next());
        if matches!(display, Some("inline" | "inline-block")) {
            return false;
        }
        if matches!(
            display,
            Some(
                "block"
                    | "flow-root"
                    | "flex"
                    | "grid"
                    | "list-item"
                    | "table"
                    | "table-row"
                    | "table-cell"
            )
        ) {
            return true;
        }
        if properties
            .get("float")
            .is_some_and(|value| !matches!(value.trim(), "none" | "initial" | "unset"))
            || properties
                .get("position")
                .is_some_and(|value| matches!(value.trim(), "absolute" | "fixed"))
        {
            return true;
        }

        // Size is deliberately only a fallback. A near-column-width authored
        // image without an explicit display value is almost certainly a figure,
        // while small and tall formula rasters remain governed by text context.
        properties
            .get("width")
            .and_then(|value| image_length(value))
            .is_some_and(|width| matches!(width, ImageLength::Fraction(value) if value >= 0.8))
    }

    fn inline_image_alignment(&self, node: Node<'_, '_>) -> InlineImageAlignment {
        let properties = self.cascaded_properties(node);
        match properties
            .get("vertical-align")
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("middle") => InlineImageAlignment::Middle,
            Some("text-top") => InlineImageAlignment::TextTop,
            Some("text-bottom") => InlineImageAlignment::TextBottom,
            Some("top") => InlineImageAlignment::Top,
            Some("bottom") => InlineImageAlignment::Bottom,
            Some("super") => InlineImageAlignment::Super,
            Some("sub") => InlineImageAlignment::Sub,
            _ => InlineImageAlignment::Baseline,
        }
    }

    fn image_style(&self, node: Node<'_, '_>) -> ImageStyle {
        let mut style = ImageStyle {
            width: attribute_local(node, "width").and_then(image_length),
            height: attribute_local(node, "height").and_then(image_length),
            ..ImageStyle::default()
        };
        let properties = self.cascaded_properties(node);
        if let Some(value) = properties
            .get("width")
            .and_then(|value| image_length(value))
        {
            style.width = Some(value);
        }
        if let Some(value) = properties
            .get("height")
            .and_then(|value| image_length(value))
        {
            style.height = Some(value);
        }
        if let Some(value) = properties
            .get("max-width")
            .and_then(|value| image_length(value))
        {
            style.max_width = Some(value);
        }
        if let Some(value) = properties
            .get("max-height")
            .and_then(|value| image_length(value))
        {
            style.max_height = Some(value);
        }
        if let Some(value) = properties
            .get("margin-top")
            .and_then(|value| css_length(value))
        {
            style.margin_before = value;
        }
        if let Some(value) = properties
            .get("margin-bottom")
            .and_then(|value| css_length(value))
        {
            style.margin_after = value;
        }
        style
    }

    fn apply_text_node(&self, node: Node<'_, '_>, style: &mut TextStyle, inherited_size: f32) {
        let properties = self.cascaded_properties(node);
        apply_text_properties(style, &properties, inherited_size);
        if let Some(language) = attribute_local(node, "lang") {
            style.language = TextLanguage::from_bcp47(language);
        }
    }

    fn cascaded_properties(&self, node: Node<'_, '_>) -> HashMap<String, String> {
        let mut matching = self
            .rules
            .iter()
            .filter(|rule| rule.selector.matches(node))
            .collect::<Vec<_>>();
        matching.sort_by_key(|rule| (rule.specificity, rule.order));

        let mut properties = HashMap::new();
        for rule in matching {
            insert_declarations(&mut properties, rule.declarations.iter().cloned());
        }
        if let Some(inline) = attribute_local(node, "style") {
            insert_declarations(&mut properties, declarations(inline));
        }
        properties
    }
}

impl SimpleSelector {
    fn parse(raw: &str) -> Option<Self> {
        let mut rest = raw.trim();
        if rest.is_empty()
            || rest
                .chars()
                .any(|character| character.is_whitespace() || ">+~[:*".contains(character))
        {
            return None;
        }

        let mut selector = Self {
            tag: None,
            id: None,
            classes: Vec::new(),
        };
        if !rest.starts_with('.') && !rest.starts_with('#') {
            let (tag, tail) = take_css_identifier(rest)?;
            selector.tag = Some(tag.to_ascii_lowercase());
            rest = tail;
        }
        while !rest.is_empty() {
            let (kind, tail) = rest.split_at(1);
            let (value, next) = take_css_identifier(tail)?;
            match kind {
                "." => selector.classes.push(value.to_owned()),
                "#" => selector.id = Some(value.to_owned()),
                _ => return None,
            }
            rest = next;
        }
        Some(selector)
    }

    fn specificity(&self) -> u16 {
        u16::from(self.id.is_some()) * 100
            + u16::try_from(self.classes.len()).unwrap_or(u16::MAX) * 10
            + u16::from(self.tag.is_some())
    }

    fn matches(&self, node: Node<'_, '_>) -> bool {
        if self
            .tag
            .as_deref()
            .is_some_and(|tag| !node.tag_name().name().eq_ignore_ascii_case(tag))
        {
            return false;
        }
        if self
            .id
            .as_deref()
            .is_some_and(|id| attribute_local(node, "id").is_none_or(|candidate| candidate != id))
        {
            return false;
        }
        let classes = attribute_local(node, "class").unwrap_or_default();
        self.classes.iter().all(|class| {
            classes
                .split_ascii_whitespace()
                .any(|candidate| candidate == class)
        })
    }
}

fn apply_semantic_block_style(kind: TextBlockKind, style: &mut TextStyle, inherited_size: f32) {
    match kind {
        TextBlockKind::Heading(level) | TextBlockKind::HeadingOrdinal(level) => {
            style.bold = true;
            style.size_scale = inherited_size
                * match level {
                    1 => 1.5,
                    2 => 1.3,
                    3 => 1.15,
                    _ => 1.05,
                };
        }
        TextBlockKind::Preformatted => style.size_scale = inherited_size * 0.9,
        _ => {}
    }
}

fn apply_block_properties(
    style: &mut BlockStyle,
    properties: &HashMap<String, String>,
    inherited_only: bool,
) {
    // Reading IR flattens nested HTML boxes into blocks. Preserve the start-side
    // offset contributed by every containing box so authored lists keep their
    // visual hierarchy after flattening.
    for property in [
        properties
            .get("margin-inline-start")
            .or_else(|| properties.get("margin-left")),
        properties
            .get("padding-inline-start")
            .or_else(|| properties.get("padding-left")),
    ]
    .into_iter()
    .flatten()
    {
        if let Some((pixels, fraction)) = css_horizontal_length(property) {
            style.margin_start += pixels;
            style.margin_start_fraction += fraction;
        }
    }
    if let Some(alignment) = properties
        .get("text-align")
        .and_then(|value| parse_text_alignment(value))
    {
        style.align = alignment;
        style.authored_alignment = Some(alignment);
    }
    if let Some(value) = properties
        .get("text-indent")
        .and_then(|value| css_length(value))
    {
        style.indent = value;
    }
    if let Some(value) = properties
        .get("line-height")
        .and_then(|value| css_line_height(value))
    {
        style.line_height = value;
    }
    if inherited_only {
        return;
    }
    if let Some(value) = properties
        .get("margin-top")
        .and_then(|value| css_length(value))
    {
        style.margin_before = value;
    }
    if let Some(value) = properties
        .get("margin-bottom")
        .and_then(|value| css_length(value))
    {
        style.margin_after = value;
    }
}

fn parse_text_alignment(value: &str) -> Option<TextAlignment> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" | "start" => Some(TextAlignment::Start),
        "center" => Some(TextAlignment::Center),
        "right" | "end" => Some(TextAlignment::End),
        "justify" => Some(TextAlignment::Justify),
        _ => None,
    }
}

fn apply_text_properties(
    style: &mut TextStyle,
    properties: &HashMap<String, String>,
    inherited_size: f32,
) {
    if let Some(value) = properties
        .get("font-size")
        .and_then(|value| css_scale(value))
    {
        style.size_scale = inherited_size * value;
    }
    if let Some(value) = properties.get("font-weight") {
        style.bold = value == "bold"
            || value == "bolder"
            || value.parse::<u16>().is_ok_and(|weight| weight >= 600);
    }
    if let Some(value) = properties.get("font-style") {
        style.italic = matches!(value.as_str(), "italic" | "oblique");
    }
    if let Some(value) = properties
        .get("text-decoration-line")
        .or_else(|| properties.get("text-decoration"))
    {
        style.underline = value.contains("underline");
    }
    if let Some(color) = properties.get("color").and_then(|value| css_color(value)) {
        style.color = color;
    }
    if let Some(value) = properties.get("vertical-align") {
        style.baseline = match value.trim().to_ascii_lowercase().as_str() {
            "super" => TextBaseline::Superscript,
            "sub" => TextBaseline::Subscript,
            "baseline" => TextBaseline::Normal,
            _ => style.baseline,
        };
    }
    if let Some(value) = properties.get("hyphens") {
        style.hyphenation = match value.trim().to_ascii_lowercase().as_str() {
            "none" => HyphenationMode::None,
            "manual" => HyphenationMode::Manual,
            "auto" => HyphenationMode::Auto,
            _ => style.hyphenation,
        };
    }
}

fn insert_declarations(
    properties: &mut HashMap<String, String>,
    declarations: impl IntoIterator<Item = (String, String)>,
) {
    for (name, value) in declarations {
        if name == "margin" {
            if let Some((top, right, bottom, left)) = box_sides(&value) {
                properties.insert("margin-top".into(), top.to_owned());
                properties.insert("margin-right".into(), right.to_owned());
                properties.insert("margin-bottom".into(), bottom.to_owned());
                properties.insert("margin-left".into(), left.to_owned());
            }
        } else if name == "padding" {
            if let Some((top, right, bottom, left)) = box_sides(&value) {
                properties.insert("padding-top".into(), top.to_owned());
                properties.insert("padding-right".into(), right.to_owned());
                properties.insert("padding-bottom".into(), bottom.to_owned());
                properties.insert("padding-left".into(), left.to_owned());
            }
        } else if matches!(
            name.as_str(),
            "margin-inline" | "padding-inline" | "margin-block" | "padding-block"
        ) {
            if let Some((start, end)) = axis_sides(&value) {
                let axis = name
                    .strip_prefix("margin-")
                    .or_else(|| name.strip_prefix("padding-"))
                    .expect("matched box-axis property");
                let prefix = if name.starts_with("margin-") {
                    "margin"
                } else {
                    "padding"
                };
                properties.insert(format!("{prefix}-{axis}-start"), start.to_owned());
                properties.insert(format!("{prefix}-{axis}-end"), end.to_owned());
            }
        } else {
            properties.insert(name, value);
        }
    }
}

fn take_css_identifier(input: &str) -> Option<(&str, &str)> {
    let end = input
        .char_indices()
        .find_map(|(index, character)| {
            (!character.is_ascii_alphanumeric() && !matches!(character, '-' | '_')).then_some(index)
        })
        .unwrap_or(input.len());
    (end > 0).then(|| input.split_at(end))
}

fn strip_css_comments(css: &str) -> String {
    let mut output = String::with_capacity(css.len());
    let mut remaining = css;
    while let Some(start) = remaining.find("/*") {
        output.push_str(&remaining[..start]);
        let Some(end) = remaining[start + 2..].find("*/") else {
            return output;
        };
        remaining = &remaining[start + end + 4..];
    }
    output.push_str(remaining);
    output
}

fn matching_brace(css: &str, open: usize) -> Option<usize> {
    let mut depth = 0_u32;
    for (relative, character) in css[open..].char_indices() {
        match character {
            '{' => depth = depth.saturating_add(1),
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + relative);
                }
            }
            _ => {}
        }
    }
    None
}

fn box_sides(value: &str) -> Option<(&str, &str, &str, &str)> {
    let values = value.split_ascii_whitespace().collect::<Vec<_>>();
    match values.as_slice() {
        [all] => Some((all, all, all, all)),
        [vertical, horizontal] => Some((vertical, horizontal, vertical, horizontal)),
        [top, horizontal, bottom] => Some((top, horizontal, bottom, horizontal)),
        [top, right, bottom, left] => Some((top, right, bottom, left)),
        _ => None,
    }
}

fn axis_sides(value: &str) -> Option<(&str, &str)> {
    let values = value.split_ascii_whitespace().collect::<Vec<_>>();
    match values.as_slice() {
        [both] => Some((both, both)),
        [start, end] => Some((start, end)),
        _ => None,
    }
}

fn declarations(style: &str) -> impl Iterator<Item = (String, String)> + '_ {
    style.split(';').filter_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        Some((
            name.trim().to_ascii_lowercase(),
            value
                .trim()
                .trim_end_matches("!important")
                .trim()
                .to_ascii_lowercase(),
        ))
    })
}

fn css_length(value: &str) -> Option<f32> {
    const BASE_FONT_SIZE: f32 = 16.0;
    let value = value.trim();
    let (number, scale) = if let Some(number) = value.strip_suffix("px") {
        (number, 1.0)
    } else if let Some(number) = value.strip_suffix("rem") {
        (number, BASE_FONT_SIZE)
    } else if let Some(number) = value.strip_suffix("em") {
        (number, BASE_FONT_SIZE)
    } else if let Some(number) = value.strip_suffix("pt") {
        (number, 96.0 / 72.0)
    } else {
        (value, 1.0)
    };
    number.trim().parse::<f32>().ok().map(|v| v * scale)
}

fn css_horizontal_length(value: &str) -> Option<(f32, f32)> {
    if let Some(percent) = value.trim().strip_suffix('%') {
        return percent
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| (0.0, value / 100.0));
    }
    css_length(value)
        .filter(|value| value.is_finite())
        .map(|value| (value, 0.0))
}

fn css_scale(value: &str) -> Option<f32> {
    const BASE_FONT_SIZE: f32 = 16.0;
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        return percent.parse::<f32>().ok().map(|number| number / 100.0);
    }
    if let Some(em) = value
        .strip_suffix("rem")
        .or_else(|| value.strip_suffix("em"))
    {
        return em.parse::<f32>().ok();
    }
    if let Some(px) = value.strip_suffix("px") {
        return px.parse::<f32>().ok().map(|number| number / BASE_FONT_SIZE);
    }
    None
}

fn css_line_height(value: &str) -> Option<f32> {
    let value = value.trim();
    let parsed = if let Some(em) = value.strip_suffix("em") {
        em.parse::<f32>().ok()
    } else if let Some(percent) = value.strip_suffix('%') {
        percent.parse::<f32>().ok().map(|number| number / 100.0)
    } else if let Some(px) = value.strip_suffix("px") {
        px.parse::<f32>().ok().map(|number| number / 16.0)
    } else {
        value.parse::<f32>().ok()
    }?;
    (0.8..=4.0).contains(&parsed).then_some(parsed)
}

fn css_color(value: &str) -> Option<Rgba> {
    let hex = value.trim().strip_prefix('#')?;
    let (red, green, blue) = match hex.len() {
        3 => (
            u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?,
            u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?,
            u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?,
        ),
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        _ => return None,
    };
    Some(Rgba {
        red,
        green,
        blue,
        alpha: 255,
    })
}

fn image_length(value: &str) -> Option<ImageLength> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") || value.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some(percent) = value.strip_suffix('%') {
        let fraction = percent.trim().parse::<f32>().ok()? / 100.0;
        return fraction
            .is_finite()
            .then_some(ImageLength::Fraction(fraction.max(0.0)));
    }
    let pixels = css_length(value)?;
    pixels
        .is_finite()
        .then_some(ImageLength::Pixels(pixels.max(0.0)))
}

fn inline_em_length(value: &str) -> Option<f32> {
    let value = value.trim();
    if value.ends_with("rem") {
        return None;
    }
    let em = value.strip_suffix("em")?.trim().parse::<f32>().ok()?;
    em.is_finite().then_some(em.max(0.0))
}

fn attribute_local<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name().eq_ignore_ascii_case(name))
        .map(|attribute| attribute.value())
}
