use std::path::Path;

pub fn run(_book_path: &Path, _output: &Path, _width: u32, _height: u32) -> Result<(), String> {
    Err(
        "The 'render' command will be implemented in Milestone 3 (Offscreen Vello Rendering)"
            .to_string(),
    )
}
