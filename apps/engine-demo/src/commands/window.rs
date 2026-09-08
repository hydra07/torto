use std::path::{Path, PathBuf};
use winit::event_loop::EventLoop;

use crate::application::DemoApplication;
use crate::metrics::MetricsFormat;
use crate::render::scene_cache::ResourceProfile;

pub fn run(
    book_path: &Path,
    metrics: Option<MetricsFormat>,
    metrics_file: Option<PathBuf>,
    profile: ResourceProfile,
) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
    let mut app = DemoApplication::new(book_path.to_path_buf(), metrics, metrics_file, profile);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("Event loop failed: {e}"))?;
    Ok(())
}
