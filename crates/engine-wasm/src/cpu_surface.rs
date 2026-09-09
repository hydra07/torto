use anyrender::{ImageRenderer, PaintScene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use rebook_engine::{FrameTransition, PreparedReaderFrame};
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

const MAX_CPU_DIMENSION: u32 = u16::MAX as u32;
const TEXT_SELECTION_COLOR: Color = Color::from_rgba8(68, 137, 103, 72);
const ANNOTATION_MARK_COLOR: Color = Color::from_rgba8(96, 165, 250, 72);

/// CPU raster fallback owned by the engine's browser adapter. It replays the
/// same retained display lists as the GPU backend, then only asks Canvas2D to
/// present the resulting RGBA pixels.
pub struct CpuSurfaceRenderer {
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
    renderer: VelloCpuImageRenderer,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl CpuSurfaceRenderer {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, String> {
        let width = canvas.width().clamp(1, MAX_CPU_DIMENSION);
        let height = canvas.height().clamp(1, MAX_CPU_DIMENSION);
        let context = canvas
            .get_context("2d")
            .map_err(js_error)?
            .ok_or_else(|| "Canvas2D is unavailable for CPU rendering".to_owned())?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|_| "Canvas2D context has an unexpected type".to_owned())?;
        Ok(Self {
            canvas,
            context,
            renderer: VelloCpuImageRenderer::new(width, height),
            pixels: Vec::new(),
            width,
            height,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.clamp(1, MAX_CPU_DIMENSION);
        let height = height.clamp(1, MAX_CPU_DIMENSION);
        if (width, height) == (self.width, self.height) {
            return;
        }
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.renderer.resize(width, height);
        self.width = width;
        self.height = height;
    }

    pub fn render_frame(&mut self, frame: &PreparedReaderFrame) -> Result<(), String> {
        self.renderer.reset();
        self.renderer.render_to_vec(
            |scene| {
                let (source_x, destination_x) = match frame.transition {
                    FrameTransition::None => (0.0, None),
                    FrameTransition::Slide {
                        primary_offset_x,
                        destination_offset_x,
                        ..
                    } => (primary_offset_x, Some(destination_offset_x)),
                    FrameTransition::Curl {
                        direction,
                        progress,
                        ..
                    } => {
                        paint_cpu_curl(scene, frame, direction, progress);
                        return;
                    }
                };
                paint_spread(scene, &frame.current_spread, &frame.overlays, source_x);
                if let (Some(spread), Some(offset)) = (&frame.destination_spread, destination_x) {
                    paint_spread(scene, spread, &Default::default(), offset);
                }
            },
            &mut self.pixels,
        );
        let image = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(self.pixels.as_slice()),
            self.width,
            self.height,
        )
        .map_err(js_error)?;
        self.context
            .put_image_data(&image, 0.0, 0.0)
            .map_err(js_error)
    }
}

fn paint_cpu_curl(
    scene: &mut anyrender_vello_cpu::VelloCpuScenePainter,
    frame: &PreparedReaderFrame,
    direction: rebook_engine::PageDirection,
    progress: f32,
) {
    let Some(destination) = &frame.destination_spread else {
        paint_spread(scene, &frame.current_spread, &frame.overlays, 0.0);
        return;
    };
    paint_spread(scene, destination, &Default::default(), 0.0);
    let width = f64::from(frame.viewport.width);
    let height = f64::from(frame.viewport.height);
    let edge = match direction {
        rebook_engine::PageDirection::Next => width * f64::from(1.0 - progress),
        rebook_engine::PageDirection::Previous => width * f64::from(progress),
    };
    let clip = match direction {
        rebook_engine::PageDirection::Next => Rect::new(0.0, 0.0, edge, height),
        rebook_engine::PageDirection::Previous => Rect::new(edge, 0.0, width, height),
    };
    scene.push_clip_layer(Affine::IDENTITY, &clip);
    paint_spread(scene, &frame.current_spread, &frame.overlays, 0.0);
    scene.pop_layer();
    let shadow = match direction {
        rebook_engine::PageDirection::Next => Rect::new((edge - 14.0).max(0.0), 0.0, edge, height),
        rebook_engine::PageDirection::Previous => {
            Rect::new(edge, 0.0, (edge + 14.0).min(width), height)
        }
    };
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(8, 12, 18, 42),
        None,
        &shadow,
    );
}

fn paint_spread(
    scene: &mut anyrender_vello_cpu::VelloCpuScenePainter,
    spread: &rebook_engine::ReaderSpread,
    overlays: &rebook_engine::OverlaySet,
    transition_x: f32,
) {
    spread.primary.paint_background_at(scene, transition_x);
    paint_page(
        scene,
        &spread.primary,
        overlays,
        spread.primary_offset_x + transition_x,
    );
    if let Some(secondary) = &spread.secondary {
        paint_page(
            scene,
            secondary,
            overlays,
            spread.secondary_offset_x + transition_x,
        );
    }
}

fn paint_page(
    scene: &mut anyrender_vello_cpu::VelloCpuScenePainter,
    page: &rebook_renderer::PageDisplayList,
    overlays: &rebook_engine::OverlaySet,
    offset_x: f32,
) {
    page.paint_images_at(scene, offset_x);
    if !overlays.highlights.is_empty() {
        page.paint_source_ranges(scene, &overlays.highlights, ANNOTATION_MARK_COLOR, offset_x);
    }
    if !overlays.selection.is_empty() {
        page.paint_source_ranges(scene, &overlays.selection, TEXT_SELECTION_COLOR, offset_x);
    }
    if !overlays.focus.is_empty() {
        page.paint_source_ranges(scene, &overlays.focus, TEXT_SELECTION_COLOR, offset_x);
    }
    page.paint_non_image_content_at(scene, offset_x);
}

fn js_error(error: wasm_bindgen::JsValue) -> String {
    error
        .as_string()
        .unwrap_or_else(|| "browser canvas operation failed".to_owned())
}
