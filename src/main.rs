//! pdvim: a minimal PDF reader driven from the keyboard, in the manner of vi.
//!
//! The pieces, from the inside out:
//! - `layout` places pages in a column and `reader` scrolls and zooms over it;
//! - `input` turns key presses into actions and `command` parses `:` commands;
//! - `backend` renders and searches documents with MuPDF on a worker thread;
//! - `history` remembers where each document was left, and `recent` lists
//!   the documents read lately to pick from;
//! - `app` is the window that ties them together.

mod app;
mod backend;
mod command;
mod history;
mod input;
mod layout;
mod num;
mod reader;
mod recent;
mod recolor;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: pdvim [FILE]";

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let path = arguments.next().map(PathBuf::from);
    if arguments.next().is_some() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    if path.as_deref().is_some_and(|path| path.starts_with("-")) {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match app::run(path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pdvim: {error}");
            ExitCode::FAILURE
        }
    }
}
