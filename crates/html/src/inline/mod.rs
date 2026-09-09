struct InlineCollector {
    content: Vec<Inline>,
    preserve_whitespace: bool,
    last_was_space: bool,
}

impl InlineCollector {
    fn new(preserve_whitespace: bool) -> Self {
        Self {
            content: Vec::new(),
            preserve_whitespace,
            last_was_space: true,
        }
    }

    fn push_text(&mut self, text: &str, style: TextStyle, link: Option<PublicationUrl>) {
        let normalized = if self.preserve_whitespace {
            text.to_owned()
        } else {
            let mut result = String::new();
            for character in text.chars() {
                if is_collapsible_html_whitespace(character) {
                    if !self.last_was_space {
                        result.push(' ');
                        self.last_was_space = true;
                    }
                } else {
                    result.push(character);
                    self.last_was_space = false;
                }
            }
            result
        };
        if normalized.is_empty() {
            return;
        }
        if let Some(Inline::Text(previous)) = self.content.last_mut()
            && previous.style == style
            && previous.link == link
        {
            previous.text.push_str(&normalized);
        } else {
            self.content.push(Inline::Text(TextRun {
                text: normalized,
                style,
                link,
            }));
        }
    }

    fn push_footnote_reference_marker(
        &mut self,
        marker: &str,
        style: TextStyle,
        link: Option<PublicationUrl>,
    ) {
        if !self.preserve_whitespace {
            if let Some(Inline::Text(previous)) = self.content.last_mut() {
                while previous
                    .text
                    .chars()
                    .next_back()
                    .is_some_and(|character| matches!(character, ' ' | '\u{00a0}'))
                {
                    previous.text.pop();
                }
                if previous.text.is_empty() {
                    self.content.pop();
                }
            }
            self.last_was_space = false;
        }
        self.push_text(marker, style, link);
    }

    fn push_break(&mut self) {
        self.content.push(Inline::Break);
        self.last_was_space = true;
    }

    fn push_block_break(&mut self) {
        if let Some(Inline::Text(run)) = self.content.last_mut() {
            while run.text.ends_with(' ') {
                run.text.pop();
            }
            if run.text.is_empty() {
                self.content.pop();
            }
        }
        if self.content.is_empty() || matches!(self.content.last(), Some(Inline::Break)) {
            return;
        }
        self.push_break();
    }

    fn push_math(&mut self, latex: &str, display: bool, size_scale: f32) {
        let latex = latex.trim().to_owned();
        if latex.is_empty() {
            return;
        }
        self.content.push(Inline::Math(MathRun {
            latex,
            display,
            size_scale,
        }));
        self.last_was_space = false;
    }

    fn push_image(&mut self, image: InlineImageRun) {
        self.content.push(Inline::Image(Box::new(image)));
        self.last_was_space = false;
    }

    fn finish(&mut self) {
        if let Some(Inline::Text(run)) = self.content.last_mut() {
            while run.text.ends_with(' ') {
                run.text.pop();
            }
        }
        self.content
            .retain(|inline| !matches!(inline, Inline::Text(run) if run.text.is_empty()));
    }
}

fn is_collapsible_html_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}' | '\u{000a}' | '\u{000c}' | '\u{000d}' | ' '
    )
}

fn is_authored_spacing_paragraph(node: Node<'_, '_>) -> bool {
    let mut has_spacing_marker = false;
    for descendant in node.descendants().skip(1) {
        if descendant.is_element() {
            let name = descendant.tag_name().name().to_ascii_lowercase();
            if matches!(name.as_str(), "img" | "image" | "svg" | "math") {
                return false;
            }
            if name == "br" {
                has_spacing_marker = true;
            }
            continue;
        }
        let Some(text) = descendant.text() else {
            continue;
        };
        for character in text.chars() {
            if !character.is_whitespace() {
                return false;
            }
            if matches!(character, '\u{00a0}' | '\u{3000}') {
                has_spacing_marker = true;
            }
        }
    }
    has_spacing_marker
}

struct InlineParseContext<'a> {
    base: &'a PublicationUrl,
    styles: &'a StyleSheet,
    footnote_links: &'a HashMap<usize, LinkRole>,
    inline_images: bool,
}

impl<'a> InlineParseContext<'a> {
    const fn new(
        base: &'a PublicationUrl,
        styles: &'a StyleSheet,
        footnote_links: &'a HashMap<usize, LinkRole>,
    ) -> Self {
        Self {
            base,
            styles,
            footnote_links,
            inline_images: false,
        }
    }

    const fn with_inline_images(mut self, enabled: bool) -> Self {
        self.inline_images = enabled;
        self
    }
}

fn node_has_visible_text(node: Node<'_, '_>) -> bool {
    node.descendants()
        .filter(Node::is_text)
        .filter_map(|text| text.text())
        .any(|text| !text.trim().is_empty())
}

fn image_should_be_inline(node: Node<'_, '_>, context: &InlineParseContext<'_>) -> bool {
    context.inline_images && !context.styles.image_establishes_block_layout(node)
}

fn inline_image(
    node: Node<'_, '_>,
    inherited: TextStyle,
    context: &InlineParseContext<'_>,
) -> Option<InlineImageRun> {
    if !image_should_be_inline(node, context) {
        return None;
    }
    let src = attribute_local(node, "src")
        .or_else(|| attribute_local(node, "href"))?
        .trim();
    if src.is_empty() {
        return None;
    }
    Some(InlineImageRun {
        image: ImageBlock {
            href: context.base.resolve(src).ok()?.resource_url(),
            alt: attribute_local(node, "alt").unwrap_or_default().to_owned(),
            style: context.styles.image_style(node),
            source: None,
            text_layer: None,
        },
        size_scale: context
            .styles
            .inline_image_height_em(node)
            .map_or(inherited.size_scale, |height| height * inherited.size_scale),
        intrinsic_sizing: context.styles.inline_image_height_em(node).is_none(),
        vertical_align: context.styles.inline_image_alignment(node),
        presentation: attribute_local(node, "role")
            .is_some_and(|role| role.eq_ignore_ascii_case("presentation")),
    })
}

fn image_is_footnote_reference(
    image: Node<'_, '_>,
    footnote_links: &HashMap<usize, LinkRole>,
) -> bool {
    image.ancestors().skip(1).any(|ancestor| {
        ancestor.is_element()
            && ancestor.tag_name().name().eq_ignore_ascii_case("a")
            && footnote_links.get(&ancestor.range().start) == Some(&LinkRole::FootnoteReference)
    })
}

fn footnote_reference_image_marker(image: Node<'_, '_>) -> String {
    let accessible_text = [
        attribute_local(image, "alt"),
        attribute_local(image, "title"),
        attribute_local(image, "aria-label"),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|text| !text.is_empty());
    let note_text = attribute_local(image, "zy-footnote")
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .or(accessible_text);

    if note_text.is_some_and(|text| text.contains("译者注")) {
        return "译".to_owned();
    }
    if let Some(marker) = accessible_text.and_then(normalize_footnote_marker) {
        return marker;
    }
    "注".to_owned()
}

fn collect_inline(
    node: Node<'_, '_>,
    inherited: TextStyle,
    link: Option<&PublicationUrl>,
    context: &InlineParseContext<'_>,
    collector: &mut InlineCollector,
) {
    collect_inline_with_block_boundaries(node, inherited, link, context, collector, false);
}

fn collect_table_cell_inline(
    node: Node<'_, '_>,
    inherited: TextStyle,
    link: Option<&PublicationUrl>,
    context: &InlineParseContext<'_>,
    collector: &mut InlineCollector,
) {
    collect_inline_with_block_boundaries(node, inherited, link, context, collector, true);
}

fn collect_inline_with_block_boundaries(
    node: Node<'_, '_>,
    inherited: TextStyle,
    link: Option<&PublicationUrl>,
    context: &InlineParseContext<'_>,
    collector: &mut InlineCollector,
    preserve_block_boundaries: bool,
) {
    for child in node.children() {
        collect_inline_node_with_block_boundaries(
            child,
            inherited,
            link,
            context,
            collector,
            preserve_block_boundaries,
        );
    }
}

fn collect_inline_node(
    node: Node<'_, '_>,
    inherited: TextStyle,
    link: Option<&PublicationUrl>,
    context: &InlineParseContext<'_>,
    collector: &mut InlineCollector,
) {
    collect_inline_node_with_block_boundaries(node, inherited, link, context, collector, false);
}

fn collect_inline_node_with_block_boundaries(
    node: Node<'_, '_>,
    inherited: TextStyle,
    link: Option<&PublicationUrl>,
    context: &InlineParseContext<'_>,
    collector: &mut InlineCollector,
    preserve_block_boundaries: bool,
) {
    if node.is_text() {
        collector.push_text(node.text().unwrap_or_default(), inherited, link.cloned());
        return;
    }
    if !node.is_element() {
        return;
    }
    let name = node.tag_name().name().to_ascii_lowercase();
    if name == "br" {
        collector.push_break();
        return;
    }
    if name == "img" || name == "image" {
        if inherited.link_role == LinkRole::FootnoteReference && link.is_some() {
            collector.push_footnote_reference_marker(
                &footnote_reference_image_marker(node),
                inherited,
                link.cloned(),
            );
        } else if let Some(image) = inline_image(node, inherited, context) {
            collector.push_image(image);
        }
        return;
    }
    if matches!(name.as_str(), "script" | "style") {
        return;
    }
    if preserve_block_boundaries && is_block_boundary(name.as_str()) {
        collector.push_block_break();
    }

    let mut style = inherited;
    match name.as_str() {
        "strong" | "b" => style.bold = true,
        "em" => {
            style.italic = true;
            style.emphasis = true;
        }
        "i" => {
            style.italic = true;
            style.alternate_voice = true;
        }
        "cite" => {
            style.italic = true;
            style.citation = true;
        }
        "u" => style.underline = true,
        "small" => style.size_scale *= 0.85,
        "big" => style.size_scale *= 1.2,
        "sup" => {
            style.baseline = TextBaseline::Superscript;
            style.size_scale *= 0.75;
        }
        "sub" => {
            style.baseline = TextBaseline::Subscript;
            style.size_scale *= 0.75;
        }
        _ => {}
    }
    context
        .styles
        .apply_text_node(node, &mut style, inherited.size_scale);
    if name == "span" {
        let classes = attribute_local(node, "class").unwrap_or_default();
        if classes
            .split_ascii_whitespace()
            .any(|class| matches!(class, "footnote" | "footnote1"))
        {
            style.inline_role = InlineRole::Footnote;
        }
        let is_math = classes
            .split_ascii_whitespace()
            .any(|class| class == "math");
        if is_math {
            let display = classes
                .split_ascii_whitespace()
                .any(|class| class == "math-display");
            let latex = node
                .descendants()
                .filter(Node::is_text)
                .filter_map(|text| text.text())
                .collect::<String>();
            collector.push_math(&latex, display, style.size_scale);
            return;
        }
    }
    if name == "a" {
        let resolved =
            attribute_local(node, "href").and_then(|href| context.base.resolve(href).ok());
        style.link_role = context
            .footnote_links
            .get(&node.range().start)
            .copied()
            .unwrap_or_default();
        collect_inline_with_block_boundaries(
            node,
            style,
            resolved.as_ref().or(link),
            context,
            collector,
            preserve_block_boundaries,
        );
    } else {
        collect_inline_with_block_boundaries(
            node,
            style,
            link,
            context,
            collector,
            preserve_block_boundaries,
        );
    }
}

fn is_generic_block_container(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "center"
            | "div"
            | "figcaption"
            | "figure"
            | "footer"
            | "header"
            | "li"
            | "main"
            | "section"
    )
}

fn is_quote_text_candidate(node: Node<'_, '_>) -> bool {
    let has_text = node
        .descendants()
        .filter(Node::is_text)
        .filter_map(|text| text.text())
        .any(|text| !text.trim().is_empty());
    if !node.is_element()
        || !is_block_boundary(node.tag_name().name().to_ascii_lowercase().as_str())
        || !has_text
    {
        return false;
    }
    !node.descendants().skip(1).any(|descendant| {
        descendant.is_element()
            && is_block_boundary(descendant.tag_name().name().to_ascii_lowercase().as_str())
    })
}

fn has_quote_semantic_word(node: Node<'_, '_>) -> bool {
    attribute_local(node, "class").is_some_and(|classes| {
        classes
            .split_ascii_whitespace()
            .any(|class| class.to_ascii_lowercase().contains("quote"))
    })
}

fn effective_start_offset(style: BlockStyle) -> f32 {
    style
        .margin_start_fraction
        .mul_add(1_000.0, style.margin_start)
}

fn apply_default_blockquote_margin(style: &mut BlockStyle) {
    if effective_start_offset(*style).abs() <= f32::EPSILON {
        style.margin_start = 24.0;
    }
}

fn quote_source_range(body: &[TextBlock], attribution: Option<&TextBlock>) -> Option<SourceRange> {
    let start = body.first()?.source.as_ref()?.start.clone();
    let end = attribution
        .and_then(|block| block.source.as_ref())
        .or_else(|| body.last()?.source.as_ref())?
        .end
        .clone();
    Some(SourceRange { start, end })
}

fn should_suppress_navigation(node: Node<'_, '_>, styles: &StyleSheet) -> bool {
    let navigation_type_is_metadata = attribute_local(node, "type").is_some_and(|types| {
        types.split_ascii_whitespace().any(|navigation_type| {
            matches!(
                navigation_type.to_ascii_lowercase().as_str(),
                "landmarks" | "page-list"
            )
        })
    });
    let role_is_metadata = attribute_local(node, "role").is_some_and(|roles| {
        roles.split_ascii_whitespace().any(|role| {
            matches!(
                role.to_ascii_lowercase().as_str(),
                "doc-landmarks" | "doc-pagelist"
            )
        })
    });
    let explicitly_hidden = node.attribute("hidden").is_some()
        || attribute_local(node, "aria-hidden")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let properties = styles.cascaded_properties(node);
    let hidden_by_css = properties
        .get("display")
        .is_some_and(|value| value == "none")
        || properties
            .get("visibility")
            .is_some_and(|value| matches!(value.as_str(), "hidden" | "collapse"));

    navigation_type_is_metadata || role_is_metadata || explicitly_hidden || hidden_by_css
}

fn is_structured_container(node: Node<'_, '_>) -> bool {
    node.is_element()
        && matches!(
            node.tag_name().name().to_ascii_lowercase().as_str(),
            "ul" | "ol" | "dl"
        )
}

fn has_named_ancestor(node: Node<'_, '_>, root: Node<'_, '_>, name: &str) -> bool {
    node.ancestors()
        .skip(1)
        .take_while(|ancestor| ancestor != &root)
        .any(|ancestor| {
            ancestor.is_element() && ancestor.tag_name().name().eq_ignore_ascii_case(name)
        })
}

fn has_nested_structured_container_ancestor(node: Node<'_, '_>, root: Node<'_, '_>) -> bool {
    node.ancestors()
        .skip(1)
        .take_while(|ancestor| ancestor != &root)
        .any(is_structured_container)
}

fn table_span(node: Node<'_, '_>, name: &str) -> u16 {
    attribute_local(node, name)
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(1)
        .clamp(1, 64)
}

fn has_descendant_image(node: Node<'_, '_>) -> bool {
    node.descendants().skip(1).any(|descendant| {
        descendant.is_element()
            && matches!(
                descendant.tag_name().name().to_ascii_lowercase().as_str(),
                "img" | "image"
            )
    })
}

fn has_explicit_paragraph_list_marker(node: Node<'_, '_>) -> bool {
    let marker = node.descendants().find(|descendant| {
        descendant.is_element()
            && descendant.tag_name().name().eq_ignore_ascii_case("span")
            && attribute_local(*descendant, "class").is_some_and(|classes| {
                classes
                    .split_ascii_whitespace()
                    .any(|class| class.eq_ignore_ascii_case("enumerator"))
            })
    });
    let Some(marker) = marker else {
        return false;
    };
    let marker = marker
        .descendants()
        .filter(Node::is_text)
        .filter_map(|text| text.text())
        .collect::<String>();
    is_semantic_bullet(marker.trim())
}

fn is_semantic_bullet(marker: &str) -> bool {
    matches!(marker, "•" | "◦" | "▪" | "‣" | "»")
}

fn is_semantic_bullet_char(marker: char) -> bool {
    matches!(marker, '•' | '◦' | '▪' | '‣' | '»')
}

fn strip_authored_list_marker(content: &mut Vec<Inline>) {
    let Some(marker_index) = content.iter().position(|inline| match inline {
        Inline::Text(run) => run
            .text
            .trim_start()
            .chars()
            .next()
            .is_some_and(is_semantic_bullet_char),
        Inline::Math(_) | Inline::Image(_) | Inline::Break => false,
    }) else {
        return;
    };
    let Inline::Text(run) = &mut content[marker_index] else {
        return;
    };
    let trimmed = run.text.trim_start();
    let marker_len = trimmed.chars().next().map_or(0, char::len_utf8);
    run.text = trimmed[marker_len..].trim_start().to_owned();
    if run.text.is_empty() {
        content.remove(marker_index);
    }
}

fn contains_display_math(node: Node<'_, '_>) -> bool {
    node.descendants().skip(1).any(|descendant| {
        descendant.is_element()
            && descendant.tag_name().name().eq_ignore_ascii_case("span")
            && attribute_local(descendant, "class").is_some_and(|classes| {
                classes
                    .split_ascii_whitespace()
                    .any(|class| class == "math-display")
            })
    })
}

fn has_only_math_content(node: Node<'_, '_>) -> bool {
    node.descendants().skip(1).all(|descendant| {
        if descendant.is_text() {
            return descendant.text().is_none_or(|text| text.trim().is_empty())
                || descendant.parent().is_some_and(|parent| {
                    attribute_local(parent, "class").is_some_and(|classes| {
                        classes
                            .split_ascii_whitespace()
                            .any(|class| class == "math-display")
                    })
                });
        }
        !descendant.is_element()
            || (descendant.tag_name().name().eq_ignore_ascii_case("span")
                && attribute_local(descendant, "class").is_some_and(|classes| {
                    classes
                        .split_ascii_whitespace()
                        .any(|class| matches!(class, "math" | "math-display"))
                }))
    })
}

fn is_block_boundary(name: &str) -> bool {
    is_generic_block_container(name)
        || matches!(
            name,
            "blockquote"
                | "dd"
                | "dl"
                | "dt"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "hr"
                | "img"
                | "image"
                | "nav"
                | "ol"
                | "p"
                | "pre"
                | "table"
                | "ul"
        )
}
