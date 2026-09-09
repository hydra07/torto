//! Compiles immutable page layouts into cheap-to-replay display lists.

use std::ops::Range;
use std::sync::Arc;

use anyrender::{Glyph, NormalizedCoord, PaintScene};
use kurbo::{Affine, BezPath, Circle, Line, Point, Rect, RoundedRect, Shape, Stroke, Vec2};
use parley::editing::{Cursor, Selection};
use parley::layout::{Affinity, BreakReason, Cluster, ClusterSide};
use parley::{FontData, Layout, PositionedLayoutItem};
use peniko::{Blob, Color, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
use rebook_layout::{
    ImagePlacement, PageItem, PageLayout, QuotePlacement, TablePlacement, TextBrush, TextPlacement,
};
use rebook_publication::{Rgba, SourceAnchor, SourceRange, TextBaseline};
include!("display_list/mod.rs");
include!("text/mod.rs");
include!("compiler/mod.rs");
#[cfg(test)]
mod tests;
