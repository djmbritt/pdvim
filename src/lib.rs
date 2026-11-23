//! pdvim: a minimal PDF reader driven from the keyboard, in the manner of vi.
//!
//! The library holds the reader; `main.rs` only reads the command line.
//!
//! The pieces, from the inside out:
//! - `layout` places pages in a column and `reader` scrolls and zooms over it;
//! - `input` turns key presses into actions through the bindings in `keymap`,
//!   and `command` parses `:` commands, which `complete` completes;
//! - `config` reads those commands from the configuration file, and `help`
//!   lists the bindings in force;
//! - `backend` renders and searches documents with MuPDF on a worker thread;
//! - `history` remembers where each document was left, and `recent` lists
//!   the documents read lately to pick from;
//! - `app` is the window that ties them together;
//! - `fuzz` drives the parsers, the key handling and MuPDF with arbitrary
//!   input, for the targets in `fuzz/`, and `bench` times the hot paths,
//!   for the benchmarks in `benches/`.

mod app;
mod backend;
#[doc(hidden)]
pub mod bench;
mod bindings;
mod clipboard;
mod command;
mod complete;
mod config;
mod contents;
mod dirs;
mod flash;
mod fonts;
#[doc(hidden)]
pub mod fuzz;
mod help;
mod hints;
mod history;
mod input;
mod jumps;
mod keymap;
mod layout;
mod manual;
mod markings;
mod num;
mod outline;
mod overview;
mod paths;
mod prose;
mod reader;
mod shell;
mod recall;
mod picker;
mod recolor;
mod theme;
mod visual;

pub use app::run;

/// What this program is, in a line: its name, version, and where it is
/// made, as `:version` and `pdvim --version` say.
pub const ABOUT: &str = concat!(
    "pdvim ",
    env!("CARGO_PKG_VERSION"),
    " - ",
    env!("CARGO_PKG_REPOSITORY"),
);
