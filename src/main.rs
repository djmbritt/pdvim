//! pdvim: a minimal PDF reader driven from the keyboard, in the manner of vi.
//!
//! The pieces, from the inside out:
//! - `layout` places pages in a column and `reader` scrolls and zooms over it;
//! - `input` turns key presses into actions through the bindings in `keymap`,
//!   and `command` parses `:` commands;
//! - `config` reads those commands from the configuration file;
//! - `backend` renders and searches documents with MuPDF on a worker thread;
//! - `history` remembers where each document was left, and `recent` lists
//!   the documents read lately to pick from;
//! - `app` is the window that ties them together.

mod app;
mod backend;
mod command;
mod config;
mod history;
mod input;
mod keymap;
mod layout;
mod num;
mod reader;
mod recent;
mod recolor;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: pdvim [FILE]...";

fn main() -> ExitCode {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if paths
        .iter()
        .any(|path| path.as_os_str().as_encoded_bytes().starts_with(b"-"))
    {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match app::run(paths) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pdvim: {error}");
            ExitCode::FAILURE
        }
    }
}
