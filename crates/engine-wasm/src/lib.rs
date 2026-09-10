//! Platform-neutral WebAssembly compositor building blocks.
//!
//! The browser adapter is intentionally kept thin: these modules contain no
//! DOM ownership and can be exercised by a future `wasm-bindgen` facade.

mod cpu_surface;
pub mod surface;

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use cpu_surface::CpuSurfaceRenderer;
use rebook_engine::{
    AppLifecycleEvent, EngineAnimationState, EngineConfig, EngineNavigationState, EngineReader,
    EngineRuntime, LocatorV1, MemoryPressure, PageDirection, ParagraphIndentMode,
    PlatformDirective, PointerEvent, PointerGestureResult, PointerKind, PointerPhase,
    ReaderDefaultFont, ReaderStyle, Rgba, SourceRange, SpreadMode, TickResult, TypesettingMode,
    ViewportMetrics,
};
use rebook_layout::ReaderFontBlob;
use rebook_vello_backend::{ReaderCompositor, SpreadSceneCache, frame_images};
use surface::GpuSurfaceRenderer;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// Thin browser-facing facade. Parsing and pagination stay in the Rust engine;
/// JavaScript only supplies bytes and viewport events.
struct WebReaderState {
    runtime: EngineRuntime,
    renderer: Option<BrowserRenderer>,
    fallback_reason: Option<String>,
    scene_cache: SpreadSceneCache,
}

enum BrowserRenderer {
    Gpu(GpuSurfaceRenderer),
    Cpu(CpuSurfaceRenderer),
}

impl Default for WebReaderState {
    fn default() -> Self {
        Self::new()
    }
}

impl WebReaderState {
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        Self {
            runtime: EngineRuntime::new(
                web_engine_config(),
                ViewportMetrics::from_logical_size(1200, 760, 1.0),
            ),
            renderer: None,
            fallback_reason: None,
            scene_cache: SpreadSceneCache::new(5),
        }
    }

    /// Creates a reader with an initialized WebGPU surface.
    ///
    /// This is a static async constructor rather than an async `&mut self`
    /// method. Exporting an async mutable method would keep wasm-bindgen's
    /// exclusive borrow guard alive across the Promise and reject subsequent
    /// `tick`, navigation, and render calls as recursive aliasing.
    pub async fn create(canvas: HtmlCanvasElement) -> Result<WebReaderState, JsValue> {
        let (renderer, fallback_reason) = match GpuSurfaceRenderer::new(canvas.clone()).await {
            Ok(renderer) => (BrowserRenderer::Gpu(renderer), None),
            Err(gpu_error) => {
                let cpu_renderer = CpuSurfaceRenderer::new(canvas).map_err(|cpu_error| {
                    JsValue::from_str(&format!(
                        "GPU initialization failed ({gpu_error}); CPU fallback failed ({cpu_error})"
                    ))
                })?;
                (BrowserRenderer::Cpu(cpu_renderer), Some(gpu_error))
            }
        };
        let mut reader = Self::new();
        reader.renderer = Some(renderer);
        reader.fallback_reason = fallback_reason;
        Ok(reader)
    }

    pub fn renderer_kind(&self) -> String {
        match self.renderer {
            Some(BrowserRenderer::Gpu(_)) => "webgpu",
            Some(BrowserRenderer::Cpu(_)) => "cpu",
            None => "none",
        }
        .to_owned()
    }

    /// Returns adapter capabilities and the reason for a renderer fallback, if any.
    pub fn renderer_capabilities(&self) -> String {
        let renderer = self.renderer_kind();
        let capabilities = serde_json::json!({
            "renderer": renderer,
            "webgpu": self
                .renderer
                .as_ref()
                .is_some_and(|renderer| matches!(renderer, BrowserRenderer::Gpu(_))),
            "cpu_fallback": self
                .renderer
                .as_ref()
                .is_some_and(|renderer| matches!(renderer, BrowserRenderer::Cpu(_))),
            "slide_transition": true,
            "curl_3d_transition": self
                .renderer
                .as_ref()
                .is_some_and(|renderer| matches!(renderer, BrowserRenderer::Gpu(_))),
            "static_scene_cache": true,
            "fallback_reason": self.fallback_reason.as_deref(),
        });
        serde_json::to_string(&capabilities).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Opens bytes through the same format dispatcher used by native clients.
    /// Returns a small JSON metadata object for the UI shell.
    pub fn open_bytes(
        &mut self,
        bytes: &[u8],
        file_name: &str,
        logical_width: u32,
        logical_height: u32,
        surface_width: u32,
        surface_height: u32,
        scale_factor: f32,
    ) -> Result<JsValue, JsValue> {
        self.close();
        let viewport = ViewportMetrics::new(
            logical_width,
            logical_height,
            surface_width,
            surface_height,
            scale_factor,
        );
        self.runtime
            .resize(viewport)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let summary = self
            .runtime
            .open_bytes(Arc::<[u8]>::from(bytes), file_name)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        serde_json::to_string(&serde_json::json!({ "title": summary.title, "sections": summary.section_count, "progress": 0.0 })).map(|json| JsValue::from_str(&json)).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn resize(
        &mut self,
        logical_width: u32,
        logical_height: u32,
        surface_width: u32,
        surface_height: u32,
        scale_factor: f32,
    ) -> Result<(), JsValue> {
        let viewport = ViewportMetrics::new(
            logical_width,
            logical_height,
            surface_width,
            surface_height,
            scale_factor,
        );
        if let Some(renderer) = self.renderer.as_mut() {
            match renderer {
                BrowserRenderer::Gpu(renderer) => renderer.resize(surface_width, surface_height),
                BrowserRenderer::Cpu(renderer) => renderer.resize(surface_width, surface_height),
            }
        }
        self.scene_cache.invalidate_all();
        self.runtime
            .resize(viewport)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Renders the current frame onto the attached WebGPU surface via Vello.
    pub fn render_frame(&mut self) -> Result<(), JsValue> {
        let reader = self
            .runtime
            .reader_mut()
            .ok_or_else(|| JsValue::from_str("no book is open"))?;
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| JsValue::from_str("no renderer attached"))?;

        let frame = reader
            .frame()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        match renderer {
            BrowserRenderer::Gpu(renderer) => {
                for image in frame_images(&frame) {
                    renderer.ensure_image_uploaded(image);
                }

                if let rebook_engine::FrameTransition::Curl {
                    direction,
                    progress,
                    start_x_ratio,
                    start_y_ratio,
                    current_x_ratio,
                    current_y_ratio,
                } = frame.transition
                {
                    if let Some(dest_scene) =
                        ReaderCompositor::compose_destination_scene(&mut self.scene_cache, &frame)
                    {
                        let current_scene =
                            ReaderCompositor::compose_current_scene(&mut self.scene_cache, &frame);
                        let curl_direction = match direction {
                            rebook_engine::PageDirection::Next => {
                                rebook_vello_backend::CurlDirection::Next
                            }
                            rebook_engine::PageDirection::Previous => {
                                rebook_vello_backend::CurlDirection::Previous
                            }
                        };
                        let viewport = self.runtime.viewport().layout;
                        let aspect_ratio =
                            viewport.height as f32 / (viewport.width as f32).max(1.0);
                        let curl_config = rebook_vello_backend::Curl3dConfig {
                            aspect_ratio,
                            ..Default::default()
                        };
                        let gesture = if progress > 0.0001 {
                            rebook_vello_backend::Curl3dGesture {
                                active: true,
                                direction: curl_direction,
                                grab_mode: rebook_vello_backend::CurlGrabMode::TouchPoint,
                                drag_start_uv: glam::Vec2::new(start_x_ratio, start_y_ratio),
                                drag_current_uv: glam::Vec2::new(current_x_ratio, current_y_ratio),
                            }
                        } else {
                            rebook_vello_backend::Curl3dGesture {
                                active: false,
                                direction: curl_direction,
                                grab_mode: rebook_vello_backend::CurlGrabMode::TouchPoint,
                                drag_start_uv: glam::Vec2::new(start_x_ratio, start_y_ratio),
                                drag_current_uv: glam::Vec2::new(current_x_ratio, current_y_ratio),
                            }
                        };
                        renderer.render_curl_3d(&current_scene, &dest_scene, gesture, curl_config)
                    } else {
                        let scene = ReaderCompositor::compose_frame(&mut self.scene_cache, &frame);
                        renderer.render_frame(&scene)
                    }
                } else {
                    let scene = ReaderCompositor::compose_frame(&mut self.scene_cache, &frame);
                    renderer.render_frame(&scene)
                }
            }
            BrowserRenderer::Cpu(renderer) => renderer.render_frame(&frame),
        }
        .map_err(|e| JsValue::from_str(&e))?;
        Ok(())
    }

    /// Advances cooperative pagination within `budget_ms`.
    /// Returns 0 for Idle, 1 for `MoreWorkRemaining`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn tick(&mut self, budget_ms: f64) -> Result<u8, JsValue> {
        let reader = self.reader_mut()?;
        let budget = Duration::from_micros((budget_ms.max(0.0) * 1000.0) as u64);
        let res = reader
            .tick(budget)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(match res {
            TickResult::Idle => 0,
            TickResult::MoreWorkRemaining => 1,
        })
    }

    pub fn close(&mut self) {
        if let Some(reader) = self.runtime.reader_mut() {
            reader.cancel_pending_navigation();
        }
        self.runtime.close();
        self.scene_cache.invalidate_all();
    }

    pub fn is_open(&self) -> bool {
        self.runtime.is_open()
    }

    pub fn toc_json(&self) -> Result<JsValue, JsValue> {
        let reader = self.reader()?;
        let items = reader.toc_items().iter().map(|item| serde_json::json!({ "id": item.id, "label": item.label, "depth": item.depth })).collect::<Vec<_>>();
        serde_json::to_string(&items)
            .map(|value| JsValue::from_str(&value))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Returns browser-facing reading state. The shell presents this data but
    /// does not derive progression or active TOC ancestry itself.
    pub fn state_json(&self) -> Result<JsValue, JsValue> {
        let reader = self.reader()?;
        let snapshot = reader.snapshot();
        let location = snapshot.location;
        let value = serde_json::json!({
            "progression": snapshot.total_progression,
            "active_toc_id": snapshot.active_toc_id,
            "active_toc_path": snapshot.active_toc_path,
            "location": {
                "section_index": location.section_index,
                "segment_index": location.segment_index,
                "segment_count": location.segment_count,
                "page_index": location.page_index,
                "page_count": location.page_count,
            },
        });
        json_to_js(&value)
    }

    pub fn locator_json(&self) -> Result<JsValue, JsValue> {
        let reader = self.reader()?;
        let value = serde_json::to_value(reader.current_locator())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        json_to_js(&value)
    }

    pub fn restore_locator_json(&mut self, locator_json: &str) -> Result<(), JsValue> {
        let locator: LocatorV1 = serde_json::from_str(locator_json)
            .map_err(|e| JsValue::from_str(&format!("invalid locator: {e}")))?;
        let reader = self.reader_mut()?;
        reader
            .restore_locator(&locator)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    pub fn navigate_toc(&mut self, id: &str) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        reader
            .go_to_toc_item(id)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    pub fn search(&self, query: &str, max_results: usize) -> Result<JsValue, JsValue> {
        let reader = self.reader()?;
        let results = reader
            .search(query, max_results)
            .map_err(|e| JsValue::from_str(&e))?;
        serde_json::to_string(&results)
            .map(|json| JsValue::from_str(&json))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn set_highlights_json(&mut self, ranges_json: &str) -> Result<(), JsValue> {
        let ranges: Vec<SourceRange> = serde_json::from_str(ranges_json)
            .map_err(|e| JsValue::from_str(&format!("invalid source ranges: {e}")))?;
        let reader = self.reader_mut()?;
        reader.set_highlights(ranges);
        Ok(())
    }

    pub fn clear_highlights(&mut self) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        reader.clear_highlights();
        Ok(())
    }

    pub fn set_focus_json(&mut self, ranges_json: &str) -> Result<(), JsValue> {
        let ranges: Vec<SourceRange> = serde_json::from_str(ranges_json)
            .map_err(|e| JsValue::from_str(&format!("invalid source ranges: {e}")))?;
        let reader = self.reader_mut()?;
        reader.set_focus_ranges(ranges);
        Ok(())
    }

    pub fn clear_focus(&mut self) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        reader.clear_focus_ranges();
        Ok(())
    }

    pub fn style_json(&self) -> Result<JsValue, JsValue> {
        let reader = self.reader()?;
        let style = reader.session().style();
        serde_json::to_string(&style)
            .map(|json| JsValue::from_str(&json))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn set_style_json(&mut self, style_json: &str) -> Result<(), JsValue> {
        let style: ReaderStyle = serde_json::from_str(style_json)
            .map_err(|e| JsValue::from_str(&format!("invalid reader style JSON: {e}")))?;
        let reader = self.reader_mut()?;
        reader
            .set_style(style)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    pub fn set_font_size(&mut self, font_size: f32) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        let mut style = reader.session().style();
        style.typography.font_size = font_size;
        style.typography.normalize();
        reader
            .set_style(style)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    pub fn set_line_height(&mut self, line_height: f32) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        let mut style = reader.session().style();
        style.typesetting.mode = TypesettingMode::Unified;
        style.typesetting.body_line_height = line_height;
        style.typesetting.normalize();
        reader
            .set_style(style)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    pub fn set_paragraph_indent(&mut self, indent_em: f32) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        let mut style = reader.session().style();
        style.typesetting.mode = TypesettingMode::Unified;
        style.typesetting.paragraph_indent_mode = ParagraphIndentMode::Custom;
        style.typesetting.paragraph_indent_em = indent_em;
        style.typesetting.normalize();
        reader
            .set_style(style)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    pub fn set_font_family(&mut self, category: &str, family: &str) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        let mut style = reader.session().style();
        match category {
            "serif" => {
                style.typography.default_font = ReaderDefaultFont::Serif;
                if !family.is_empty() {
                    style.typography.serif_font = family.to_owned();
                }
            }
            "sans-serif" | "sans" => {
                style.typography.default_font = ReaderDefaultFont::SansSerif;
                if !family.is_empty() {
                    style.typography.sans_serif_font = family.to_owned();
                }
            }
            "cjk" => {
                if !family.is_empty() {
                    style.typography.default_cjk_font = family.to_owned();
                }
            }
            "other" | _ => {
                style.typography.default_font = ReaderDefaultFont::Other;
                if !family.is_empty() {
                    style.typography.other_font = family.to_owned();
                }
            }
        }
        style.typography.normalize();
        reader
            .set_style(style)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    pub fn set_margins(&mut self, horizontal: f32, top: f32, bottom: f32) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        let mut style = reader.session().style();
        style.horizontal_margin = horizontal;
        style.top_margin = top;
        style.bottom_margin = bottom;
        reader
            .set_style(style)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    pub fn set_spread_mode(&mut self, mode: &str) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        let mut style = reader.session().style();
        style.spread = match mode {
            "single" => SpreadMode::Single,
            "double" => SpreadMode::Double,
            "scroll" => SpreadMode::Scroll,
            _ => return Err(JsValue::from_str("invalid spread mode")),
        };
        reader
            .set_style(style)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    pub fn set_colors(
        &mut self,
        fg_r: u8,
        fg_g: u8,
        fg_b: u8,
        bg_r: u8,
        bg_g: u8,
        bg_b: u8,
    ) -> Result<(), JsValue> {
        let reader = self.reader_mut()?;
        let mut style = reader.session().style();
        style.foreground = Rgba {
            red: fg_r,
            green: fg_g,
            blue: fg_b,
            alpha: 255,
        };
        style.background = Rgba {
            red: bg_r,
            green: bg_g,
            blue: bg_b,
            alpha: 255,
        };
        reader
            .set_style(style)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.scene_cache.invalidate_all();
        Ok(())
    }

    /// Returns retained-page diagnostics after pagination. This is the first
    /// browser-visible proof that the reader, not just the parser, is active.
    pub fn page_info(&mut self) -> Result<JsValue, JsValue> {
        let reader = self.reader_mut()?;
        let spread = reader
            .current_spread()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let primary = &spread.primary;
        let secondary = spread.secondary.as_ref();
        let json = serde_json::json!({
            "primary_commands": primary.command_count(),
            "primary_text_regions": primary.text_region_count(),
            "secondary_commands": secondary.map(|page| page.command_count()),
            "secondary_text_regions": secondary.map(|page| page.text_region_count()),
        });
        serde_json::to_string(&json)
            .map(|value| JsValue::from_str(&value))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn page_text(&mut self) -> Result<JsValue, JsValue> {
        let reader = self.reader_mut()?;
        let spread = reader
            .current_spread()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut pages = vec![&spread.primary];
        if let Some(secondary) = spread.secondary.as_ref() {
            pages.push(secondary);
        }
        let text = pages
            .into_iter()
            .flat_map(|page| {
                (0..page.text_region_count()).filter_map(|index| page.text_region_text(index))
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(JsValue::from_str(&text))
    }

    pub fn navigate_next(&mut self) -> Result<u8, JsValue> {
        self.navigation_step(PageDirection::Next)
    }

    pub fn navigate_previous(&mut self) -> Result<u8, JsValue> {
        self.navigation_step(PageDirection::Previous)
    }

    pub fn pointer_down(
        &mut self,
        id: u32,
        x: f32,
        y: f32,
        timestamp_ms: f64,
    ) -> Result<u8, JsValue> {
        self.handle_pointer(id, PointerPhase::Down, x, y, timestamp_ms)
    }

    pub fn pointer_move(
        &mut self,
        id: u32,
        x: f32,
        y: f32,
        timestamp_ms: f64,
    ) -> Result<u8, JsValue> {
        self.handle_pointer(id, PointerPhase::Move, x, y, timestamp_ms)
    }

    pub fn pointer_up(
        &mut self,
        id: u32,
        x: f32,
        y: f32,
        timestamp_ms: f64,
    ) -> Result<u8, JsValue> {
        self.handle_pointer(id, PointerPhase::Up, x, y, timestamp_ms)
    }

    fn handle_pointer(
        &mut self,
        id: u32,
        phase: PointerPhase,
        x: f32,
        y: f32,
        timestamp_ms: f64,
    ) -> Result<u8, JsValue> {
        self.reader_mut()?
            .handle_pointer(PointerEvent {
                id: u64::from(id),
                phase,
                kind: PointerKind::Unknown,
                x,
                y,
                timestamp_ms,
                pressure: 1.0,
            })
            .map(pointer_result_code)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn pointer_cancel(&mut self, timestamp_ms: f64) -> Result<u8, JsValue> {
        Ok(pointer_result_code(
            self.reader_mut()?.cancel_pointer_gesture(timestamp_ms),
        ))
    }

    pub fn focus_lost(&mut self, timestamp_ms: f64) -> Result<u8, JsValue> {
        self.pointer_cancel(timestamp_ms)
    }

    pub fn lifecycle(&mut self, state: &str, timestamp_ms: f64) -> Result<(), JsValue> {
        let event = match state {
            "resumed" => AppLifecycleEvent::Resumed,
            "suspended" => AppLifecycleEvent::Suspended,
            "surface-lost" => AppLifecycleEvent::SurfaceLost,
            "surface-restored" => AppLifecycleEvent::SurfaceRestored,
            _ => return Err(JsValue::from_str("unknown lifecycle state")),
        };
        let directive = self.runtime.lifecycle(event, timestamp_ms);
        if matches!(
            directive,
            PlatformDirective::ReleaseTransientRenderResources
        ) {
            self.scene_cache.invalidate_all();
        }
        Ok(())
    }

    pub fn memory_pressure(&mut self, critical: bool) {
        let level = if critical {
            MemoryPressure::Critical
        } else {
            MemoryPressure::Moderate
        };
        self.runtime.memory_pressure(level);
        self.scene_cache.invalidate_all();
    }

    pub fn selection_start(&mut self, x: f32, y: f32) -> Result<bool, JsValue> {
        self.reader_mut()?
            .begin_text_selection(x, y)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn selection_update(&mut self, x: f32, y: f32) -> Result<bool, JsValue> {
        self.reader_mut()?
            .update_text_selection(x, y)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn selection_end(&mut self) -> Result<String, JsValue> {
        Ok(self
            .reader_mut()?
            .end_text_selection()
            .unwrap_or_default()
            .to_owned())
    }

    pub fn selection_clear(&mut self) -> Result<bool, JsValue> {
        Ok(self.reader_mut()?.clear_text_selection())
    }

    pub fn selection_json(&self) -> Result<JsValue, JsValue> {
        let reader = self.reader()?;
        let value = reader.selection().map_or_else(
            || serde_json::json!({"text": "", "ranges": [], "rects": []}),
            |selection| {
                serde_json::json!({
                    "text": selection.text,
                    "ranges": selection.ranges,
                    "rects": selection.rects.iter().map(|rect| serde_json::json!({
                        "position": {
                            "section_index": rect.position.section_index,
                            "segment_index": rect.position.segment_index,
                            "page_index": rect.position.page_index,
                        },
                        "x": rect.x,
                        "y": rect.y,
                        "width": rect.width,
                        "height": rect.height,
                    })).collect::<Vec<_>>(),
                })
            },
        );
        json_to_js(&value)
    }

    pub fn animation_step(&mut self, timestamp_ms: f64) -> Result<u8, JsValue> {
        self.reader_mut()?
            .animation_step(timestamp_ms)
            .map(|state| match state {
                EngineAnimationState::Idle => 0,
                EngineAnimationState::NeedsFrame => 1,
                EngineAnimationState::Moved => 2,
            })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    fn navigation_step(&mut self, direction: PageDirection) -> Result<u8, JsValue> {
        let reader = self.reader_mut()?;
        reader
            .navigation_step(direction)
            .map(|state| match state {
                EngineNavigationState::Boundary => 0,
                EngineNavigationState::Pending => 1,
                EngineNavigationState::Moved => 2,
            })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    fn reader_mut(&mut self) -> Result<&mut EngineReader, JsValue> {
        self.runtime
            .reader_mut()
            .ok_or_else(|| JsValue::from_str("no book is open"))
    }

    fn reader(&self) -> Result<&EngineReader, JsValue> {
        self.runtime
            .reader()
            .ok_or_else(|| JsValue::from_str("no book is open"))
    }
}

fn web_engine_config() -> EngineConfig {
    const LITERATA: &[u8] = include_bytes!("../../../assets/fonts/Literata-opsz-wght.ttf");
    const LITERATA_ITALIC: &[u8] =
        include_bytes!("../../../assets/fonts/Literata-Italic-opsz-wght.ttf");
    let fonts = vec![
        ReaderFontBlob::new(Arc::new(LITERATA)),
        ReaderFontBlob::new(Arc::new(LITERATA_ITALIC)),
    ];
    EngineConfig {
        fonts: fonts.into(),
    }
}

/// Browser-facing handle with a shared WASM ABI. Mutable engine state is
/// guarded internally so wasm-bindgen never holds an exclusive borrow of the
/// exported object across browser callbacks.
#[wasm_bindgen]
pub struct WebReader {
    inner: RefCell<Option<WebReaderState>>,
}

#[wasm_bindgen]
impl WebReader {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Some(WebReaderState::new())),
        }
    }

    pub async fn create(canvas: HtmlCanvasElement) -> Result<WebReader, JsValue> {
        Ok(Self {
            inner: RefCell::new(Some(WebReaderState::create(canvas).await?)),
        })
    }

    pub fn renderer_kind(&self) -> Result<String, JsValue> {
        self.with_inner(|inner| Ok(inner.renderer_kind()))
    }

    pub fn renderer_capabilities(&self) -> Result<String, JsValue> {
        self.with_inner(|inner| Ok(inner.renderer_capabilities()))
    }

    pub fn open_bytes(
        &self,
        bytes: &[u8],
        file_name: &str,
        logical_width: u32,
        logical_height: u32,
        surface_width: u32,
        surface_height: u32,
        scale_factor: f32,
    ) -> Result<JsValue, JsValue> {
        self.with_inner_mut(|inner| {
            inner.open_bytes(
                bytes,
                file_name,
                logical_width,
                logical_height,
                surface_width,
                surface_height,
                scale_factor,
            )
        })
    }

    pub fn resize(
        &self,
        logical_width: u32,
        logical_height: u32,
        surface_width: u32,
        surface_height: u32,
        scale_factor: f32,
    ) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| {
            inner.resize(
                logical_width,
                logical_height,
                surface_width,
                surface_height,
                scale_factor,
            )
        })
    }

    pub fn render_frame(&self) -> Result<(), JsValue> {
        self.with_inner_mut(WebReaderState::render_frame)
    }

    pub fn tick(&self, budget_ms: f64) -> Result<u8, JsValue> {
        self.with_inner_mut(|inner| inner.tick(budget_ms))
    }

    pub fn close(&self) {
        let _ = self.with_inner_mut(|inner| {
            inner.close();
            Ok(())
        });
    }

    pub fn is_open(&self) -> bool {
        self.inner
            .try_borrow()
            .ok()
            .and_then(|inner| inner.as_ref().map(WebReaderState::is_open))
            .unwrap_or(false)
    }

    pub fn toc_json(&self) -> Result<JsValue, JsValue> {
        self.with_inner(WebReaderState::toc_json)
    }

    pub fn state_json(&self) -> Result<JsValue, JsValue> {
        self.with_inner(WebReaderState::state_json)
    }

    pub fn locator_json(&self) -> Result<JsValue, JsValue> {
        self.with_inner(WebReaderState::locator_json)
    }

    pub fn restore_locator_json(&self, locator_json: &str) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.restore_locator_json(locator_json))
    }

    pub fn navigate_toc(&self, id: &str) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.navigate_toc(id))
    }

    pub fn search(&self, query: &str, max_results: usize) -> Result<JsValue, JsValue> {
        self.with_inner(|inner| inner.search(query, max_results))
    }

    pub fn set_highlights_json(&self, ranges_json: &str) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_highlights_json(ranges_json))
    }

    pub fn clear_highlights(&self) -> Result<(), JsValue> {
        self.with_inner_mut(WebReaderState::clear_highlights)
    }

    pub fn set_focus_json(&self, ranges_json: &str) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_focus_json(ranges_json))
    }

    pub fn clear_focus(&self) -> Result<(), JsValue> {
        self.with_inner_mut(WebReaderState::clear_focus)
    }

    pub fn style_json(&self) -> Result<JsValue, JsValue> {
        self.with_inner(WebReaderState::style_json)
    }

    pub fn set_style_json(&self, style_json: &str) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_style_json(style_json))
    }

    pub fn set_font_size(&self, font_size: f32) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_font_size(font_size))
    }

    pub fn set_line_height(&self, line_height: f32) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_line_height(line_height))
    }

    pub fn set_paragraph_indent(&self, indent_em: f32) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_paragraph_indent(indent_em))
    }

    pub fn set_font_family(&self, category: &str, family: &str) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_font_family(category, family))
    }

    pub fn set_margins(&self, horizontal: f32, top: f32, bottom: f32) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_margins(horizontal, top, bottom))
    }

    pub fn set_spread_mode(&self, mode: &str) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_spread_mode(mode))
    }

    pub fn set_colors(
        &self,
        fg_r: u8,
        fg_g: u8,
        fg_b: u8,
        bg_r: u8,
        bg_g: u8,
        bg_b: u8,
    ) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.set_colors(fg_r, fg_g, fg_b, bg_r, bg_g, bg_b))
    }

    pub fn page_info(&self) -> Result<JsValue, JsValue> {
        self.with_inner_mut(WebReaderState::page_info)
    }

    pub fn page_text(&self) -> Result<JsValue, JsValue> {
        self.with_inner_mut(WebReaderState::page_text)
    }

    pub fn navigate_next(&self) -> Result<u8, JsValue> {
        self.with_inner_mut(WebReaderState::navigate_next)
    }

    pub fn navigate_previous(&self) -> Result<u8, JsValue> {
        self.with_inner_mut(WebReaderState::navigate_previous)
    }

    pub fn pointer_down(&self, id: u32, x: f32, y: f32, timestamp_ms: f64) -> Result<u8, JsValue> {
        self.with_inner_mut(|inner| inner.pointer_down(id, x, y, timestamp_ms))
    }

    pub fn pointer_move(&self, id: u32, x: f32, y: f32, timestamp_ms: f64) -> Result<u8, JsValue> {
        self.with_inner_mut(|inner| inner.pointer_move(id, x, y, timestamp_ms))
    }

    pub fn pointer_up(&self, id: u32, x: f32, y: f32, timestamp_ms: f64) -> Result<u8, JsValue> {
        self.with_inner_mut(|inner| inner.pointer_up(id, x, y, timestamp_ms))
    }

    pub fn pointer_cancel(&self, timestamp_ms: f64) -> Result<u8, JsValue> {
        self.with_inner_mut(|inner| inner.pointer_cancel(timestamp_ms))
    }

    pub fn focus_lost(&self, timestamp_ms: f64) -> Result<u8, JsValue> {
        self.with_inner_mut(|inner| inner.focus_lost(timestamp_ms))
    }

    pub fn lifecycle(&self, state: &str, timestamp_ms: f64) -> Result<(), JsValue> {
        self.with_inner_mut(|inner| inner.lifecycle(state, timestamp_ms))
    }

    pub fn memory_pressure(&self, critical: bool) {
        let _ = self.with_inner_mut(|inner| {
            inner.memory_pressure(critical);
            Ok(())
        });
    }

    pub fn selection_start(&self, x: f32, y: f32) -> Result<bool, JsValue> {
        self.with_inner_mut(|inner| inner.selection_start(x, y))
    }

    pub fn selection_update(&self, x: f32, y: f32) -> Result<bool, JsValue> {
        self.with_inner_mut(|inner| inner.selection_update(x, y))
    }

    pub fn selection_end(&self) -> Result<String, JsValue> {
        self.with_inner_mut(WebReaderState::selection_end)
    }

    pub fn selection_clear(&self) -> Result<bool, JsValue> {
        self.with_inner_mut(WebReaderState::selection_clear)
    }

    pub fn selection_json(&self) -> Result<JsValue, JsValue> {
        self.with_inner(WebReaderState::selection_json)
    }

    pub fn animation_step(&self, timestamp_ms: f64) -> Result<u8, JsValue> {
        self.with_inner_mut(|inner| inner.animation_step(timestamp_ms))
    }

    fn with_inner<T>(
        &self,
        operation: impl FnOnce(&WebReaderState) -> Result<T, JsValue>,
    ) -> Result<T, JsValue> {
        let inner = self.inner.try_borrow().map_err(|_| busy_error())?;
        operation(inner.as_ref().ok_or_else(busy_error)?)
    }

    fn with_inner_mut<T>(
        &self,
        operation: impl FnOnce(&mut WebReaderState) -> Result<T, JsValue>,
    ) -> Result<T, JsValue> {
        // Release the cell borrow before calling engine/GPU/browser code. A
        // synchronous callback sees a temporary busy state but cannot recurse
        // into or permanently poison the exported reader object.
        let mut state = self
            .inner
            .try_borrow_mut()
            .map_err(|_| busy_error())?
            .take()
            .ok_or_else(busy_error)?;
        let result = operation(&mut state);
        self.inner.replace(Some(state));
        result
    }
}

impl Default for WebReader {
    fn default() -> Self {
        Self::new()
    }
}

fn busy_error() -> JsValue {
    JsValue::from_str("reader is busy; retry on the next animation frame")
}

fn pointer_result_code(result: PointerGestureResult) -> u8 {
    match result {
        PointerGestureResult::Ignored => 0,
        PointerGestureResult::Tracking => 1,
        PointerGestureResult::Claimed => 2,
        PointerGestureResult::Turn(PageDirection::Next) => 3,
        PointerGestureResult::Turn(PageDirection::Previous) => 4,
        PointerGestureResult::Cancelled => 5,
    }
}

fn json_to_js(value: &serde_json::Value) -> Result<JsValue, JsValue> {
    serde_json::to_string(value)
        .map(|json| JsValue::from_str(&json))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
