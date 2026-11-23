//! The `pdvim` command: opens the files named on the command line.

// On Windows, a program for the console gets a console window of its own
// when started from the desktop.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: pdvim [FILE]...\n       pdvim --version";

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the version and usage asked for, and why no window could be made"
)]
fn main() -> ExitCode {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if paths
        .iter()
        .any(|path| matches!(path.to_str(), Some("--version" | "-V")))
    {
        println!("{}", pdvim::ABOUT);
        return ExitCode::SUCCESS;
    }
    if paths
        .iter()
        .any(|path| path.as_os_str().as_encoded_bytes().starts_with(b"-"))
    {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match pdvim::run(paths) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pdvim: {error}");
            ExitCode::FAILURE
        }
    }
}
