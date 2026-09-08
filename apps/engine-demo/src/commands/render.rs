use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use rebook_engine::{Engine, ReaderConfig};
use rebook_layout::{LayoutViewport, ReaderStyle};

use crate::render::OffscreenTarget;

pub fn run(book_path: &Path, output: &Path, width: u32, height: u32) -> Result<(), String> {
    let start_read = Instant::now();
    let bytes = fs::read(book_path)
        .map_err(|e| format!("Failed to read book file {}: {e}", book_path.display()))?;
    let _read_duration = start_read.elapsed();

    let file_name = book_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("Invalid file name: {}", book_path.display()))?;

    let engine = Engine::default();
    let book = engine
        .open_bytes(Arc::<[u8]>::from(bytes), file_name)
        .map_err(|e| format!("Failed to open book {}: {e}", book_path.display()))?;

    let mut reader = engine
        .create_reader(
            &book,
            ReaderConfig {
                viewport: LayoutViewport { width, height },
                style: ReaderStyle::default(),
                locator: None,
            },
        )
        .map_err(|e| format!("Failed to create reader for {}: {e}", book_path.display()))?;

    let spread = reader
        .current_spread()
        .map_err(|e| format!("Failed to get current spread: {e}"))?;

    let mut target = pollster::block_on(OffscreenTarget::new())?;
    let (png_bytes, metrics) = target.render_spread_to_png(&spread, width, height)?;

    fs::write(output, &png_bytes)
        .map_err(|e| format!("Failed to write output image {}: {e}", output.display()))?;

    println!("=== Render Output ===");
    println!("Output File:              {}", output.display());
    println!(
        "Dimensions:               {}x{}",
        metrics.width, metrics.height
    );
    println!("Referenced Images:        {}", metrics.image_count);
    println!("Scene Build CPU Time:     {:.2?}", metrics.scene_build);
    println!("Vello Submit Time:        {:.2?}", metrics.gpu_submit);
    println!("GPU Readback Time:        {:.2?}", metrics.readback);
    println!("PNG Encode Time:          {:.2?}", metrics.png_encode);

    Ok(())
}
