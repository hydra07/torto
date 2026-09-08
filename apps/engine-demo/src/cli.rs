use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Inspect {
        book: PathBuf,
    },
    Paginate {
        book: PathBuf,
        width: u32,
        height: u32,
        pages: Option<usize>,
    },
    Render {
        book: PathBuf,
        output: PathBuf,
        width: u32,
        height: u32,
    },
    Window {
        book: PathBuf,
    },
}

#[allow(clippy::too_many_lines)]
pub fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut iter = args.into_iter();
    let _bin_name = iter.next();

    let subcmd = iter.next().ok_or_else(|| {
        "No command provided. Usage: rebook-engine-demo <inspect|paginate|render|window> [OPTIONS] BOOK"
            .to_string()
    })?;

    match subcmd.as_str() {
        "inspect" => {
            let mut book = None;
            for arg in iter {
                if !arg.starts_with('-') && book.is_none() {
                    book = Some(PathBuf::from(arg));
                }
            }
            let book = book.ok_or_else(|| "Usage: rebook-engine-demo inspect BOOK".to_string())?;
            Ok(Command::Inspect { book })
        }
        "paginate" => {
            let mut book = None;
            let mut width = 800;
            let mut height = 1000;
            let mut pages = None;

            let mut iter = iter.peekable();
            while let Some(arg) = iter.next() {
                if arg == "--width" {
                    let val = iter.next().ok_or("Missing value for --width")?;
                    width = val
                        .parse::<u32>()
                        .map_err(|e| format!("Invalid width: {e}"))?;
                } else if arg == "--height" {
                    let val = iter.next().ok_or("Missing value for --height")?;
                    height = val
                        .parse::<u32>()
                        .map_err(|e| format!("Invalid height: {e}"))?;
                } else if arg == "--pages" {
                    let val = iter.next().ok_or("Missing value for --pages")?;
                    pages = Some(
                        val.parse::<usize>()
                            .map_err(|e| format!("Invalid pages: {e}"))?,
                    );
                } else if !arg.starts_with('-') && book.is_none() {
                    book = Some(PathBuf::from(arg));
                }
            }
            let book = book.ok_or_else(|| {
                "Usage: rebook-engine-demo paginate BOOK [--width W] [--height H] [--pages N]"
                    .to_string()
            })?;
            Ok(Command::Paginate {
                book,
                width,
                height,
                pages,
            })
        }
        "render" => {
            let mut book = None;
            let mut output = None;
            let mut width = 800;
            let mut height = 1000;

            let mut iter = iter.peekable();
            while let Some(arg) = iter.next() {
                if arg == "--output" || arg == "-o" {
                    let val = iter.next().ok_or("Missing value for --output")?;
                    output = Some(PathBuf::from(val));
                } else if arg == "--width" {
                    let val = iter.next().ok_or("Missing value for --width")?;
                    width = val
                        .parse::<u32>()
                        .map_err(|e| format!("Invalid width: {e}"))?;
                } else if arg == "--height" {
                    let val = iter.next().ok_or("Missing value for --height")?;
                    height = val
                        .parse::<u32>()
                        .map_err(|e| format!("Invalid height: {e}"))?;
                } else if !arg.starts_with('-') && book.is_none() {
                    book = Some(PathBuf::from(arg));
                }
            }
            let book = book.ok_or_else(|| {
                "Usage: rebook-engine-demo render BOOK --output PAGE.png".to_string()
            })?;
            let output = output.ok_or_else(|| {
                "Missing required --output parameter for render command".to_string()
            })?;
            Ok(Command::Render {
                book,
                output,
                width,
                height,
            })
        }
        "window" => {
            let mut book = None;
            for arg in iter {
                if !arg.starts_with('-') && book.is_none() {
                    book = Some(PathBuf::from(arg));
                }
            }
            let book = book.ok_or_else(|| "Usage: rebook-engine-demo window BOOK".to_string())?;
            Ok(Command::Window { book })
        }
        other => Err(format!(
            "Unknown command: {other}. Expected inspect, paginate, render, or window."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inspect_command() {
        let args = vec![
            "rebook-engine-demo".into(),
            "inspect".into(),
            "book.epub".into(),
        ];
        assert_eq!(
            parse_args(args).unwrap(),
            Command::Inspect {
                book: PathBuf::from("book.epub"),
            }
        );
    }

    #[test]
    fn parses_paginate_command_with_defaults_and_options() {
        let args = vec![
            "rebook-engine-demo".into(),
            "paginate".into(),
            "book.epub".into(),
            "--width".into(),
            "1024".into(),
            "--height".into(),
            "768".into(),
            "--pages".into(),
            "5".into(),
        ];
        assert_eq!(
            parse_args(args).unwrap(),
            Command::Paginate {
                book: PathBuf::from("book.epub"),
                width: 1024,
                height: 768,
                pages: Some(5),
            }
        );
    }

    #[test]
    fn parses_render_and_window_commands() {
        let render_args = vec![
            "rebook-engine-demo".into(),
            "render".into(),
            "book.epub".into(),
            "--output".into(),
            "page.png".into(),
        ];
        assert_eq!(
            parse_args(render_args).unwrap(),
            Command::Render {
                book: PathBuf::from("book.epub"),
                output: PathBuf::from("page.png"),
                width: 800,
                height: 1000,
            }
        );

        let window_args = vec![
            "rebook-engine-demo".into(),
            "window".into(),
            "book.epub".into(),
        ];
        assert_eq!(
            parse_args(window_args).unwrap(),
            Command::Window {
                book: PathBuf::from("book.epub"),
            }
        );
    }
}
