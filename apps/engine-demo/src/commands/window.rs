use std::path::Path;
use winit::event_loop::EventLoop;

use crate::application::DemoApplication;

pub fn run(book_path: &Path) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
    let mut app = DemoApplication::new(book_path.to_path_buf());
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("Event loop failed: {e}"))?;
    Ok(())
}
