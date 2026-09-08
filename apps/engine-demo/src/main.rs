pub mod application;
mod cli;
mod commands;
pub mod render;
pub mod surface;

use std::env;
use std::process;

fn main() {
    let command = match cli::parse_args(env::args()) {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("Error: {err}");
            process::exit(1);
        }
    };

    let result = match command {
        cli::Command::Inspect { book } => commands::inspect::run(&book),
        cli::Command::Paginate {
            book,
            width,
            height,
            pages,
        } => commands::paginate::run(&book, width, height, pages),
        cli::Command::Render {
            book,
            output,
            width,
            height,
        } => commands::render::run(&book, &output, width, height),
        cli::Command::Window { book } => commands::window::run(&book),
    };

    if let Err(err) = result {
        eprintln!("Error: {err}");
        process::exit(1);
    }
}
