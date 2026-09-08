use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use rebook_engine::{Engine, EngineBook, EngineReader, ReaderConfig};
use rebook_layout::{LayoutViewport, ReaderStyle};
use rebook_reader::PageDirection;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::render::scene::{OverlaySet, PageSceneKey, SpreadSceneKey};
use crate::render::{ReaderCompositor, SpreadSceneCache};
use crate::surface::SurfaceRenderer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyState {
    Clean,
    Scene,
    Surface,
    Animation,
}

pub struct DemoApplication {
    book_path: PathBuf,
    engine: Engine,
    book: Option<EngineBook>,
    reader: Option<EngineReader>,
    gpu: Option<SurfaceRenderer>,
    scene_cache: SpreadSceneCache,
    window: Option<Arc<Window>>,
    viewport: PhysicalSize<u32>,
    dirty: DirtyState,
    layout_generation: u64,
    show_metrics: bool,
}

impl DemoApplication {
    pub fn new(book_path: PathBuf) -> Self {
        Self {
            book_path,
            engine: Engine::default(),
            book: None,
            reader: None,
            gpu: None,
            scene_cache: SpreadSceneCache::new(16),
            window: None,
            viewport: PhysicalSize::new(800, 1000),
            dirty: DirtyState::Scene,
            layout_generation: 0,
            show_metrics: false,
        }
    }

    fn ensure_reader(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.book.is_none() {
            let bytes = std::fs::read(&self.book_path)
                .map_err(|e| format!("Failed to read {}: {e}", self.book_path.display()))?;
            let file_name = self
                .book_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("book");
            let book = self
                .engine
                .open_bytes(bytes, file_name)
                .map_err(|e| format!("Failed to open book: {e}"))?;
            self.book = Some(book);
        }

        let book = self.book.as_ref().unwrap();
        if self.reader.is_none() {
            let reader = self
                .engine
                .create_reader(
                    book,
                    ReaderConfig {
                        viewport: LayoutViewport { width, height },
                        style: ReaderStyle::default(),
                        locator: None,
                    },
                )
                .map_err(|e| format!("Failed to create reader: {e}"))?;
            self.reader = Some(reader);
            self.layout_generation += 1;
        }

        Ok(())
    }

    fn render_current_spread(&mut self) -> Result<(), String> {
        let Some(reader) = self.reader.as_mut() else {
            return Ok(());
        };
        let Some(gpu) = self.gpu.as_mut() else {
            return Ok(());
        };

        let start_time = Instant::now();
        let spread = reader
            .current_spread()
            .map_err(|e| format!("Failed to get spread: {e}"))?;

        let snapshot = reader.snapshot();
        let key = SpreadSceneKey {
            primary: PageSceneKey {
                position: snapshot.location.into(),
                layout_generation: self.layout_generation,
            },
            secondary: None,
            width: self.viewport.width,
            height: self.viewport.height,
        };

        // Pin current spread
        self.scene_cache.clear_pins();
        self.scene_cache.pin(key.clone());

        let layers = self.scene_cache.get_or_build(&key, &spread);
        for image in layers.images.iter() {
            gpu.mark_image_dirty(image);
        }

        let overlays = OverlaySet::default();
        let scene = ReaderCompositor::compose_spread_scene(&layers, &spread, &overlays, None);
        gpu.render_frame(&scene)?;

        let elapsed = start_time.elapsed();
        if self.show_metrics
            && let Some(window) = &self.window
        {
            let cache_metrics = self.scene_cache.metrics();
            window.set_title(&format!(
                "Torto Engine Demo - Spread {}/{} (sec {}) | Frame: {:.2?} | Cache: hits={}, misses={}, builds={}",
                snapshot.location.page_index + 1,
                snapshot.location.page_count,
                snapshot.location.section_index,
                elapsed,
                cache_metrics.hits,
                cache_metrics.misses,
                cache_metrics.builds
            ));
        }

        self.dirty = DirtyState::Clean;
        Ok(())
    }
}

impl ApplicationHandler for DemoApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Torto Engine Demo")
            .with_inner_size(LogicalSize::new(800u32, 1000u32))
            .with_min_inner_size(LogicalSize::new(400u32, 500u32));

        let window = match event_loop.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("Failed to create window: {e}");
                event_loop.exit();
                return;
            }
        };

        let size = window.inner_size();
        self.viewport = size;

        if let Err(e) = self.ensure_reader(size.width.max(1), size.height.max(1)) {
            eprintln!("Initialization error: {e}");
            event_loop.exit();
            return;
        }

        let gpu = match pollster::block_on(SurfaceRenderer::new(Arc::clone(&window))) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("Failed to initialize GPU: {e}");
                event_loop.exit();
                return;
            }
        };

        self.gpu = Some(gpu);
        self.window = Some(window);
        self.dirty = DirtyState::Scene;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    self.viewport = new_size;
                    if let Some(gpu) = self.gpu.as_mut() {
                        gpu.resize(new_size);
                    }
                    if let Some(reader) = self.reader.as_mut() {
                        let _ = reader.resize(LayoutViewport {
                            width: new_size.width,
                            height: new_size.height,
                        });
                        self.layout_generation += 1;
                        self.scene_cache.clear();
                    }
                    self.dirty = DirtyState::Scene;
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if self.dirty != DirtyState::Clean
                    && let Err(e) = self.render_current_spread()
                {
                    eprintln!("Render error: {e}");
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        event_loop.exit();
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight | KeyCode::Space) => {
                        if let Some(reader) = self.reader.as_mut() {
                            let _ = reader.try_turn_page(PageDirection::Next);
                            self.dirty = DirtyState::Scene;
                            if let Some(w) = &self.window {
                                w.request_redraw();
                            }
                        }
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        if let Some(reader) = self.reader.as_mut() {
                            let _ = reader.try_turn_page(PageDirection::Previous);
                            self.dirty = DirtyState::Scene;
                            if let Some(w) = &self.window {
                                w.request_redraw();
                            }
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyR) => {
                        self.scene_cache.clear();
                        self.dirty = DirtyState::Scene;
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    PhysicalKey::Code(KeyCode::F1) => {
                        self.show_metrics = !self.show_metrics;
                        if !self.show_metrics
                            && let Some(w) = &self.window
                        {
                            w.set_title("Torto Engine Demo");
                        }
                        self.dirty = DirtyState::Scene;
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
