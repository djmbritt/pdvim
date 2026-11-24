//! Jumping to text in view by typing some of it, then a match's label, as
//! flash.nvim's `s` does. From normal mode just the cursor lands there,
//! with nothing selected: it moves without selecting until `v` or `V`
//! starts a selection from it. While selecting, the cursor moves there and
//! the selection runs to it.

use iced::widget::canvas;
use iced::{Point, Task};

use super::{
    App, Document, LINK, Message, Pending, TextFor, Wanted, draw_text_label, label_size, rectangle,
};
use crate::flash::{Flash, Outcome, Press};
use crate::input::{Key, PromptBinding};
use crate::keymap::Match;
use crate::visual::{self, Spot, Visual};

impl App {
    /// Asks for text to jump to, once the text in view is read.
    pub(super) fn start_flash(&mut self) -> Task<Message> {
        self.pending = Pending::new();
        let Some(document) = self.document() else {
            return Task::none();
        };
        let id = document.id;
        let pages: Vec<u32> = document.reader.visible_pages().collect();
        if pages.iter().all(|page| document.text.contains_key(page)) {
            self.flash = Some(Flash::new());
            return Task::none();
        }
        self.lines_wanted = Some((id, TextFor::Flash));
        self.read_text(pages)
    }

    /// Takes a key typed while text to jump to is asked for.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the other keys do nothing here"
    )]
    pub(super) fn on_flash_key(&mut self, key: Key) -> Task<Message> {
        let Some(mut flash) = self.flash.take() else {
            return Task::none();
        };
        let press = match self.keymaps.prompt.lookup(&[key]) {
            Match::Bound(PromptBinding::Submit) => Press::Submit,
            Match::Bound(PromptBinding::Cancel) => Press::Cancel,
            Match::Bound(PromptBinding::RubOut) => Press::RubOut,
            Match::Bound(_) | Match::Command(_) | Match::Prefix | Match::None => match key {
                Key::Char(character) => Press::Char(character),
                Key::ShiftSpace => Press::Char(' '),
                _ => Press::Other,
            },
        };
        let order = self.lines_in_view();
        // Distances are measured from visual mode's cursor, or else from the
        // top of the view.
        let from = self.visual.as_ref().map_or_else(
            || {
                order
                    .first()
                    .map_or(Spot::line_start(0, 0), |&(page, line)| {
                        Spot::line_start(page, line)
                    })
            },
            Visual::cursor,
        );
        let outcome = {
            let Some(document) = self.document() else {
                return Task::none();
            };
            let lines: Vec<_> = order
                .iter()
                .filter_map(|&(page, line)| {
                    Some((page, line, document.text.get(&page)?.get(line)?))
                })
                .collect();
            flash.feed(press, &lines, from)
        };
        match outcome {
            Outcome::Stay => {
                self.flash = Some(flash);
                Task::none()
            }
            Outcome::Cancel => Task::none(),
            Outcome::Jump(spot) => self.land(spot),
        }
    }

    /// Moves the cursor to a spot: the selection, if one is being made,
    /// runs to it; otherwise just the cursor lands there.
    fn land(&mut self, spot: Spot) -> Task<Message> {
        if let Some(visual) = &mut self.visual {
            if visual.selecting() {
                visual.move_to(spot);
            } else {
                visual.place_at(spot);
            }
            return self.follow_cursor();
        }
        self.visual = Some(Visual::cursor_at(spot));
        // Known, so that the bottom line can show the note at the cursor.
        let markings = self.with_markings(Wanted::Refresh);
        Task::batch([self.follow_cursor(), markings])
    }
}

/// Marks each match of the text typed, and puts its label just after it.
pub(super) fn draw(frame: &mut canvas::Frame, document: &Document, flash: &Flash) {
    for target in flash.targets() {
        let mut last = None;
        for step in 0..target.len {
            let spot = Spot {
                index: target.start.index.saturating_add(step),
                ..target.start
            };
            let Some(area) = visual::character(&document.text, spot)
                .and_then(|area| document.reader.area_on_screen(spot.page, area))
            else {
                continue;
            };
            let shown = rectangle(area);
            frame.fill_rectangle(shown.position(), shown.size(), LINK);
            last = Some(area);
        }
        if let Some(area) = last
            && !target.label.is_empty()
        {
            // Over the characters after the match, which a label never
            // could be typed as.
            let top = area.top + (area.bottom - area.top - label_size().height) / 2.0;
            draw_text_label(frame, &target.label, Point::new(area.right, top));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::mode::Mode;
    use crate::app::testing::Session;
    use crate::visual::Spot;

    #[test]
    fn enter_puts_a_cursor_at_the_nearest_match() {
        let mut session = Session::open("text.pdf");
        session.keys("swor<Enter>");
        assert_eq!(session.app.mode(), Mode::Cursor);
        let cursor = session.app.visual.as_ref().unwrap().cursor();
        assert_eq!(
            cursor,
            Spot {
                page: 0,
                line: 0,
                index: 7
            }
        );
        // v starts selecting from the cursor; y copies and leaves.
        session.keys("vey");
        assert_eq!(session.copied, ["world."]);
        assert!(session.app.visual.is_none());
    }

    #[test]
    fn while_selecting_a_label_moves_the_cursor_and_the_selection_runs_to_it() {
        let mut session = Session::open("text.pdf");
        // The first line's label is the first one made.
        session.keys("vaa");
        assert_eq!(session.app.mode(), Mode::Visual);
        session.keys("swor");
        let flash = session.app.flash.as_ref().unwrap();
        let words = flash
            .targets()
            .iter()
            .find(|target| target.start.line == 1)
            .unwrap();
        let label = words.label.clone();
        session.keys(&label);
        assert_eq!(
            session.selected(),
            "Hello, world. A line of text second line -- with w"
        );
    }

    #[test]
    fn escape_gives_up_the_jump() {
        let mut session = Session::open("text.pdf");
        session.keys("swo<Esc>");
        assert!(session.app.flash.is_none());
        assert!(session.app.visual.is_none());
    }
}
