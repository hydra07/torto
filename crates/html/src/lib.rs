//! Shared HTML/CSS to format-neutral reading IR parser.

use std::collections::{HashMap, HashSet};

use rebook_publication::{
    Block, BlockStyle, CaptionPosition, FigureBlock, HyphenationMode, ImageBlock, ImageLength,
    ImageStyle, Inline, InlineImageAlignment, InlineImageRun, InlineRole, LinkRole, MathRun,
    NoteBlock, NoteBlockKind, PublicationUrl, QuoteBlock, Rgba, Section, SectionAnchor,
    SeparatorBlock, SourceAnchor, SourceRange, SpineItem, SpineItemId, TableBlock, TableCell,
    TableRow, TextAlignment, TextBaseline, TextBlock, TextBlockKind, TextLanguage, TextRun,
    TextStyle,
};
use roxmltree::{Document, Node};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HtmlError {
    #[error("invalid HTML in {resource}: {message}")]
    InvalidDocument { resource: String, message: String },
    #[error(transparent)]
    Publication(#[from] rebook_publication::PublicationError),
}

/// Publication-level semantic hints that cannot be derived reliably from one HTML resource.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SectionParseHints {
    /// The publication navigation identifies this entire resource as Notes/Endnotes.
    pub note_section: bool,
}
include!("parser/mod.rs");
include!("inline/mod.rs");
include!("css/mod.rs");
#[cfg(test)]
mod tests;
