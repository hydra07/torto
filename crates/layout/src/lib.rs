//! Renderer-independent pagination for normalized reading IR.

pub mod linebreak;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use image::ImageError;
use parley::setting::Tag;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStyle, FontVariation, FontVariations,
    FontWeight, IndentOptions, InlineBox as ParleyInlineBox, InlineBoxKind, Layout, LayoutContext,
    LineHeight, PositionedLayoutItem, StyleProperty,
};
use read_fonts::{FontRef, TableProvider as _};
use rebook_publication::{
    Block, BlockStyle, BookSource, CaptionPosition, FixedPageDimensions, FixedPageTextLayer,
    FixedPageTextRect, ImageBlock, ImageLength, ImageStyle, Inline, InlineImageAlignment,
    InlineRole, LinkRole, MathRun, NoteBlockKind, PublicationError, PublicationUrl,
    RenditionLayout, Rgba, Section, SeparatorKind, SourceRange, TableBlock, TableCell,
    TextAlignment, TextBaseline, TextBlock, TextBlockKind, TextRun, TextStyle, WritingSystem,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use unicode_script::{Script, UnicodeScript as _};
use unicode_segmentation::UnicodeSegmentation as _;
include!("style/mod.rs");
include!("model/mod.rs");
include!("engine/mod.rs");
include!("text/mod.rs");
include!("pagination/mod.rs");
#[cfg(test)]
mod tests;
