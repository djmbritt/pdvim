//! A trace of how the window came up, for measuring: with
//! `PDVIM_LAUNCH_MARKS=FILE`, each step Rune and pdvim reached on the way
//! to the first page drawn (and to the GPU taking over), in microseconds
//! from the process's start, is appended to FILE as one JSON line a few
//! seconds after the start. Without the variable nothing is recorded.

use std::io::Write as _;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long after the start the marks are written: past the GPU's
/// handover, which is the last step of a launch.
const AFTER: Duration = Duration::from_secs(4);

/// When `run` began, and how long the process had run by then.
static BOOT: OnceLock<(Instant, Duration)> = OnceLock::new();
static PAGE: AtomicBool = AtomicBool::new(false);
static DRAWN: AtomicBool = AtomicBool::new(false);

/// Whether a trace was asked for.
fn on() -> bool {
    BOOT.get().is_some()
}

/// Starts the trace, if asked for: the file is written from a thread of
/// its own once the launch is over.
pub(super) fn start() {
    let Some(path) = std::env::var_os("PDVIM_LAUNCH_MARKS") else {
        return;
    };
    let age = rune::process_age().unwrap_or_default();
    BOOT.get_or_init(|| (Instant::now(), age));
    rune::marks::mark("pdvim.run");
    std::thread::spawn(move || {
        std::thread::sleep(AFTER);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{}", report());
        }
    });
}

/// A step of pdvim's own.
pub(super) fn mark(name: &'static str) {
    if on() {
        rune::marks::mark(name);
    }
}

/// The first page has come back from the backend.
pub(super) fn page_arrived() {
    if on() && !PAGE.swap(true, Ordering::Relaxed) {
        rune::marks::mark("pdvim.page");
    }
}

/// A frame was drawn: the first after the first page came is the page
/// on screen.
pub(super) fn frame_drawn() {
    if on() && PAGE.load(Ordering::Relaxed) && !DRAWN.swap(true, Ordering::Relaxed) {
        rune::marks::mark("pdvim.page.drawn");
    }
}

/// The marks, metrics and notes so far as one JSON object.
fn report() -> String {
    let Some(&(boot, age)) = BOOT.get() else {
        return String::new();
    };
    let micros = |at: Instant| {
        let since = at.checked_duration_since(boot).unwrap_or_default();
        age.saturating_add(since).as_micros()
    };
    let marks: Vec<String> = rune::marks::marks()
        .into_iter()
        .map(|(name, at)| format!("\"{name}\":{}", micros(at)))
        .collect();
    let metrics: Vec<String> = rune::marks::metrics()
        .into_iter()
        .map(|(name, total)| format!("\"{name}\":{total}"))
        .collect();
    let notes: Vec<String> = rune::marks::notes()
        .into_iter()
        .map(|(key, value)| format!("\"{key}\":\"{value}\""))
        .collect();
    format!(
        "{{\"marks\":{{{}}},\"metrics\":{{{}}},\"notes\":{{{}}}}}",
        marks.join(","),
        metrics.join(","),
        notes.join(",")
    )
}
