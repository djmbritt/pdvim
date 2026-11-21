//! Hiding the tab bar and the status bar while reading, and filling the
//! screen.

use iced::{Task, window};

use super::{App, Message, Notice, mode::Mode};

/// Which bars show while reading, and whether the window fills the screen.
#[derive(Debug, Clone, Copy)]
pub(super) struct Bars {
    /// The tab bar, when there is more than one tab.
    pub tabs: bool,
    /// The status bar. Hidden, it still shows while it has something to
    /// show, over the bottom of the pages.
    pub status: bool,
    fullscreen: bool,
}

impl Default for Bars {
    fn default() -> Self {
        Self {
            tabs: true,
            status: true,
            fullscreen: false,
        }
    }
}

impl Bars {
    /// Hides both bars, or shows both when both are hidden.
    pub const fn toggle(&mut self) {
        let shown = !(self.tabs || self.status);
        self.tabs = shown;
        self.status = shown;
    }

    /// Fills the screen with the window, or stops.
    pub fn toggle_fullscreen(&mut self) -> Task<Message> {
        self.fullscreen = !self.fullscreen;
        let mode = if self.fullscreen {
            window::Mode::Fullscreen
        } else {
            window::Mode::Windowed
        };
        window::latest().and_then(move |id| window::set_mode(id, mode))
    }
}

impl App {
    /// Whether the status bar has something to show besides the file and
    /// the page: a message, keys half typed, or a mode other than reading.
    pub(super) fn bar_wanted(&self) -> bool {
        !matches!(self.notice, Notice::None)
            || self.awaiting_mark.is_some()
            || !self.pending.display().is_empty()
            || self.mode() != Mode::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_hides_both_bars_then_shows_both() {
        let mut bars = Bars::default();
        bars.toggle();
        assert!(!bars.tabs && !bars.status);
        bars.toggle();
        assert!(bars.tabs && bars.status);
        // One hidden: both go.
        bars.status = false;
        bars.toggle();
        assert!(!bars.tabs && !bars.status);
    }
}
