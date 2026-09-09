fn classify_footnote_links(
    document: &Document<'_>,
    base: &PublicationUrl,
) -> HashMap<usize, LinkRole> {
    let anchors = document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name().eq_ignore_ascii_case("a"))
        .collect::<Vec<_>>();
    let mut roles = anchors
        .iter()
        .filter_map(|node| explicit_link_role(*node).map(|role| (node.range().start, role)))
        .collect::<HashMap<_, _>>();
    let by_fragment = document
        .descendants()
        .filter(Node::is_element)
        .filter_map(|node| node_fragment(node).map(|fragment| (fragment.to_owned(), node)))
        .collect::<HashMap<_, _>>();
    let targets = anchors
        .iter()
        .map(|node| attribute_local(*node, "href").and_then(|href| base.resolve(href).ok()))
        .collect::<Vec<_>>();

    for (index, node) in anchors.iter().copied().enumerate() {
        let Some(source_fragment) = footnote_reference_fragment(node) else {
            continue;
        };
        let Some(target) = targets[index].as_ref() else {
            continue;
        };
        if target.path() != base.path() {
            continue;
        }
        let Some(target_fragment) = target.fragment() else {
            continue;
        };
        let Some(&target_node) = by_fragment.get(target_fragment) else {
            continue;
        };
        let Some(counterpart) = target_node.descendants().find(|candidate| {
            if !candidate.is_element()
                || !candidate.tag_name().name().eq_ignore_ascii_case("a")
                || candidate.range().start == node.range().start
                || !matching_footnote_markers(node, *candidate)
            {
                return false;
            }
            attribute_local(*candidate, "href")
                .and_then(|href| base.resolve(href).ok())
                .is_some_and(|candidate_target| {
                    candidate_target.path() == base.path()
                        && candidate_target.fragment() == Some(source_fragment)
                })
        }) else {
            continue;
        };

        let node_has_prose_before = link_has_preceding_block_text(node);
        let counterpart_has_prose_before = link_has_preceding_block_text(counterpart);
        let (reference, backlink) = match (node_has_prose_before, counterpart_has_prose_before) {
            (true, false) => (node, counterpart),
            (false, true) => (counterpart, node),
            _ if node.range().start <= counterpart.range().start => (node, counterpart),
            _ => (counterpart, node),
        };
        roles
            .entry(reference.range().start)
            .or_insert(LinkRole::FootnoteReference);
        roles
            .entry(backlink.range().start)
            .or_insert(LinkRole::FootnoteBacklink);
    }
    roles
}

fn footnote_reference_fragment<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    if let Some(fragment) = node_fragment(node) {
        return Some(fragment);
    }
    let mut sibling = node.prev_sibling();
    while let Some(candidate) = sibling {
        if candidate.is_text() && candidate.text().is_some_and(|text| text.trim().is_empty()) {
            sibling = candidate.prev_sibling();
            continue;
        }
        let is_empty_anchor = candidate.is_element()
            && candidate.tag_name().name().eq_ignore_ascii_case("a")
            && attribute_local(candidate, "href").is_none()
            && node_text(candidate).trim().is_empty();
        return is_empty_anchor.then(|| node_fragment(candidate)).flatten();
    }
    None
}

fn explicit_link_role(node: Node<'_, '_>) -> Option<LinkRole> {
    let tokens = [
        attribute_local(node, "type"),
        attribute_local(node, "role"),
        attribute_local(node, "rel"),
    ];
    for token in tokens
        .into_iter()
        .flatten()
        .flat_map(str::split_ascii_whitespace)
    {
        match token.to_ascii_lowercase().as_str() {
            "noteref" | "doc-noteref" => return Some(LinkRole::FootnoteReference),
            "backlink" | "doc-backlink" => return Some(LinkRole::FootnoteBacklink),
            _ => {}
        }
    }
    None
}

fn is_semantic_footnote_definition(node: Node<'_, '_>) -> bool {
    [attribute_local(node, "type"), attribute_local(node, "role")]
        .into_iter()
        .flatten()
        .flat_map(str::split_ascii_whitespace)
        .any(|token| {
            matches!(
                token.to_ascii_lowercase().as_str(),
                "footnote" | "doc-footnote" | "endnote" | "doc-endnote"
            )
        })
}

fn node_fragment<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    attribute_local(node, "id")
        .or_else(|| attribute_local(node, "name"))
        .map(str::trim)
        .filter(|fragment| !fragment.is_empty())
}

fn matching_footnote_markers(left: Node<'_, '_>, right: Node<'_, '_>) -> bool {
    let marker = |node: Node<'_, '_>| {
        let text = node
            .descendants()
            .filter(Node::is_text)
            .filter_map(|text| text.text())
            .collect::<String>();
        normalize_footnote_marker(&text)
    };
    marker(left)
        .zip(marker(right))
        .is_some_and(|(left, right)| left.eq_ignore_ascii_case(&right))
}

fn normalize_footnote_marker(marker: &str) -> Option<String> {
    let marker = marker.trim().trim_end_matches(['.', '．']).trim();
    let marker = marker
        .trim_start_matches(['[', '(', '（', '【'])
        .trim_end_matches([']', ')', '）', '】'])
        .trim_end_matches(['.', '．'])
        .trim();
    let compact = !marker.is_empty()
        && marker.chars().count() <= 8
        && marker.chars().all(|character| {
            character.is_alphanumeric() || matches!(character, '*' | '†' | '‡' | '§')
        });
    compact.then(|| marker.to_owned())
}

fn link_has_preceding_block_text(link: Node<'_, '_>) -> bool {
    let Some(block) = link.ancestors().skip(1).find(|ancestor| {
        ancestor.is_element()
            && is_block_boundary(ancestor.tag_name().name().to_ascii_lowercase().as_str())
    }) else {
        return false;
    };
    block
        .descendants()
        .take_while(|descendant| descendant.range().start < link.range().start)
        .filter(Node::is_text)
        .filter_map(|text| text.text())
        .any(|text| !text.trim().is_empty())
}

pub fn parse_section(
    xml: &str,
    descriptor: &SpineItem,
    load_stylesheet: impl FnMut(&PublicationUrl) -> Option<String>,
) -> Result<Section, HtmlError> {
    parse_section_with_image_classifier(xml, descriptor, load_stylesheet, |_| false)
}

/// Parses one HTML section while allowing its publication container to identify decorative
/// image resources from intrinsic metadata that is unavailable to the HTML layer.
pub fn parse_section_with_image_classifier(
    xml: &str,
    descriptor: &SpineItem,
    load_stylesheet: impl FnMut(&PublicationUrl) -> Option<String>,
    is_decorative_separator_image: impl FnMut(&PublicationUrl) -> bool,
) -> Result<Section, HtmlError> {
    parse_section_with_hints_and_image_classifier(
        xml,
        descriptor,
        load_stylesheet,
        is_decorative_separator_image,
        SectionParseHints::default(),
    )
}

/// Parses one HTML section with publication-level semantic hints and image classification.
pub fn parse_section_with_hints_and_image_classifier(
    xml: &str,
    descriptor: &SpineItem,
    mut load_stylesheet: impl FnMut(&PublicationUrl) -> Option<String>,
    mut is_decorative_separator_image: impl FnMut(&PublicationUrl) -> bool,
    hints: SectionParseHints,
) -> Result<Section, HtmlError> {
    let document = Document::parse(xml).map_err(|error| HtmlError::InvalidDocument {
        resource: descriptor.href.to_string(),
        message: error.to_string(),
    })?;
    let styles = StyleSheet::from_document(&document, &descriptor.href, &mut load_stylesheet);
    let root = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "body")
        .unwrap_or_else(|| document.root_element());
    let footnote_links = classify_footnote_links(&document, &descriptor.href);
    let mut parser = ReadingIrParser::new(
        descriptor.id.clone(),
        descriptor.href.clone(),
        styles,
        footnote_links,
        &mut is_decorative_separator_image,
    );
    parser.queue_node_anchors(root);
    if hints.note_section {
        parser.parse_note_section_children(root)?;
    } else {
        parser.parse_children(root)?;
    }

    if parser.blocks.is_empty() && !parser.suppressed_content {
        let style = parser.styles.block_style(root, BlockStyle::default());
        parser.push_text_block(root, TextBlockKind::Paragraph, style)?;
    }

    Ok(Section {
        id: descriptor.id.clone(),
        href: descriptor.href.clone(),
        blocks: parser.blocks,
        anchors: parser.anchors,
    })
}

struct ReadingIrParser<'a> {
    section_id: SpineItemId,
    section_href: PublicationUrl,
    next_node: u64,
    blocks: Vec<Block>,
    anchors: Vec<SectionAnchor>,
    pending_anchors: Vec<String>,
    seen_anchors: HashSet<String>,
    styles: StyleSheet,
    footnote_links: HashMap<usize, LinkRole>,
    paragraph_list_indents: Vec<f32>,
    suppressed_content: bool,
    inside_quote: bool,
    inside_note_definition: bool,
    is_decorative_separator_image: &'a mut dyn FnMut(&PublicationUrl) -> bool,
}

impl<'a> ReadingIrParser<'a> {
    fn new(
        section_id: SpineItemId,
        section_href: PublicationUrl,
        styles: StyleSheet,
        footnote_links: HashMap<usize, LinkRole>,
        is_decorative_separator_image: &'a mut dyn FnMut(&PublicationUrl) -> bool,
    ) -> Self {
        Self {
            section_id,
            section_href,
            next_node: 0,
            blocks: Vec::new(),
            anchors: Vec::new(),
            pending_anchors: Vec::new(),
            seen_anchors: HashSet::new(),
            styles,
            footnote_links,
            paragraph_list_indents: Vec::new(),
            suppressed_content: false,
            inside_quote: false,
            inside_note_definition: false,
            is_decorative_separator_image,
        }
    }

    fn parse_children(&mut self, parent: Node<'_, '_>) -> Result<(), HtmlError> {
        let children = parent.children().collect::<Vec<_>>();
        let mut index = 0;
        while index < children.len() {
            let node = children[index];
            if node.is_element() {
                if let Some(caption_index) =
                    inferred_figure_caption_sibling(&children, index, &self.footnote_links)
                {
                    self.parse_inferred_figure_pair(node, children[caption_index])?;
                    index = caption_index + 1;
                    continue;
                }
                if note_section_starts_at(node, &children[index + 1..], &self.footnote_links) {
                    self.parse_note_section_nodes(&children[index..])?;
                    break;
                }
                if let Some(consumed) = self.try_parse_sibling_quote(parent, &children[index..])? {
                    index += consumed;
                    continue;
                }
                self.parse_node(node)?;
            }
            index += 1;
        }
        Ok(())
    }

    fn parse_inferred_figure_pair(
        &mut self,
        image: Node<'_, '_>,
        caption: Node<'_, '_>,
    ) -> Result<(), HtmlError> {
        let image_start = self.blocks.len();
        self.parse_node(image)?;
        let parsed_as_images = self.blocks.len() > image_start
            && self.blocks[image_start..]
                .iter()
                .all(|block| matches!(block, Block::Image(_)));

        let caption_start = self.blocks.len();
        self.parse_node(caption)?;
        if parsed_as_images
            && let [Block::Text(caption)] = &mut self.blocks[caption_start..]
            && caption.kind == TextBlockKind::Paragraph
        {
            caption.kind = TextBlockKind::Caption;
        }
        Ok(())
    }

    fn parse_note_section_children(&mut self, parent: Node<'_, '_>) -> Result<(), HtmlError> {
        let children = parent.children().collect::<Vec<_>>();
        self.parse_note_section_nodes(&children)
    }

    fn parse_note_section_nodes(&mut self, nodes: &[Node<'_, '_>]) -> Result<(), HtmlError> {
        let mut index = 0;
        while index < nodes.len() {
            let node = nodes[index];
            if !node.is_element() {
                index += 1;
                continue;
            }
            if note_entry_starts_at(node, &self.footnote_links, true) {
                let end = nodes[index + 1..]
                    .iter()
                    .position(|candidate| {
                        candidate.is_element()
                            && (note_entry_starts_at(*candidate, &self.footnote_links, true)
                                || is_note_subsection_heading(*candidate))
                    })
                    .map_or(nodes.len(), |offset| index + 1 + offset);
                self.parse_grouped_note_nodes(&nodes[index..end], NoteBlockKind::Section)?;
                index = end;
                continue;
            }
            let block_start = self.blocks.len();
            self.parse_node(node)?;
            self.wrap_blocks(block_start, NoteBlockKind::Section);
            index += 1;
        }
        Ok(())
    }

    fn parse_note_definition_nodes(&mut self, nodes: &[Node<'_, '_>]) -> Result<(), HtmlError> {
        self.parse_grouped_note_nodes(nodes, NoteBlockKind::Definition)
    }

    fn parse_grouped_note_nodes(
        &mut self,
        nodes: &[Node<'_, '_>],
        kind: NoteBlockKind,
    ) -> Result<(), HtmlError> {
        let block_start = self.blocks.len();
        let previous = self.inside_note_definition;
        self.inside_note_definition = true;
        for node in nodes.iter().copied().filter(Node::is_element) {
            if let Err(error) = self.parse_node(node) {
                self.inside_note_definition = previous;
                return Err(error);
            }
        }
        self.inside_note_definition = previous;
        self.wrap_blocks(block_start, kind);
        Ok(())
    }

    fn parse_implicit_note_container(&mut self, container: Node<'_, '_>) -> Result<(), HtmlError> {
        let nodes = container.children().collect::<Vec<_>>();
        let mut index = 0;
        while index < nodes.len() {
            let node = nodes[index];
            if !node.is_element() {
                index += 1;
                continue;
            }
            if note_entry_starts_at(node, &self.footnote_links, false) {
                let end = nodes[index + 1..]
                    .iter()
                    .position(|candidate| {
                        candidate.is_element()
                            && note_entry_starts_at(*candidate, &self.footnote_links, false)
                    })
                    .map_or(nodes.len(), |offset| index + 1 + offset);
                self.parse_note_definition_nodes(&nodes[index..end])?;
                index = end;
                continue;
            }
            self.parse_node(node)?;
            index += 1;
        }
        Ok(())
    }

    fn wrap_blocks(&mut self, block_start: usize, kind: NoteBlockKind) {
        let blocks = self.blocks.split_off(block_start);
        if blocks.is_empty() {
            return;
        }
        let source = combined_block_source(&blocks);
        self.blocks.push(Block::Note(NoteBlock {
            kind,
            blocks,
            source,
        }));
    }

    fn parse_block_container(&mut self, container: Node<'_, '_>) -> Result<(), HtmlError> {
        if self.try_parse_structural_quote(container)? {
            return Ok(());
        }
        let style = self.styles.block_style(container, BlockStyle::default());
        let text_style = self
            .styles
            .text_style_for_block(container, TextBlockKind::Paragraph);
        let mut collector = InlineCollector::new(false);

        let children = container.children().collect::<Vec<_>>();
        let mut index = 0;
        while index < children.len() {
            let child = children[index];
            if child.is_element()
                && (is_block_boundary(child.tag_name().name().to_ascii_lowercase().as_str())
                    || child.tag_name().name().eq_ignore_ascii_case("br"))
            {
                self.push_collected_text_block(
                    TextBlockKind::Paragraph,
                    style,
                    std::mem::replace(&mut collector, InlineCollector::new(false)),
                );
                if let Some(caption_index) =
                    inferred_figure_caption_sibling(&children, index, &self.footnote_links)
                {
                    self.parse_inferred_figure_pair(child, children[caption_index])?;
                    index = caption_index + 1;
                    continue;
                }
                if note_section_starts_at(child, &children[index + 1..], &self.footnote_links) {
                    self.parse_note_section_nodes(&children[index..])?;
                    break;
                }
                if let Some(consumed) =
                    self.try_parse_sibling_quote(container, &children[index..])?
                {
                    index += consumed;
                    continue;
                }
                self.parse_node(child)?;
                index += 1;
                continue;
            }
            if child.is_element() && has_descendant_image(child) {
                self.push_collected_text_block(
                    TextBlockKind::Paragraph,
                    style,
                    std::mem::replace(&mut collector, InlineCollector::new(false)),
                );
                self.queue_node_anchors(child);
                self.push_text_block(child, TextBlockKind::Paragraph, style)?;
                index += 1;
                continue;
            }
            if child.is_element() {
                self.queue_node_anchors(child);
                self.queue_descendant_anchors(child);
            }
            collect_inline_node(
                child,
                text_style,
                None,
                &InlineParseContext::new(&self.section_href, &self.styles, &self.footnote_links),
                &mut collector,
            );
            index += 1;
        }
        self.push_collected_text_block(TextBlockKind::Paragraph, style, collector);
        Ok(())
    }

    fn parse_footnote_definition(&mut self, note: Node<'_, '_>) -> Result<(), HtmlError> {
        let block_start = self.blocks.len();
        let previous = self.inside_note_definition;
        self.inside_note_definition = true;
        let result = self.parse_block_container(note);
        self.inside_note_definition = previous;
        result?;
        for block in &mut self.blocks[block_start..] {
            mark_block_as_footnote_definition(block);
        }
        self.wrap_blocks(block_start, NoteBlockKind::Definition);
        Ok(())
    }

    fn try_parse_sibling_quote(
        &mut self,
        container: Node<'_, '_>,
        siblings: &[Node<'_, '_>],
    ) -> Result<Option<usize>, HtmlError> {
        const MIN_ATTRIBUTED_BODY_BLOCKS: usize = 1;
        const MIN_UNATTRIBUTED_BODY_BLOCKS: usize = 2;

        let mut body = Vec::new();
        let mut reference_layout = None;
        let mut reference_text_style = None;
        let mut reference_tag = None::<String>;
        let mut stanza_break_after = Vec::new();
        let mut pending_stanza_break = None;
        let mut body_has_distinct_typography = false;
        let mut body_has_vertical_boundary = false;
        let mut last_body_consumed = 0;

        for (index, node) in siblings.iter().copied().enumerate() {
            if node.is_text() {
                if node.text().is_some_and(|text| text.trim().is_empty()) {
                    continue;
                }
                break;
            }
            if !node.is_element() {
                continue;
            }
            if node.tag_name().name().eq_ignore_ascii_case("br") && !body.is_empty() {
                pending_stanza_break = Some(body.len() - 1);
                continue;
            }
            if !is_quote_text_candidate(node) {
                break;
            }

            let block_style = self.styles.block_style(node, BlockStyle::default());
            if block_style.align == TextAlignment::End {
                if body.len() < MIN_ATTRIBUTED_BODY_BLOCKS
                    || !self.styles.has_sibling_quote_attribution_role(
                        node,
                        reference_layout.expect("quote body layout exists"),
                        reference_text_style.expect("quote body text style exists"),
                        body_has_vertical_boundary,
                    )
                {
                    break;
                }
                if let Some(previous) = pending_stanza_break.take()
                    && stanza_break_after.last().copied() != Some(previous)
                {
                    stanza_break_after.push(previous);
                }
                self.parse_quote_nodes_with_stanza_breaks(
                    container,
                    &body,
                    Some(node),
                    &stanza_break_after,
                )?;
                return Ok(Some(index + 1));
            }

            let Some(layout) = self.styles.grouped_quote_body_layout(node) else {
                break;
            };
            let text_style = self
                .styles
                .text_style_for_block(node, TextBlockKind::Paragraph);
            let tag = node.tag_name().name().to_ascii_lowercase();
            if let (Some(reference_layout), Some(reference_tag)) =
                (reference_layout, reference_tag.as_ref())
                && (tag != *reference_tag || !layout.compatible_with(reference_layout))
            {
                break;
            }

            if let Some(previous) = pending_stanza_break.take()
                && stanza_break_after.last().copied() != Some(previous)
            {
                stanza_break_after.push(previous);
            }
            reference_layout.get_or_insert(layout);
            reference_text_style.get_or_insert(text_style);
            reference_tag.get_or_insert(tag);
            body_has_distinct_typography |= self.styles.has_distinct_quote_typography(node);
            body_has_vertical_boundary |= layout.has_vertical_boundary();
            body.push(node);
            last_body_consumed = index + 1;
        }

        if body.len() >= MIN_UNATTRIBUTED_BODY_BLOCKS
            && body_has_distinct_typography
            && body_has_vertical_boundary
        {
            self.parse_quote_nodes_with_stanza_breaks(container, &body, None, &stanza_break_after)?;
            return Ok(Some(last_body_consumed));
        }

        Ok(None)
    }

    fn parse_node(&mut self, node: Node<'_, '_>) -> Result<(), HtmlError> {
        let name = node.tag_name().name().to_ascii_lowercase();
        if matches!(name.as_str(), "script" | "style" | "head") {
            return Ok(());
        }
        if name == "nav" && should_suppress_navigation(node, &self.styles) {
            self.suppressed_content = true;
            return Ok(());
        }
        if name != "p" {
            self.paragraph_list_indents.clear();
        }
        self.queue_node_anchors(node);
        if is_semantic_footnote_definition(node) {
            self.parse_footnote_definition(node)?;
            return Ok(());
        }
        if !self.inside_note_definition && is_implicit_note_container(node, &self.footnote_links) {
            self.parse_implicit_note_container(node)?;
            return Ok(());
        }
        if !self.inside_note_definition && note_entry_starts_at(node, &self.footnote_links, false) {
            self.parse_note_definition_nodes(&[node])?;
            return Ok(());
        }
        if matches!(name.as_str(), "p" | "div") && self.try_parse_symbol_separator(node)? {
            return Ok(());
        }
        match name.as_str() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = name[1..].parse::<u8>().unwrap_or(1);
                let style = self.styles.block_style(
                    node,
                    BlockStyle {
                        margin_before: 32.0,
                        margin_after: 8.0,
                        indent: 0.0,
                        line_height: 1.3,
                        ..BlockStyle::default()
                    },
                );
                self.push_text_block(node, TextBlockKind::Heading(level), style)?;
            }
            "p" => {
                if self.styles.has_standalone_quote_layout(node)
                    && (has_quote_semantic_word(node)
                        || self.styles.has_distinct_quote_typography(node))
                {
                    self.parse_standalone_quote(node)?;
                } else {
                    let mut style = self.styles.block_style(node, BlockStyle::default());
                    if !self.inside_quote && is_authored_spacing_paragraph(node) {
                        self.queue_descendant_anchors(node);
                        self.blocks
                            .push(Block::Separator(SeparatorBlock::spacing(style)));
                        return Ok(());
                    }
                    if contains_display_math(node) && has_only_math_content(node) {
                        style.align = TextAlignment::Center;
                        style.margin_before = style.margin_before.max(12.0);
                        style.margin_after = style.margin_after.max(12.0);
                    }
                    let has_marker = has_explicit_paragraph_list_marker(node);
                    let markerless_nested_item = !has_marker
                        && style.indent < -0.5
                        && self.paragraph_list_indents.first().is_some_and(|root| {
                            let indent = style
                                .margin_start_fraction
                                .mul_add(1_000.0, style.margin_start);
                            indent > *root + 4.0
                        });
                    let kind = if has_marker || markerless_nested_item {
                        TextBlockKind::ListItem {
                            ordered: false,
                            ordinal: 1,
                            depth: self.paragraph_list_depth(style),
                            marker_visible: has_marker,
                        }
                    } else {
                        self.paragraph_list_indents.clear();
                        TextBlockKind::Paragraph
                    };
                    if matches!(kind, TextBlockKind::ListItem { .. }) {
                        style.indent = 0.0;
                    }
                    self.push_text_block(node, kind, style)?;
                }
            }
            "blockquote" => self.parse_semantic_quote(node)?,
            "pre" => {
                let style = self.styles.block_style(
                    node,
                    BlockStyle {
                        line_height: 1.35,
                        ..BlockStyle::default()
                    },
                );
                self.push_text_block(node, TextBlockKind::Preformatted, style)?;
            }
            "table" => self.parse_table(node),
            "figure" => self.parse_figure(node)?,
            "nav" => self.parse_block_container(node)?,
            name if is_generic_block_container(name) => self.parse_block_container(node)?,
            "ul" => self.parse_list(node, false, 0)?,
            "ol" => self.parse_list(node, true, 0)?,
            "dl" => self.parse_definition_list(node, 0)?,
            "dt" => self.parse_definition_entry(node, true, 0)?,
            "dd" => self.parse_definition_entry(node, false, 0)?,
            "img" | "image" => self.push_image(node, None, true)?,
            "hr" => self.blocks.push(Block::Separator(if self.inside_quote {
                SeparatorBlock::rule_in_quote()
            } else {
                SeparatorBlock::rule()
            })),
            "br" => self.blocks.push(Block::LineBreak),
            _ => self.parse_children(node)?,
        }
        Ok(())
    }

    fn try_parse_symbol_separator(&mut self, node: Node<'_, '_>) -> Result<bool, HtmlError> {
        if self.inside_quote
            || self.inside_note_definition
            || has_quote_semantic_word(node)
            || node.ancestors().filter(Node::is_element).any(|ancestor| {
                matches!(
                    ancestor.tag_name().name(),
                    "blockquote"
                        | "figure"
                        | "figcaption"
                        | "table"
                        | "td"
                        | "th"
                        | "ul"
                        | "ol"
                        | "li"
                        | "dl"
                        | "dt"
                        | "dd"
                        | "pre"
                        | "code"
                        | "nav"
                        | "aside"
                )
            })
            || node
                .descendants()
                .skip(1)
                .filter(Node::is_element)
                .any(|child| {
                    matches!(
                        child.tag_name().name(),
                        "p" | "div"
                            | "br"
                            | "img"
                            | "image"
                            | "svg"
                            | "math"
                            | "a"
                            | "sup"
                            | "sub"
                            | "code"
                            | "table"
                    )
                })
        {
            return Ok(false);
        }
        let compact = node
            .descendants()
            .filter(Node::is_text)
            .filter_map(|n| n.text())
            .flat_map(str::chars)
            .filter(|c| !c.is_whitespace() && !matches!(c, '\u{200b}' | '\u{2060}' | '\u{feff}'))
            .take(33)
            .collect::<String>();
        let chars = compact.chars().collect::<Vec<_>>();
        if chars.is_empty() || chars.len() > 32 {
            return Ok(false);
        }
        let repeated = chars.len() >= 3
            && matches!(chars[0], '*' | '＊' | '※' | '•' | '⁂' | '⁎' | '✻' | '✽')
            && chars.iter().all(|c| *c == chars[0]);
        let weak = matches!(compact.as_str(), "▲" | "◆" | "❦" | "❧")
            || (chars.len() >= 3
                && chars
                    .iter()
                    .all(|c| matches!(c, '-' | '—' | '–' | '─' | '━' | '_')))
            || (compact.ends_with('▲')
                && chars.len() >= 4
                && chars[..chars.len() - 1]
                    .iter()
                    .all(|c| matches!(c, '-' | '—' | '─')));
        if !repeated && !weak {
            return Ok(false);
        }
        let style = self.styles.block_style(node, BlockStyle::default());
        if weak {
            let centered = style.align == TextAlignment::Center
                || node.children().filter(Node::is_element).any(|child| {
                    self.styles.block_style(child, BlockStyle::default()).align
                        == TextAlignment::Center
                });
            let previous = node.prev_siblings().skip(1).find(Node::is_element);
            let next = node.next_siblings().skip(1).find(Node::is_element);
            let prose = |neighbor: Option<Node<'_, '_>>| {
                neighbor.is_some_and(|n| {
                    n.tag_name().name() == "p"
                        && n.descendants()
                            .filter(Node::is_text)
                            .filter_map(|n| n.text())
                            .map(|s| s.chars().count())
                            .sum::<usize>()
                            >= 40
                })
            };
            let has_rule = previous.is_some_and(|n| n.tag_name().name() == "hr")
                || next.is_some_and(|n| n.tag_name().name() == "hr");
            if !centered
                || !((prose(previous)
                    && prose(next)
                    && (style.margin_before >= 8.0 || style.margin_after >= 8.0))
                    || has_rule)
            {
                return Ok(false);
            }
        }
        self.push_text_block(node, TextBlockKind::Paragraph, style)?;
        if let Some(Block::Text(text)) = self.blocks.last().cloned() {
            *self.blocks.last_mut().expect("symbol text exists") =
                Block::Separator(SeparatorBlock {
                    kind: rebook_publication::SeparatorKind::Symbols,
                    text: Some(text),
                    in_quote: false,
                    image: None,
                    style,
                });
            return Ok(true);
        }
        Ok(false)
    }

    fn try_parse_structural_quote(&mut self, container: Node<'_, '_>) -> Result<bool, HtmlError> {
        let children = container
            .children()
            .filter(Node::is_element)
            .collect::<Vec<_>>();
        if children.len() < 2
            || container.children().any(|child| {
                child.is_text() && child.text().is_some_and(|text| !text.trim().is_empty())
            })
            || children
                .iter()
                .any(|child| !is_quote_text_candidate(*child))
        {
            return Ok(false);
        }

        let attribution = *children.last().expect("quote candidates are non-empty");
        let attribution_style = self.styles.block_style(attribution, BlockStyle::default());
        if attribution_style.align != TextAlignment::End {
            return Ok(false);
        }
        let body = &children[..children.len() - 1];
        if body.iter().any(|node| {
            self.styles.block_style(*node, BlockStyle::default()).align == TextAlignment::End
        }) {
            return Ok(false);
        }

        let attribution_start = effective_start_offset(attribution_style);
        let body_has_role_style = body.iter().any(|node| {
            let style = self.styles.block_style(*node, BlockStyle::default());
            let text_style = self
                .styles
                .text_style_for_block(*node, TextBlockKind::Paragraph);
            text_style.italic || effective_start_offset(style) > attribution_start + 4.0
        });
        if !body_has_role_style || !self.styles.has_visual_boundary(container) {
            return Ok(false);
        }

        self.parse_quote_nodes(container, body, Some(attribution))?;
        Ok(true)
    }

    fn parse_semantic_quote(&mut self, quote: Node<'_, '_>) -> Result<(), HtmlError> {
        let children = quote
            .children()
            .filter(Node::is_element)
            .filter(|child| {
                is_quote_text_candidate(*child)
                    || matches!(
                        child.tag_name().name().to_ascii_lowercase().as_str(),
                        "cite" | "footer"
                    )
            })
            .collect::<Vec<_>>();
        if children.is_empty() {
            let start = self.blocks.len();
            let mut style = self.styles.block_style(quote, BlockStyle::default());
            apply_default_blockquote_margin(&mut style);
            let previously_inside_quote = self.inside_quote;
            self.inside_quote = true;
            let parse_result = self.push_text_block(quote, TextBlockKind::Blockquote, style);
            self.inside_quote = previously_inside_quote;
            parse_result?;
            let mut body = self
                .blocks
                .drain(start..)
                .filter_map(|block| match block {
                    Block::Text(block) => Some(block),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if let Some(source) = quote_source_range(&body, None) {
                self.blocks.push(Block::Quote(QuoteBlock {
                    body: std::mem::take(&mut body),
                    attribution: None,
                    source: Some(source),
                }));
            }
            return Ok(());
        }

        let last = *children.last().expect("semantic quote has children");
        let last_name = last.tag_name().name().to_ascii_lowercase();
        let last_is_attribution = matches!(last_name.as_str(), "cite" | "footer")
            || (children.len() > 1
                && self.styles.block_style(last, BlockStyle::default()).align
                    == TextAlignment::End);
        let (body, attribution) = if last_is_attribution {
            (&children[..children.len() - 1], Some(last))
        } else {
            (children.as_slice(), None)
        };
        self.parse_quote_nodes(quote, body, attribution)
    }

    fn parse_standalone_quote(&mut self, node: Node<'_, '_>) -> Result<(), HtmlError> {
        let start = self.blocks.len();
        let style = self.styles.block_style(node, BlockStyle::default());
        let previously_inside_quote = self.inside_quote;
        self.inside_quote = true;
        let parse_result = self.push_text_block(node, TextBlockKind::Blockquote, style);
        self.inside_quote = previously_inside_quote;
        parse_result?;
        let mut body = self
            .blocks
            .drain(start..)
            .filter_map(|block| match block {
                Block::Text(block) => Some(block),
                _ => None,
            })
            .collect::<Vec<_>>();
        if let Some(source) = quote_source_range(&body, None) {
            self.blocks.push(Block::Quote(QuoteBlock {
                body: std::mem::take(&mut body),
                attribution: None,
                source: Some(source),
            }));
        }
        Ok(())
    }

    fn parse_quote_nodes(
        &mut self,
        container: Node<'_, '_>,
        body_nodes: &[Node<'_, '_>],
        attribution_node: Option<Node<'_, '_>>,
    ) -> Result<(), HtmlError> {
        self.parse_quote_nodes_with_stanza_breaks(container, body_nodes, attribution_node, &[])
    }

    fn parse_quote_nodes_with_stanza_breaks(
        &mut self,
        container: Node<'_, '_>,
        body_nodes: &[Node<'_, '_>],
        attribution_node: Option<Node<'_, '_>>,
        stanza_break_after: &[usize],
    ) -> Result<(), HtmlError> {
        let start = self.blocks.len();
        let previously_inside_quote = self.inside_quote;
        self.inside_quote = true;
        let parse_result: Result<(), HtmlError> = (|| {
            for node in body_nodes.iter().copied().chain(attribution_node) {
                // The enclosing structure has already established the quote roles. Parsing the
                // child through `parse_node` would run standalone quote detection again and turn a
                // class such as `prosequote1` into a nested Quote, which drops its sibling source
                // when the outer quote collects text blocks.
                self.queue_node_anchors(node);
                let style = self.styles.block_style(node, BlockStyle::default());
                self.push_text_block(node, TextBlockKind::Paragraph, style)?;
            }
            Ok(())
        })();
        self.inside_quote = previously_inside_quote;
        parse_result?;
        let mut parsed = self.blocks.drain(start..).collect::<Vec<_>>();
        let attribution = attribution_node.and_then(|node| match parsed.pop() {
            Some(Block::Text(mut block)) => {
                block.kind = TextBlockKind::QuoteAttribution;
                if node.tag_name().name().eq_ignore_ascii_case("cite") {
                    for inline in &mut block.content {
                        if let Inline::Text(run) = inline {
                            run.style.italic = true;
                            run.style.citation = true;
                        }
                    }
                }
                Some(block)
            }
            Some(block) => {
                self.blocks.push(block);
                None
            }
            None => None,
        });
        let semantic_blockquote = container
            .tag_name()
            .name()
            .eq_ignore_ascii_case("blockquote");
        let mut body = parsed
            .into_iter()
            .filter_map(|block| match block {
                Block::Text(mut block) => {
                    block.kind = TextBlockKind::Blockquote;
                    if semantic_blockquote {
                        apply_default_blockquote_margin(&mut block.style);
                    }
                    Some(block)
                }
                block => {
                    self.blocks.push(block);
                    None
                }
            })
            .collect::<Vec<_>>();
        if body.is_empty() {
            return Ok(());
        }
        for index in stanza_break_after {
            if let Some(block) = body.get_mut(*index) {
                block.style.hard_break_after = true;
            }
        }
        let source = quote_source_range(&body, attribution.as_ref());
        self.blocks.push(Block::Quote(QuoteBlock {
            body: std::mem::take(&mut body),
            attribution,
            source,
        }));
        Ok(())
    }

    fn paragraph_list_depth(&mut self, style: BlockStyle) -> u8 {
        const INDENT_TOLERANCE: f32 = 4.0;
        const FRACTION_REFERENCE_WIDTH: f32 = 1_000.0;

        let indent = style
            .margin_start_fraction
            .mul_add(FRACTION_REFERENCE_WIDTH, style.margin_start);
        let Some(previous) = self.paragraph_list_indents.last().copied() else {
            self.paragraph_list_indents.push(indent);
            return 0;
        };

        if indent > previous + INDENT_TOLERANCE {
            self.paragraph_list_indents.push(indent);
        } else if let Some(level) = self
            .paragraph_list_indents
            .iter()
            .rposition(|known| (indent - *known).abs() <= INDENT_TOLERANCE)
        {
            self.paragraph_list_indents.truncate(level + 1);
        } else if let Some(parent) = self
            .paragraph_list_indents
            .iter()
            .rposition(|known| *known < indent)
        {
            self.paragraph_list_indents.truncate(parent + 1);
            self.paragraph_list_indents.push(indent);
        } else {
            self.paragraph_list_indents.clear();
            self.paragraph_list_indents.push(indent);
        }

        u8::try_from(self.paragraph_list_indents.len().saturating_sub(1)).unwrap_or(u8::MAX)
    }

    fn parse_list(
        &mut self,
        list: Node<'_, '_>,
        ordered: bool,
        depth: u8,
    ) -> Result<(), HtmlError> {
        let mut ordinal = 1_u32;
        for item in list
            .children()
            .filter(|node| node.is_element() && node.tag_name().name().eq_ignore_ascii_case("li"))
        {
            self.queue_node_anchors(item);
            let mut style = self.styles.block_style(item, BlockStyle::default());
            style.indent = 0.0;
            style.margin_start = style.margin_start.max(24.0 * (f32::from(depth) + 1.0));
            self.push_structured_item(
                item,
                TextBlockKind::ListItem {
                    ordered,
                    ordinal,
                    depth,
                    marker_visible: true,
                },
                style,
            )?;
            self.parse_nested_structured_containers(item, depth.saturating_add(1))?;
            ordinal = ordinal.saturating_add(1);
        }
        Ok(())
    }

    fn parse_definition_list(&mut self, list: Node<'_, '_>, depth: u8) -> Result<(), HtmlError> {
        for entry in list.children().filter(Node::is_element) {
            match entry.tag_name().name().to_ascii_lowercase().as_str() {
                "dt" => self.parse_definition_entry(entry, true, depth)?,
                "dd" => self.parse_definition_entry(entry, false, depth)?,
                _ => self.parse_node(entry)?,
            }
        }
        Ok(())
    }

    fn parse_definition_entry(
        &mut self,
        entry: Node<'_, '_>,
        term: bool,
        depth: u8,
    ) -> Result<(), HtmlError> {
        self.queue_node_anchors(entry);
        let mut style = self.styles.block_style(entry, BlockStyle::default());
        style.indent = 0.0;
        let semantic_indent = 24.0 * (f32::from(depth) + if term { 0.0 } else { 1.0 });
        style.margin_start = style.margin_start.max(semantic_indent);
        let kind = if term {
            TextBlockKind::DefinitionTerm { depth }
        } else {
            TextBlockKind::DefinitionDescription { depth }
        };
        self.push_structured_item(entry, kind, style)?;
        self.parse_nested_structured_containers(entry, depth.saturating_add(1))
    }

    fn parse_nested_structured_containers(
        &mut self,
        parent: Node<'_, '_>,
        depth: u8,
    ) -> Result<(), HtmlError> {
        for nested in parent
            .children()
            .filter(|child| is_structured_container(*child))
        {
            self.queue_node_anchors(nested);
            match nested.tag_name().name().to_ascii_lowercase().as_str() {
                "ol" => self.parse_list(nested, true, depth)?,
                "ul" => self.parse_list(nested, false, depth)?,
                "dl" => self.parse_definition_list(nested, depth)?,
                _ => unreachable!("filtered structured container"),
            }
        }
        Ok(())
    }

    fn push_structured_item(
        &mut self,
        node: Node<'_, '_>,
        kind: TextBlockKind,
        style: BlockStyle,
    ) -> Result<(), HtmlError> {
        for descendant in node.descendants().skip(1).filter(Node::is_element) {
            if !has_nested_structured_container_ancestor(descendant, node) {
                self.queue_node_anchors(descendant);
            }
        }

        let text_style = self.styles.text_style_for_block(node, kind);
        let inline_images = node_has_visible_text(node);
        let mut collector = InlineCollector::new(false);
        for child in node.children() {
            if is_structured_container(child) {
                continue;
            }
            collect_inline_node(
                child,
                text_style,
                None,
                &InlineParseContext::new(&self.section_href, &self.styles, &self.footnote_links)
                    .with_inline_images(inline_images),
                &mut collector,
            );
        }
        collector.finish();
        let has_text = !collector.content.is_empty();
        self.push_collected_text_block(kind, style, collector);

        let images = node
            .descendants()
            .filter(|descendant| {
                descendant != &node
                    && descendant.is_element()
                    && matches!(
                        descendant.tag_name().name().to_ascii_lowercase().as_str(),
                        "img" | "image"
                    )
                    && !has_nested_structured_container_ancestor(*descendant, node)
                    && !image_is_footnote_reference(*descendant, &self.footnote_links)
                    && (!inline_images || self.styles.image_establishes_block_layout(*descendant))
            })
            .collect::<Vec<_>>();
        let image_count = images.len();
        for (index, image) in images.into_iter().enumerate() {
            let container_style = (!has_text).then_some((
                if index == 0 { style.margin_before } else { 0.0 },
                if index + 1 == image_count {
                    style.margin_after
                } else {
                    0.0
                },
            ));
            self.push_image(image, container_style, false)?;
        }
        Ok(())
    }

    fn parse_table(&mut self, table: Node<'_, '_>) {
        let table_node = self.allocate_node();
        let table_source = self.source_range(&table_node, 0);
        self.bind_pending_anchors(&table_source.start);
        let mut rows = Vec::new();
        for row in table.descendants().filter(|node| {
            node.is_element()
                && node.tag_name().name().eq_ignore_ascii_case("tr")
                && node.ancestors().skip(1).find(|ancestor| {
                    ancestor.is_element()
                        && ancestor.tag_name().name().eq_ignore_ascii_case("table")
                }) == Some(table)
        }) {
            let mut cells = Vec::new();
            for cell in row.children().filter(|node| {
                node.is_element()
                    && matches!(
                        node.tag_name().name().to_ascii_lowercase().as_str(),
                        "td" | "th"
                    )
            }) {
                self.queue_node_anchors(cell);
                let header = cell.tag_name().name().eq_ignore_ascii_case("th");
                let mut style = self.styles.block_style(cell, BlockStyle::default());
                style.indent = 0.0;
                style.margin_before = 0.0;
                style.margin_after = 0.0;
                style.line_height = style.line_height.clamp(1.0, 1.5);
                let mut text_style = self
                    .styles
                    .text_style_for_block(cell, TextBlockKind::Paragraph);
                if header {
                    text_style.bold = true;
                }
                let mut collector = InlineCollector::new(false);
                collect_table_cell_inline(
                    cell,
                    text_style,
                    None,
                    &InlineParseContext::new(
                        &self.section_href,
                        &self.styles,
                        &self.footnote_links,
                    )
                    .with_inline_images(node_has_visible_text(cell)),
                    &mut collector,
                );
                collector.finish();
                let text_len = collector
                    .content
                    .iter()
                    .map(|inline| match inline {
                        Inline::Text(run) => run.text.chars().count() as u64,
                        Inline::Math(_) => 0,
                        Inline::Image(_) => 0,
                        Inline::Break => 1,
                    })
                    .sum();
                let node_id = self.allocate_node();
                let source = self.source_range(&node_id, text_len);
                self.bind_pending_anchors(&source.start);
                cells.push(TableCell {
                    text: TextBlock {
                        kind: TextBlockKind::Paragraph,
                        content: collector.content,
                        style,
                        source: Some(source),
                    },
                    authored_alignment: self.styles.table_cell_alignment(cell),
                    column_span: table_span(cell, "colspan"),
                    row_span: table_span(cell, "rowspan"),
                    header,
                });
            }
            if !cells.is_empty() {
                rows.push(TableRow { cells });
            }
        }
        if !rows.is_empty() {
            self.blocks.push(Block::Table(TableBlock {
                rows,
                source: Some(table_source),
            }));
        }
    }

    fn parse_figure(&mut self, figure: Node<'_, '_>) -> Result<(), HtmlError> {
        let caption_nodes = figure
            .descendants()
            .skip(1)
            .filter(|node| {
                node.is_element()
                    && node.tag_name().name().eq_ignore_ascii_case("figcaption")
                    && !has_named_ancestor(*node, figure, "figure")
            })
            .collect::<Vec<_>>();
        let image_nodes = figure
            .descendants()
            .skip(1)
            .filter(|node| {
                node.is_element()
                    && matches!(
                        node.tag_name().name().to_ascii_lowercase().as_str(),
                        "img" | "image"
                    )
                    && !has_named_ancestor(*node, figure, "figure")
                    && !has_named_ancestor(*node, figure, "figcaption")
            })
            .collect::<Vec<_>>();
        let unsupported_caption = caption_nodes.iter().any(|caption| {
            caption.descendants().skip(1).any(|node| {
                node.is_element()
                    && matches!(
                        node.tag_name().name().to_ascii_lowercase().as_str(),
                        "figure" | "img" | "image" | "table"
                    )
            })
        });
        if image_nodes.is_empty() || unsupported_caption {
            return self.parse_block_container(figure);
        }

        let figure_node = self.allocate_node();
        let figure_source = self.source_range(&figure_node, 0);
        self.bind_pending_anchors(&figure_source.start);
        let caption_position = figure
            .descendants()
            .skip(1)
            .find_map(|node| {
                if !node.is_element() || has_named_ancestor(node, figure, "figure") {
                    return None;
                }
                match node.tag_name().name().to_ascii_lowercase().as_str() {
                    "figcaption" => Some(CaptionPosition::Before),
                    "img" | "image" => Some(CaptionPosition::After),
                    _ => None,
                }
            })
            .unwrap_or_default();

        let mut images = Vec::with_capacity(image_nodes.len());
        for image in image_nodes {
            self.queue_node_anchors(image);
            self.queue_descendant_anchors(image);
            let source = images.is_empty().then(|| figure_source.clone());
            if let Some(image) = self.image_block(image, None, source)? {
                images.push(image);
            }
        }
        if images.is_empty() {
            return Ok(());
        }

        let mut captions = Vec::new();
        for caption in caption_nodes {
            self.queue_node_anchors(caption);
            let block_start = self.blocks.len();
            self.parse_block_container(caption)?;
            for block in self.blocks.drain(block_start..) {
                if let Block::Text(mut caption) = block {
                    caption.kind = TextBlockKind::Caption;
                    captions.push(caption);
                }
            }
        }
        self.blocks.push(Block::Figure(FigureBlock {
            images,
            captions,
            caption_position,
            style: self.styles.block_style(figure, BlockStyle::default()),
            source: Some(figure_source),
        }));
        Ok(())
    }

    fn push_text_block(
        &mut self,
        node: Node<'_, '_>,
        kind: TextBlockKind,
        mut style: BlockStyle,
    ) -> Result<(), HtmlError> {
        let node_id = self.allocate_node();
        self.queue_descendant_anchors(node);
        if let Some(alignment) = self.styles.sole_content_block_alignment(node) {
            style.align = alignment;
            style.authored_alignment = Some(alignment);
        }
        // HTML images are inline replaced elements by default. Keep them in the
        // authored text flow whenever this container also carries prose; an
        // image-only paragraph remains a semantic block even when the resource
        // itself is only a few pixels high (a common display-equation pattern).
        let inline_images = node_has_visible_text(node);
        let mut collector = InlineCollector::new(matches!(kind, TextBlockKind::Preformatted));
        collect_inline(
            node,
            self.styles.text_style_for_block(node, kind),
            None,
            &InlineParseContext::new(&self.section_href, &self.styles, &self.footnote_links)
                .with_inline_images(inline_images),
            &mut collector,
        );
        collector.finish();
        if node.tag_name().name().eq_ignore_ascii_case("p")
            && matches!(kind, TextBlockKind::ListItem { .. })
        {
            strip_authored_list_marker(&mut collector.content);
        }
        let text_len = collector
            .content
            .iter()
            .map(|inline| match inline {
                Inline::Text(run) => run.text.chars().count() as u64,
                Inline::Math(_) => 0,
                Inline::Image(_) => 0,
                Inline::Break => 1,
            })
            .sum();
        if !collector.content.is_empty() {
            let source = self.source_range(&node_id, text_len);
            self.bind_pending_anchors(&source.start);
            self.blocks.push(Block::Text(TextBlock {
                kind,
                content: collector.content,
                style,
                source: Some(source),
            }));
        }

        let images = node
            .descendants()
            .filter(|descendant| {
                descendant != &node
                    && descendant.is_element()
                    && matches!(
                        descendant.tag_name().name().to_ascii_lowercase().as_str(),
                        "img" | "image"
                    )
                    && !image_is_footnote_reference(*descendant, &self.footnote_links)
                    && (!inline_images || self.styles.image_establishes_block_layout(*descendant))
            })
            .collect::<Vec<_>>();
        let image_count = images.len();
        for (index, image) in images.into_iter().enumerate() {
            let container_style = (text_len == 0).then_some((
                if index == 0 { style.margin_before } else { 0.0 },
                if index + 1 == image_count {
                    style.margin_after
                } else {
                    0.0
                },
            ));
            self.push_image(image, container_style, text_len == 0 && image_count == 1)?;
        }
        Ok(())
    }

    fn push_collected_text_block(
        &mut self,
        kind: TextBlockKind,
        style: BlockStyle,
        mut collector: InlineCollector,
    ) {
        collector.finish();
        let text_len = collector
            .content
            .iter()
            .map(|inline| match inline {
                Inline::Text(run) => run.text.chars().count() as u64,
                Inline::Math(_) => 0,
                Inline::Image(_) => 0,
                Inline::Break => 1,
            })
            .sum();
        if collector.content.is_empty() {
            return;
        }
        let node_id = self.allocate_node();
        let source = self.source_range(&node_id, text_len);
        self.bind_pending_anchors(&source.start);
        self.blocks.push(Block::Text(TextBlock {
            kind,
            content: collector.content,
            style,
            source: Some(source),
        }));
    }

    fn push_image(
        &mut self,
        node: Node<'_, '_>,
        container_style: Option<(f32, f32)>,
        allow_separator: bool,
    ) -> Result<(), HtmlError> {
        if let Some(image) = self.image_block(node, container_style, None)? {
            let generic_alt = image.alt.trim();
            let generic_alt = generic_alt.is_empty()
                || generic_alt.eq_ignore_ascii_case("image")
                || generic_alt.eq_ignore_ascii_case("ornament")
                || generic_alt.eq_ignore_ascii_case("separator")
                || generic_alt.eq_ignore_ascii_case("divider");
            if allow_separator
                && !self.inside_quote
                && generic_alt
                && !node.ancestors().any(|ancestor| {
                    ancestor.is_element()
                        && ancestor.tag_name().name().eq_ignore_ascii_case("a")
                        && attribute_local(ancestor, "href").is_some()
                })
                && (self.is_decorative_separator_image)(&image.href)
            {
                self.blocks
                    .push(Block::Separator(SeparatorBlock::ornament(image)));
            } else {
                self.blocks.push(Block::Image(image));
            }
        }
        Ok(())
    }

    fn image_block(
        &mut self,
        node: Node<'_, '_>,
        container_style: Option<(f32, f32)>,
        source: Option<SourceRange>,
    ) -> Result<Option<ImageBlock>, HtmlError> {
        let Some(src) = attribute_local(node, "src")
            .or_else(|| attribute_local(node, "href"))
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(None);
        };
        let href = self.section_href.resolve(src)?.resource_url();
        let source = source.unwrap_or_else(|| {
            let node_id = self.allocate_node();
            self.source_range(&node_id, 0)
        });
        self.bind_pending_anchors(&source.start);
        let mut style = self.styles.image_style(node);
        if let Some((margin_before, margin_after)) = container_style {
            style.margin_before = style.margin_before.max(margin_before);
            style.margin_after = style.margin_after.max(margin_after);
        }
        Ok(Some(ImageBlock {
            href,
            alt: attribute_local(node, "alt").unwrap_or_default().to_owned(),
            style,
            source: Some(source),
            text_layer: None,
        }))
    }

    fn queue_descendant_anchors(&mut self, node: Node<'_, '_>) {
        for descendant in node.descendants().skip(1).filter(Node::is_element) {
            self.queue_node_anchors(descendant);
        }
    }

    fn queue_node_anchors(&mut self, node: Node<'_, '_>) {
        for fragment in [attribute_local(node, "id"), attribute_local(node, "name")]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|fragment| !fragment.is_empty())
        {
            if self.seen_anchors.insert(fragment.to_owned()) {
                self.pending_anchors.push(fragment.to_owned());
            }
        }
    }

    fn bind_pending_anchors(&mut self, source: &SourceAnchor) {
        self.anchors.extend(
            self.pending_anchors
                .drain(..)
                .map(|fragment| SectionAnchor {
                    fragment,
                    source: source.clone(),
                }),
        );
    }

    fn allocate_node(&mut self) -> String {
        let id = format!("n{}", self.next_node);
        self.next_node = self.next_node.saturating_add(1);
        id
    }

    fn source_range(&self, node: &str, text_len: u64) -> SourceRange {
        SourceRange {
            start: SourceAnchor {
                spine: self.section_id.clone(),
                node: node.to_owned(),
                text_offset: 0,
            },
            end: SourceAnchor {
                spine: self.section_id.clone(),
                node: node.to_owned(),
                text_offset: text_len,
            },
        }
    }
}

fn mark_block_as_footnote_definition(block: &mut Block) {
    let mark = |text: &mut TextBlock| text.kind = TextBlockKind::FootnoteDefinition;
    match block {
        Block::Text(text) => mark(text),
        Block::Quote(quote) => {
            quote.body.iter_mut().for_each(mark);
            quote.attribution.iter_mut().for_each(mark);
        }
        Block::Table(table) => table
            .rows
            .iter_mut()
            .flat_map(|row| &mut row.cells)
            .for_each(|cell| mark(&mut cell.text)),
        Block::Figure(figure) => figure.captions.iter_mut().for_each(mark),
        Block::Note(note) => note
            .blocks
            .iter_mut()
            .for_each(mark_block_as_footnote_definition),
        Block::Image(_) | Block::Separator(_) | Block::LineBreak | Block::PageBreak => {}
    }
}

fn combined_block_source(blocks: &[Block]) -> Option<SourceRange> {
    let first = blocks.iter().find_map(block_source_range)?.start.clone();
    let last = blocks
        .iter()
        .rev()
        .find_map(block_source_range)?
        .end
        .clone();
    Some(SourceRange {
        start: first,
        end: last,
    })
}

fn block_source_range(block: &Block) -> Option<&SourceRange> {
    match block {
        Block::Text(text) => text.source.as_ref(),
        Block::Quote(quote) => quote.source.as_ref(),
        Block::Table(table) => table.source.as_ref(),
        Block::Image(image) => image.source.as_ref(),
        Block::Figure(figure) => figure.source.as_ref(),
        Block::Note(note) => note.source.as_ref(),
        Block::Separator(_) | Block::LineBreak | Block::PageBreak => None,
    }
}

fn inferred_figure_caption_sibling(
    siblings: &[Node<'_, '_>],
    image_index: usize,
    footnote_links: &HashMap<usize, LinkRole>,
) -> Option<usize> {
    let image = *siblings.get(image_index)?;
    if !is_captionable_image_container(image, footnote_links) {
        return None;
    }
    for (index, sibling) in siblings.iter().copied().enumerate().skip(image_index + 1) {
        if sibling.is_text() {
            if sibling.text().is_some_and(|text| !text.trim().is_empty()) {
                return None;
            }
            continue;
        }
        if !sibling.is_element() {
            continue;
        }
        return is_inferred_figure_caption(sibling).then_some(index);
    }
    None
}

fn is_captionable_image_container(
    node: Node<'_, '_>,
    footnote_links: &HashMap<usize, LinkRole>,
) -> bool {
    if !node.is_element()
        || node
            .descendants()
            .filter(Node::is_text)
            .filter_map(|text| text.text())
            .any(|text| !text.trim().is_empty())
    {
        return false;
    }

    let images = node
        .descendants()
        .filter(|descendant| {
            descendant.is_element()
                && matches!(
                    descendant.tag_name().name().to_ascii_lowercase().as_str(),
                    "img" | "image"
                )
        })
        .collect::<Vec<_>>();
    !images.is_empty()
        && images
            .iter()
            .all(|image| !image_is_footnote_reference(*image, footnote_links))
        && !node.descendants().skip(1).any(|descendant| {
            descendant.is_element()
                && matches!(
                    descendant.tag_name().name().to_ascii_lowercase().as_str(),
                    "figcaption" | "table"
                )
        })
}

fn is_inferred_figure_caption(node: Node<'_, '_>) -> bool {
    if !node.is_element()
        || !matches!(
            node.tag_name().name().to_ascii_lowercase().as_str(),
            "p" | "div"
        )
        || node.descendants().skip(1).any(|descendant| {
            descendant.is_element()
                && matches!(
                    descendant.tag_name().name().to_ascii_lowercase().as_str(),
                    "div" | "figure" | "figcaption" | "img" | "image" | "table"
                )
        })
    {
        return false;
    }

    let text = node_text(node);
    !text.trim().is_empty()
        && (has_caption_semantic_attribute(node) || starts_with_numbered_caption_label(&text))
}

fn has_caption_semantic_attribute(node: Node<'_, '_>) -> bool {
    [
        attribute_local(node, "class"),
        attribute_local(node, "type"),
        attribute_local(node, "role"),
    ]
    .into_iter()
    .flatten()
    .flat_map(str::split_whitespace)
    .map(|token| {
        token
            .chars()
            .filter(|character| !matches!(character, '-' | '_'))
            .collect::<String>()
            .to_ascii_lowercase()
    })
    .any(|token| {
        matches!(
            token.as_str(),
            "caption"
                | "fcaption"
                | "figcaption"
                | "figurecaption"
                | "doccaption"
                | "legend"
                | "finure"
                | "tushuo"
        )
    })
}

fn starts_with_numbered_caption_label(text: &str) -> bool {
    let text = text.trim_start_matches(|character: char| {
        character.is_whitespace()
            || matches!(
                character,
                '▲' | '△' | '◆' | '◇' | '■' | '□' | '●' | '○' | '※'
            )
    });

    for label in ["图片", "图表", "插图", "图版", "表格", "图", "表"] {
        if let Some(rest) = text.strip_prefix(label) {
            return starts_with_caption_identifier(rest.trim_start());
        }
    }

    let lower = text.to_ascii_lowercase();
    for label in [
        "illustration",
        "figure",
        "table",
        "plate",
        "chart",
        "exhibit",
        "fig",
    ] {
        if lower.starts_with(label) {
            let mut rest = &text[label.len()..];
            if label == "fig" {
                rest = rest.strip_prefix('.').unwrap_or(rest);
            }
            return starts_with_caption_identifier(rest.trim_start());
        }
    }
    false
}

fn starts_with_caption_identifier(text: &str) -> bool {
    text.chars().next().is_some_and(|character| {
        character.is_ascii_digit()
            || character.is_ascii_uppercase()
            || matches!(
                character,
                '一' | '二'
                    | '三'
                    | '四'
                    | '五'
                    | '六'
                    | '七'
                    | '八'
                    | '九'
                    | '十'
                    | '百'
                    | '千'
                    | '零'
                    | '〇'
            )
    })
}

fn is_note_section_heading(node: Node<'_, '_>) -> bool {
    let name = node.tag_name().name().to_ascii_lowercase();
    matches!(name.as_str(), "h1" | "h2" | "h3" | "p") && is_note_section_label(&node_text(node))
}

fn node_text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(Node::is_text)
        .filter_map(|text| text.text())
        .collect()
}

fn starts_with_attribution_marker(node: Node<'_, '_>) -> bool {
    let text = node_text(node);
    let text = text.trim_start();
    matches!(text.chars().next(), Some('—' | '–' | '―')) || text.starts_with("--")
}

fn first_visible_text_has_attribution_markup(node: Node<'_, '_>) -> bool {
    let Some(first_text) = node.descendants().find(|descendant| {
        descendant.is_text()
            && descendant
                .text()
                .is_some_and(|text| !text.trim().is_empty())
    }) else {
        return false;
    };
    first_text
        .ancestors()
        .take_while(|ancestor| *ancestor != node)
        .filter(Node::is_element)
        .any(|ancestor| {
            matches!(
                ancestor.tag_name().name().to_ascii_lowercase().as_str(),
                "cite" | "em" | "i"
            )
        })
}

fn is_note_section_label(label: &str) -> bool {
    let label = label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches([':', '：'])
        .to_owned();
    matches!(
        label.to_ascii_lowercase().as_str(),
        "note"
            | "notes"
            | "endnote"
            | "endnotes"
            | "footnote"
            | "footnotes"
            | "注释"
            | "注解"
            | "尾注"
            | "本章注"
            | "章节注释"
            | "作者附注"
    )
}

fn note_section_extends_to_container_end(heading: Node<'_, '_>, suffix: &[Node<'_, '_>]) -> bool {
    let level = heading
        .tag_name()
        .name()
        .get(1..)
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(1);
    let has_content = suffix.iter().any(|node| {
        node.is_element()
            || (node.is_text() && node.text().is_some_and(|text| !text.trim().is_empty()))
    });
    has_content
        && !suffix.iter().any(|node| {
            node.is_element()
                && node
                    .tag_name()
                    .name()
                    .to_ascii_lowercase()
                    .strip_prefix('h')
                    .and_then(|value| value.parse::<u8>().ok())
                    .is_some_and(|candidate| candidate <= level)
        })
}

fn note_section_starts_at(
    heading: Node<'_, '_>,
    suffix: &[Node<'_, '_>],
    footnote_links: &HashMap<usize, LinkRole>,
) -> bool {
    is_note_section_heading(heading)
        && note_section_extends_to_container_end(heading, suffix)
        && suffix
            .iter()
            .copied()
            .any(|node| node.is_element() && note_entry_starts_at(node, footnote_links, true))
}

fn is_note_subsection_heading(node: Node<'_, '_>) -> bool {
    let name = node.tag_name().name().to_ascii_lowercase();
    name.strip_prefix('h')
        .and_then(|value| value.parse::<u8>().ok())
        .is_some_and(|level| (1..=6).contains(&level))
}

fn note_entry_starts_at(
    node: Node<'_, '_>,
    footnote_links: &HashMap<usize, LinkRole>,
    confirmed_note_section: bool,
) -> bool {
    node.descendants()
        .filter(|candidate| {
            candidate.is_element()
                && candidate.tag_name().name().eq_ignore_ascii_case("a")
                && !link_has_preceding_block_text(*candidate)
                && link_is_scoped_to_note_entry_node(node, *candidate)
        })
        .any(|anchor| {
            if footnote_links.get(&anchor.range().start) == Some(&LinkRole::FootnoteBacklink) {
                return true;
            }
            if !confirmed_note_section
                || (node_fragment(anchor).is_none() && node_fragment(node).is_none())
                || !attribute_local(anchor, "href").is_some_and(|href| href.contains('#'))
            {
                return false;
            }
            let marker = anchor
                .descendants()
                .filter(Node::is_text)
                .filter_map(|text| text.text())
                .collect::<String>();
            normalize_footnote_marker(&marker).is_some()
        })
}

fn link_is_scoped_to_note_entry_node(node: Node<'_, '_>, link: Node<'_, '_>) -> bool {
    let closest_block = link.ancestors().skip(1).find(|ancestor| {
        ancestor.is_element()
            && is_block_boundary(ancestor.tag_name().name().to_ascii_lowercase().as_str())
    });
    if closest_block == Some(node) {
        return true;
    }
    node_fragment(node).is_some()
        && !node
            .descendants()
            .take_while(|descendant| descendant.range().start < link.range().start)
            .filter(Node::is_text)
            .filter_map(|text| text.text())
            .any(|text| !text.trim().is_empty())
}

fn is_implicit_note_container(
    node: Node<'_, '_>,
    footnote_links: &HashMap<usize, LinkRole>,
) -> bool {
    let name = node.tag_name().name().to_ascii_lowercase();
    if !matches!(name.as_str(), "div" | "section" | "ol" | "ul") {
        return false;
    }
    let semantic_name = [attribute_local(node, "class"), attribute_local(node, "id")]
        .into_iter()
        .flatten()
        .flat_map(|value| {
            value.split(|character: char| character.is_whitespace() || character == '-')
        })
        .any(|token| {
            matches!(
                token.to_ascii_lowercase().as_str(),
                "footnote" | "footnotes" | "endnote" | "endnotes"
            )
        });
    semantic_name
        && node
            .children()
            .filter(Node::is_element)
            .any(|child| note_entry_starts_at(child, footnote_links, false))
}
