use rebook_publication::{
    Block, BookSource, Inline, LocatorV1, RenditionLayout, SourceAnchor, SourceRange, TextBlock,
    TextBlockKind, TocEntry,
};
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};

/// Persistent bookmark pointing to a canonical locator in the book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub title: String,
    pub locator: LocatorV1,
    pub created_at_ms: i64,
}

/// Color category for persistent text highlights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HighlightColor {
    #[default]
    Yellow,
    Green,
    Blue,
    Purple,
    Red,
}

/// Persistent user highlight / note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Highlight {
    pub id: String,
    pub range: SourceRange,
    pub text: String,
    pub color: HighlightColor,
    pub note: Option<String>,
    pub locator: LocatorV1,
    pub created_at_ms: i64,
}

/// A matched full-text search result inside the book content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub section_index: usize,
    pub section_title: String,
    pub excerpt: String,
    pub matched_text: String,
    pub block_kind: String,
    pub range: SourceRange,
    pub locator: LocatorV1,
}

const DEFAULT_CONTEXT_CHARS: usize = 72;

/// Executes full-text search across sections of a book source.
pub fn search_book(
    source: &dyn BookSource,
    query: &str,
    max_results: usize,
) -> Result<Vec<SearchResult>, String> {
    let query = query.trim();
    if query.is_empty() || max_results == 0 {
        return Ok(Vec::new());
    }
    let matcher = RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .unicode(true)
        .build()
        .map_err(|error| format!("Invalid search pattern: {error}"))?;

    let mut results = Vec::new();
    let section_count = source.book().sections.len();

    for section_index in 0..section_count {
        let section = source
            .parse_section(section_index)
            .map_err(|error| format!("Failed to parse section {}: {error}", section_index + 1))?;
        let section_title = section_title(source, section_index, &section.blocks);

        for block in &section.blocks {
            if let Block::Table(table) = block {
                for cell in table.rows.iter().flat_map(|row| &row.cells) {
                    let Some(source_range) = &cell.text.source else {
                        continue;
                    };
                    let text = text_block_text(&cell.text);
                    for found in matcher.find_iter(&text) {
                        let range =
                            source_range_for_match(source_range, &text, found.start(), found.end());
                        let locator = LocatorV1 {
                            version: LocatorV1::VERSION,
                            publication_id: source.book().id.clone(),
                            href: section.href.clone(),
                            progression: None,
                            total_progression: None,
                            position: None,
                            source: Some(range.clone()),
                            partial_cfi: None,
                            text: Some(rebook_publication::TextQuote {
                                before: String::new(),
                                highlight: found.as_str().to_string(),
                                after: String::new(),
                            }),
                        };
                        results.push(SearchResult {
                            section_index,
                            section_title: section_title.clone(),
                            excerpt: excerpt(&text, found.start(), found.end(), DEFAULT_CONTEXT_CHARS),
                            matched_text: found.as_str().to_owned(),
                            block_kind: "table-cell".into(),
                            range,
                            locator,
                        });
                        if results.len() >= max_results {
                            return Ok(results);
                        }
                    }
                }
                continue;
            }

            let (text, source_range, block_kind) = match block {
                Block::Text(block) => {
                    let Some(source_range) = &block.source else {
                        continue;
                    };
                    (
                        text_block_text(block),
                        source_range,
                        text_block_kind(block).to_owned(),
                    )
                }
                Block::Quote(quote) => {
                    for child in quote.body.iter().chain(quote.attribution.iter()) {
                        let Some(source_range) = &child.source else {
                            continue;
                        };
                        let text = text_block_text(child);
                        for found in matcher.find_iter(&text) {
                            let range = source_range_for_match(
                                source_range,
                                &text,
                                found.start(),
                                found.end(),
                            );
                            let locator = LocatorV1 {
                                version: LocatorV1::VERSION,
                                publication_id: source.book().id.clone(),
                                href: section.href.clone(),
                                progression: None,
                                total_progression: None,
                                position: None,
                                source: Some(range.clone()),
                                partial_cfi: None,
                                text: Some(rebook_publication::TextQuote {
                                    before: String::new(),
                                    highlight: found.as_str().to_string(),
                                    after: String::new(),
                                }),
                            };
                            results.push(SearchResult {
                                section_index,
                                section_title: section_title.clone(),
                                excerpt: excerpt(
                                    &text,
                                    found.start(),
                                    found.end(),
                                    DEFAULT_CONTEXT_CHARS,
                                ),
                                matched_text: found.as_str().to_owned(),
                                block_kind: text_block_kind(child).to_owned(),
                                range,
                                locator,
                            });
                            if results.len() >= max_results {
                                return Ok(results);
                            }
                        }
                    }
                    continue;
                }
                Block::Image(image) => {
                    let (Some(layer), Some(source_range)) = (&image.text_layer, &image.source)
                    else {
                        continue;
                    };
                    (layer.text.clone(), source_range, "image-text".into())
                }
                Block::Figure(_)
                | Block::Table(_)
                | Block::Note(_)
                | Block::Separator(_)
                | Block::LineBreak
                | Block::PageBreak => {
                    continue;
                }
            };

            for found in matcher.find_iter(&text) {
                let range = source_range_for_match(source_range, &text, found.start(), found.end());
                let locator = LocatorV1 {
                    version: LocatorV1::VERSION,
                    publication_id: source.book().id.clone(),
                    href: section.href.clone(),
                    progression: None,
                    total_progression: None,
                    position: None,
                    source: Some(range.clone()),
                    partial_cfi: None,
                    text: Some(rebook_publication::TextQuote {
                        before: String::new(),
                        highlight: found.as_str().to_string(),
                        after: String::new(),
                    }),
                };
                results.push(SearchResult {
                    section_index,
                    section_title: section_title.clone(),
                    excerpt: excerpt(&text, found.start(), found.end(), DEFAULT_CONTEXT_CHARS),
                    matched_text: found.as_str().to_owned(),
                    block_kind: block_kind.clone(),
                    range,
                    locator,
                });
                if results.len() >= max_results {
                    return Ok(results);
                }
            }
        }
    }

    Ok(results)
}

pub(crate) fn text_block_kind(block: &TextBlock) -> &'static str {
    match block.kind {
        TextBlockKind::Paragraph => "paragraph",
        TextBlockKind::Heading(_) | TextBlockKind::HeadingOrdinal(_) => "heading",
        TextBlockKind::Blockquote => "blockquote",
        TextBlockKind::QuoteAttribution => "quote-attribution",
        TextBlockKind::Preformatted => "preformatted",
        TextBlockKind::ListItem { .. } => "list-item",
        TextBlockKind::DefinitionTerm { .. } => "definition-term",
        TextBlockKind::DefinitionDescription { .. } => "definition-description",
        TextBlockKind::Caption => "caption",
        TextBlockKind::FootnoteDefinition => "footnote-definition",
    }
}

pub(crate) fn text_block_text(block: &TextBlock) -> String {
    block
        .content
        .iter()
        .map(|inline| match inline {
            Inline::Text(run) => run.text.as_str(),
            Inline::Math(run) => run.latex.as_str(),
            Inline::Image(_) => "",
            Inline::Break => "\n",
        })
        .collect()
}

pub(crate) fn section_title(
    source: &dyn BookSource,
    section_index: usize,
    blocks: &[Block],
) -> String {
    let book = source.book();
    if book.metadata.layout == RenditionLayout::PrePaginated {
        let href = &book.sections[section_index].href;
        return toc_label_for_href(&book.table_of_contents, href)
            .unwrap_or_else(|| format!("Section {}", section_index + 1));
    }
    if let Some(title) = blocks.iter().find_map(|block| match block {
        Block::Text(block) if matches!(block.kind, TextBlockKind::Heading(_)) => {
            let text = text_block_text(block);
            (!text.trim().is_empty()).then(|| text.trim().to_owned())
        }
        _ => None,
    }) {
        return title;
    }
    let href = &book.sections[section_index].href;
    toc_label_for_href(&book.table_of_contents, href)
        .unwrap_or_else(|| format!("Section {}", section_index + 1))
}

fn toc_label_for_href(
    entries: &[TocEntry],
    href: &rebook_publication::PublicationUrl,
) -> Option<String> {
    for entry in entries {
        if entry
            .href
            .as_ref()
            .is_some_and(|target| target.resource_url() == href.resource_url())
        {
            return Some(entry.label.clone());
        }
        if let Some(label) = toc_label_for_href(&entry.children, href) {
            return Some(label);
        }
    }
    None
}

fn source_range_for_match(
    source: &SourceRange,
    text: &str,
    byte_start: usize,
    byte_end: usize,
) -> SourceRange {
    if source.start.spine != source.end.spine || source.start.node != source.end.node {
        return source.clone();
    }
    let start_offset = source.start.text_offset
        + u64::try_from(text[..byte_start].chars().count()).unwrap_or(u64::MAX);
    let end_offset = source.start.text_offset
        + u64::try_from(text[..byte_end].chars().count()).unwrap_or(u64::MAX);
    if start_offset >= end_offset || end_offset > source.end.text_offset {
        return source.clone();
    }
    SourceRange {
        start: SourceAnchor {
            spine: source.start.spine.clone(),
            node: source.start.node.clone(),
            text_offset: start_offset,
        },
        end: SourceAnchor {
            spine: source.end.spine.clone(),
            node: source.end.node.clone(),
            text_offset: end_offset,
        },
    }
}

fn excerpt(text: &str, start: usize, end: usize, context_chars: usize) -> String {
    let context_start = text[..start]
        .char_indices()
        .rev()
        .nth(context_chars.saturating_sub(1))
        .map_or(0, |(index, _)| index);
    let context_end = text[end..]
        .char_indices()
        .nth(context_chars)
        .map_or(text.len(), |(index, _)| end + index);
    format!(
        "{}{}{}",
        if context_start > 0 { "…" } else { "" },
        text[context_start..context_end].trim(),
        if context_end < text.len() { "…" } else { "" }
    )
}
