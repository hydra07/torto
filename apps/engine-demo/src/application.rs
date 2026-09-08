use std::fs;
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

use crate::metrics::{FrameStats, MetricsFormat, PipelineMetrics, RollingWindowMetrics};
use crate::render::scene::{OverlaySet, PageSceneKey, SpreadSceneKey};
use crate::render::scene_cache::{MemoryPressure, ResourceProfile};
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
    metrics_format: Option<MetricsFormat>,
    metrics_file: Option<PathBuf>,
    profile: ResourceProfile,
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
    rolling_metrics: RollingWindowMetrics,
    pipeline_metrics: Option<PipelineMetrics>,
    last_frame_instant: Option<Instant>,
}

impl DemoApplication {
    pub fn new(
        book_path: PathBuf,
        metrics_format: Option<MetricsFormat>,
        metrics_file: Option<PathBuf>,
        profile: ResourceProfile,
    ) -> Self {
        Self {
            book_path,
            metrics_format,
            metrics_file,
            profile,
            engine: Engine::default(),
            book: None,
            reader: None,
            gpu: None,
            scene_cache: SpreadSceneCache::with_profile(profile),
            window: None,
            viewport: PhysicalSize::new(800, 1000),
            dirty: DirtyState::Scene,
            layout_generation: 0,
            show_metrics: false,
            rolling_metrics: RollingWindowMetrics::new(120),
            pipeline_metrics: None,
            last_frame_instant: None,
        }
    }

    fn ensure_reader(&mut self, width: u32, height: u32) -> Result<(), String> {
        let mut file_read_dur = std::time::Duration::ZERO;
        let mut open_dur = std::time::Duration::ZERO;

        if self.book.is_none() {
            let start_read = Instant::now();
            let bytes = fs::read(&self.book_path)
                .map_err(|e| format!("Failed to read {}: {e}", self.book_path.display()))?;
            file_read_dur = start_read.elapsed();

            let file_name = self
                .book_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("book");

            let start_open = Instant::now();
            let book = self
                .engine
                .open_bytes(bytes, file_name)
                .map_err(|e| format!("Failed to open book: {e}"))?;
            open_dur = start_open.elapsed();
            self.book = Some(book);
        }

        let book = self.book.as_ref().unwrap();
        if self.reader.is_none() {
            let start_reader = Instant::now();
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
            let reader_dur = start_reader.elapsed();
            self.reader = Some(reader);
            self.layout_generation += 1;

            self.pipeline_metrics = Some(PipelineMetrics::from_durations(
                file_read_dur,
                open_dur,
                reader_dur,
                std::time::Duration::ZERO,
                std::time::Duration::ZERO,
                None,
            ));
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

        let frame_start = Instant::now();
        let frame_interval = self
            .last_frame_instant
            .map_or(std::time::Duration::from_millis(16), |prev| prev.elapsed());
        self.last_frame_instant = Some(frame_start);

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

        let pre_builds = self.scene_cache.metrics().builds;
        let layers = self.scene_cache.get_or_build(&key, &spread);
        let scene_builds = self.scene_cache.metrics().builds - pre_builds;

        for image in layers.images.iter() {
            gpu.mark_image_dirty(image);
        }

        let overlays = OverlaySet::default();
        let scene = ReaderCompositor::compose_spread_scene(&layers, &spread, &overlays, None);
        gpu.render_frame(&scene)?;

        let cpu_duration = frame_start.elapsed();

        let missed_60hz = cpu_duration > std::time::Duration::from_millis(16);
        let missed_120hz = cpu_duration > std::time::Duration::from_millis(8);

        let stats = FrameStats {
            interval_ms: frame_interval.as_secs_f64() * 1000.0,
            cpu_duration_ms: cpu_duration.as_secs_f64() * 1000.0,
            gpu_duration_ms: None,
            scene_builds,
            target_recreations: 0,
            missed_60hz,
            missed_120hz,
        };
        self.rolling_metrics.record_frame(&stats);

        if self.show_metrics
            && let Some(window) = &self.window
        {
            let (p50, p95, _p99) = self.rolling_metrics.cpu_percentiles();
            let cache_metrics = self.scene_cache.metrics();
            window.set_title(&format!(
                "Torto Demo - Page {}/{} | CPU p50: {:.2?} p95: {:.2?} | Cache: hits={}, misses={}, builds={}",
                snapshot.location.page_index + 1,
                snapshot.location.page_count,
                p50,
                p95,
                cache_metrics.hits,
                cache_metrics.misses,
                cache_metrics.builds
            ));
        }

        self.dirty = DirtyState::Clean;
        Ok(())
    }

    fn dump_metrics_if_requested(&self) {
        if self.metrics_format.is_none() && self.metrics_file.is_none() {
            return;
        }

        let ctx = crate::metrics::ReportContext {
            viewport: (self.viewport.width, self.viewport.height),
            adapter_backend: "wgpu-vello-surface".to_string(),
            spread_mode: "Single".to_string(),
            transition_kind: "None".to_string(),
            resource_profile: format!("{:?}", self.profile),
        };
        let report = self.rolling_metrics.build_report(
            ctx,
            self.pipeline_metrics.clone(),
            self.scene_cache.metrics().clone(),
        );

        let formatted = match self.metrics_format {
            Some(MetricsFormat::Json) => serde_json::to_string_pretty(&report).unwrap_or_default(),
            Some(MetricsFormat::Text) => format!("{report:#?}"),
            None => String::new(),
        };

        if let Some(format) = self.metrics_format {
            println!("\n=== Final Window Metrics ({format:?}) ===");
            println!("{formatted}");
        }

        if let Some(path) = &self.metrics_file {
            let content = if formatted.is_empty() {
                serde_json::to_string_pretty(&report).unwrap_or_default()
            } else {
                formatted
            };
            let _ = fs::write(path, content);
        }
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
                self.dump_metrics_if_requested();
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
                        self.dump_metrics_if_requested();
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
                    PhysicalKey::Code(KeyCode::KeyM) => {
                        // Simulate critical memory pressure
                        self.scene_cache
                            .handle_memory_pressure(MemoryPressure::Critical);
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
