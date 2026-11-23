//! Going from one paragraph to the next, as vim's `}` and `{`: the start of
//! the paragraph is brought to the top of the window. Paragraphs are found
//! in the text of the pages around the window, read first if need be, as
//! visual mode's `ip` finds them.

use std::ops::Range;

use iced::Task;

use super::{App, Message, Notice, TextFor};
use crate::layout::Rect;
use crate::outline;
use crate::reader::Reader;
use crate::visual::{self, Text};

/// The most pages past the window looked through for paragraphs, however
/// large the count.
const MOST_PAGES: u32 = 10;

impl App {
    /// Goes a number of paragraphs on from the top of the window, or back,
    /// reading the text of the pages around first if need be.
    pub(super) fn step_paragraph(&mut self, forward: bool, times: u32) -> Task<Message> {
        let Some(document) = self.document() else {
            return Task::none();
        };
        let id = document.id;
        let pages: Vec<u32> = around(&document.reader, forward, times).collect();
        if pages.iter().all(|page| document.text.contains_key(page)) {
            self.go_to_paragraph(forward, times);
            return Task::none();
        }
        self.lines_wanted = Some((id, TextFor::Paragraph { forward, times }));
        self.read_text(pages)
    }

    /// Whether the text of the pages around the window is read, to go a
    /// number of paragraphs on or back.
    pub(super) fn paragraphs_read(&self, forward: bool, times: u32) -> bool {
        self.document().is_some_and(|document| {
            around(&document.reader, forward, times).all(|page| document.text.contains_key(&page))
        })
    }

    /// Brings the start of the paragraph a number of paragraphs on from the
    /// top of the window, or back, to the top.
    pub(super) fn go_to_paragraph(&mut self, forward: bool, times: u32) {
        let Some(document) = self.document_mut() else {
            return;
        };
        let reader = &document.reader;
        let starts = starts(&document.text, around(reader, forward, times));
        let offsets: Vec<f32> = starts
            .iter()
            .map(|&(page, top)| {
                let spot = Rect {
                    left: 0.0,
                    top,
                    right: 0.0,
                    bottom: top,
                };
                reader
                    .area_on_screen(page, spot)
                    .map_or(f32::NAN, |area| area.top)
            })
            .collect();
        let Some(&(page, top)) =
            outline::step(&offsets, forward, times).and_then(|index| starts.get(index))
        else {
            self.notice = Notice::Info(
                if forward {
                    "no paragraph after this one"
                } else {
                    "no paragraph before this one"
                }
                .to_owned(),
            );
            return;
        };
        document.jumps.record(document.reader.place());
        document.reader.go_to_spot(page, Some(top));
    }
}

/// The pages paragraphs are looked for in: those in the window, the one
/// before them, and as many after, or before, as the count, within reason.
fn around(reader: &Reader, forward: bool, times: u32) -> Range<u32> {
    let visible = reader.visible_pages();
    let more = times.clamp(1, MOST_PAGES);
    let (before, after) = if forward { (1, more) } else { (more, 1) };
    // The page before the first is read too, to tell whether a paragraph
    // goes on over the page.
    let start = visible.start.saturating_sub(before.saturating_add(1));
    let end = visible.end.saturating_add(after).min(reader.page_count());
    start..end
}

/// Where the paragraphs of some pages start, in order: each page, and how
/// far down it the paragraph's first line is. A page's first line starts
/// one only if the paragraph the page before ends with does not go on.
fn starts(text: &Text, pages: Range<u32>) -> Vec<(u32, f32)> {
    let mut starts = Vec::new();
    for page in pages {
        let Some(lines) = text.get(&page) else {
            continue;
        };
        for row in visual::paragraph_starts(lines) {
            if row == 0 && !visual::page_starts_paragraph(text, page) {
                continue;
            }
            let top = lines
                .get(row)
                .and_then(|line| line.chars.iter().map(|c| c.area.top).min_by(f32::total_cmp));
            if let Some(top) = top {
                starts.push((page, top));
            }
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::showing;
    use crate::app::{Action, App};
    use crate::backend::{TextChar, TextLine};

    /// A line of text this far down, full across the page or short.
    fn line(top: f32, full: bool) -> TextLine {
        let length = if full { 40 } else { 12 };
        TextLine {
            chars: (0..length)
                .map(|column| {
                    let left = f32::from(u16::try_from(column).unwrap()).mul_add(10.0, 72.0);
                    TextChar {
                        character: if column == 0 { 'A' } else { 'a' },
                        area: Rect {
                            left,
                            top,
                            right: left + 10.0,
                            bottom: top + 12.0,
                        },
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn paragraphs_start_after_short_lines_but_not_at_every_page() {
        // Page 0: a paragraph of three lines, ending short, then one going
        // on over the page; page 1 goes on with it, ends it short, and
        // starts a new one.
        let text = Text::from([
            (
                0,
                vec![
                    line(100.0, true),
                    line(116.0, true),
                    line(132.0, false),
                    line(148.0, true),
                    line(164.0, true),
                ],
            ),
            (
                1,
                vec![
                    line(100.0, true),
                    line(116.0, true),
                    line(132.0, true),
                    line(148.0, false),
                    line(164.0, true),
                ],
            ),
        ]);
        // The second page's first line goes on with the first page's last
        // paragraph, which does not end short.
        assert_eq!(starts(&text, 0..2), [(0, 100.0), (0, 148.0), (1, 164.0)]);
    }

    #[test]
    fn a_brace_brings_the_next_paragraph_to_the_top() {
        let (mut app, _) = showing("links.pdf");
        // Three paragraphs on the first page, each ending short; the second
        // page has no text.
        app.document_mut().unwrap().text = Text::from([
            (
                0,
                vec![
                    line(100.0, true),
                    line(116.0, false),
                    line(300.0, true),
                    line(316.0, false),
                    line(500.0, true),
                ],
            ),
            (1, Vec::new()),
        ]);
        let at_top = |app: &App, down: f32| {
            let spot = Rect {
                left: 0.0,
                top: down,
                right: 0.0,
                bottom: down,
            };
            let area = app
                .document()
                .unwrap()
                .reader
                .area_on_screen(0, spot)
                .unwrap();
            area.top.abs() < 1.0
        };
        let brace = |app: &mut App, forward: bool, times: u32| {
            let _: Task<Message> = app.act(Action::Paragraph { forward, times });
        };
        brace(&mut app, true, 1);
        assert!(at_top(&app, 100.0));
        brace(&mut app, true, 1);
        assert!(at_top(&app, 300.0));
        brace(&mut app, false, 1);
        assert!(at_top(&app, 100.0));
        brace(&mut app, true, 2);
        assert!(at_top(&app, 500.0));
        // Nothing after the last: it stays, and says so.
        brace(&mut app, true, 1);
        assert!(at_top(&app, 500.0));
    }
}
