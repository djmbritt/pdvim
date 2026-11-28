//! The mouse: clicking a link follows it, dragging over text selects it,
//! a double click selects a word, and dragging off the pages, or with the
//! middle button, moves the view. In the lists, a click picks an entry.

use std::time::{Duration, Instant};

use crate::task::Task;
use crate::ui::Point;

use super::{App, Message, Notice, Sidebar, current_document};
use crate::backend::Link;
use crate::input::LinkGoal;
use crate::markings;
use crate::visual::{self, Kind, Visual};

/// How far the pointer moves, in pixels, before a press is a drag.
const DRAG: f32 = 4.0;

/// How soon a second click must come for a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// What a mouse button did over the pages, at a point of the canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pointer {
    /// A button went down: the left, or else the middle.
    Press(Point, bool),
    /// The pointer moved while a button is held.
    Drag(Point),
    /// The button held went up.
    Release(Point),
}

/// How far the wheel turned: by lines, as a mouse's notches go, or by
/// pixels, as a touchpad goes. Positive is up and to the left, the
/// content moving down and right.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDelta {
    Lines { x: f32, y: f32 },
    Pixels { x: f32, y: f32 },
}

/// A click on an entry of a list, the tab bar or the overview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// A tab in the tab bar.
    Tab(usize),
    /// An entry shown in the list of recent documents, by its place.
    Recent(usize),
    /// An entry shown in the list of tabs, by its place.
    TabList(usize),
    /// A row of the table of contents shown.
    Outline(usize),
    /// A marking in the list of markings, by its index.
    Marking(usize),
    /// A small page in the overview.
    Overview(u32),
    /// An entry shown in the list of matches, by its place.
    Match(usize),
}

/// What the mouse is doing over the pages.
#[derive(Debug, Clone, Default)]
pub struct Mouse {
    press: Option<Press>,
    /// When and where the last click was, to tell a double click.
    last_click: Option<(Instant, Point)>,
    /// A click on a page whose links were not yet found, to follow once
    /// they are: the document, the page, and the spot on it.
    pub(super) waiting: Option<(u64, u32, (f32, f32))>,
    /// Whether control was held as the wheel last turned, to zoom.
    pub(super) control: bool,
}

/// A button held down over the pages.
#[derive(Debug, Clone, Copy)]
struct Press {
    /// Where it went down, and where the pointer was last.
    at: Point,
    last: Point,
    left: bool,
    /// The page under it and the spot on that page, if on a page.
    target: Option<(u32, (f32, f32))>,
    dragged: bool,
    /// Whether it was the second of a double click, which selected a word.
    double: bool,
}

impl Mouse {
    /// Whether the pointer moves the view as it is dragged.
    pub fn panning(&self) -> bool {
        self.press
            .is_some_and(|press| !press.left || press.target.is_none())
    }
}

impl App {
    pub(super) fn on_pointer(&mut self, pointer: Pointer) -> Task<Message> {
        // A press on the pages closes a list or panel over them, and does
        // nothing more.
        if matches!(pointer, Pointer::Press(..)) && self.close_panels() {
            return Task::none();
        }
        self.hints = None;
        self.line_hints = None;
        match pointer {
            Pointer::Press(at, left) => self.press(at, left),
            Pointer::Drag(to) => self.drag(to),
            Pointer::Release(at) => self.release(at),
        }
    }

    /// Closes the lists and panels shown over the pages; whether any was.
    fn close_panels(&mut self) -> bool {
        let open = self.picker.is_some()
            || self.tab_list.is_some()
            || self.match_list.is_some()
            || self.help.is_some()
            || self.info.is_some()
            || self.overview.is_some();
        self.picker = None;
        self.tab_list = None;
        self.match_list = None;
        self.help = None;
        self.info = None;
        self.overview = None;
        open
    }

    fn press(&mut self, at: Point, left: bool) -> Task<Message> {
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return Task::none();
        };
        let target = document
            .reader
            .spot_on_page(at.x, at.y)
            .map(|(page, across, down)| (page, (across, down)));
        let double = left
            && target.is_some()
            && self.mouse.last_click.is_some_and(|(when, where_)| {
                when.elapsed() < DOUBLE_CLICK && where_.distance(at) < DRAG
            });
        self.mouse.press = Some(Press {
            at,
            last: at,
            left,
            target,
            dragged: false,
            double,
        });
        let Some((page, spot)) = target else {
            return Task::none();
        };
        if double {
            self.mouse.last_click = None;
            self.select_word(page, spot);
        }
        // The text and links of the page, to select and follow.
        Task::batch([self.read_text(vec![page]), self.find_links(page)])
    }

    fn drag(&mut self, to: Point) -> Task<Message> {
        let Some(press) = &mut self.mouse.press else {
            return Task::none();
        };
        if press.at.distance(to) >= DRAG {
            press.dragged = true;
        }
        let press = *press;
        if let Some(mouse) = &mut self.mouse.press {
            mouse.last = to;
        }
        if !press.dragged {
            return Task::none();
        }
        let panning = !press.left || press.target.is_none();
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return Task::none();
        };
        if panning {
            document
                .reader
                .scroll_by(press.last.x - to.x, press.last.y - to.y);
            return Task::none();
        }
        let Some((page, spot)) = press.target else {
            return Task::none();
        };
        // Off the pages, the selection keeps the end it had.
        let Some((to_page, across, down)) = document.reader.spot_on_page(to.x, to.y) else {
            return Task::none();
        };
        let reading = self.read_text(vec![to_page]);
        let Some(document) = self.document() else {
            return reading;
        };
        let text = &document.text;
        let (Some(anchor), Some(cursor)) = (
            visual::spot_at(text, page, spot),
            visual::spot_at(text, to_page, (across, down)),
        ) else {
            return reading;
        };
        let mut selection = Visual::new(Kind::Characters, anchor);
        selection.move_to(cursor);
        self.visual = Some(selection);
        reading
    }

    fn release(&mut self, at: Point) -> Task<Message> {
        let Some(press) = self.mouse.press.take() else {
            return Task::none();
        };
        if press.dragged || !press.left || press.double {
            return Task::none();
        }
        self.mouse.last_click = Some((Instant::now(), at));
        let Some((page, spot)) = press.target else {
            return Task::none();
        };
        self.click_page(page, spot)
    }

    /// Follows the link clicked, once the page's links are known; a click
    /// elsewhere ends the selection.
    fn click_page(&mut self, page: u32, spot: (f32, f32)) -> Task<Message> {
        self.leave_visual();
        let Some(document) = self.document() else {
            return Task::none();
        };
        let Some(links) = document.links.get(&page) else {
            self.mouse.waiting = Some((document.id, page, spot));
            return Task::none();
        };
        let link = link_at(links, spot);
        self.follow_click(link)
    }

    /// Follows a link clicked, or, a click missing every link, ends the
    /// selection.
    fn follow_click(&mut self, link: Option<Link>) -> Task<Message> {
        self.leave_visual();
        let Some(link) = link else {
            return Task::none();
        };
        // What is under the pointer now is somewhere else: a second click
        // is a click of its own, not the end of a double click.
        self.mouse.last_click = None;
        self.link_goal = LinkGoal::Follow;
        self.follow_link(link)
    }

    /// Follows the click that waited for the links of its page, now found.
    pub(super) fn links_arrived(&mut self, id: u64) -> Task<Message> {
        let Some((waiting, page, spot)) = self.mouse.waiting else {
            return Task::none();
        };
        if waiting != id {
            return Task::none();
        }
        let Some(document) = self.document().filter(|document| document.id == id) else {
            self.mouse.waiting = None;
            return Task::none();
        };
        let Some(links) = document.links.get(&page) else {
            return Task::none();
        };
        let link = link_at(links, spot);
        self.mouse.waiting = None;
        self.follow_click(link)
    }

    /// Asks for the links of a page, unless they are known or asked for.
    fn find_links(&mut self, page: u32) -> Task<Message> {
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return Task::none();
        };
        if document.links.contains_key(&page) || !document.asked_links.insert(page) {
            return Task::none();
        }
        let id = document.id;
        let finding = document.backend.links(vec![page]);
        Task::perform(finding, move |result| Message::LinksFound(id, result))
    }

    /// Selects the word at a spot of a page, as a double click does.
    fn select_word(&mut self, page: u32, spot: (f32, f32)) {
        let Some(document) = self.document() else {
            return;
        };
        let Some(at) = visual::spot_at(&document.text, page, spot) else {
            self.notice = Notice::Info("reading the page's text: click again".to_owned());
            return;
        };
        let (first, last) = visual::word_at(&document.text, at);
        let mut selection = Visual::new(Kind::Characters, first);
        selection.move_to(last);
        self.visual = Some(selection);
    }

    /// Picks what was clicked in a list, the tab bar or the overview, as
    /// the key that picks the entry selected does.
    pub(super) fn on_click(&mut self, click: Click) -> Task<Message> {
        match click {
            Click::Tab(index) => {
                self.go_to_tab(index);
                Task::none()
            }
            Click::Recent(index) => {
                let chosen = self.picker.as_mut().and_then(|list| list.choose(index));
                let Some(path) = chosen else {
                    return Task::none();
                };
                self.picker = None;
                self.open_in_tab(path)
            }
            Click::TabList(index) => {
                let chosen = self.tab_list.as_mut().and_then(|list| list.choose(index));
                if let Some(tab) = chosen {
                    self.tab_list = None;
                    self.go_to_tab(tab);
                }
                Task::none()
            }
            Click::Outline(row) => {
                let rows = self.sidebar_rows();
                let Some(Sidebar::Outline(sidebar)) = &mut self.sidebar else {
                    return Task::none();
                };
                let outcome = sidebar.click(row, rows);
                self.on_outline_outcome(outcome);
                Task::none()
            }
            Click::Marking(index) => self.on_marking_list(markings::Outcome::Go(index)),
            Click::Overview(page) => self.go_from_overview(page),
            Click::Match(index) => {
                let chosen = self
                    .match_list
                    .as_mut()
                    .and_then(|(_, list)| list.choose(index));
                if let Some(hit) = chosen {
                    self.go_to_match(hit);
                }
                Task::none()
            }
        }
    }
}

/// The link over a spot of a page, the last drawn if several are.
fn link_at(links: &[Link], (across, down): (f32, f32)) -> Option<Link> {
    links
        .iter()
        .rev()
        .find(|link| {
            (link.area.left..=link.area.right).contains(&across)
                && (link.area.top..=link.area.bottom).contains(&down)
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::app::testing::{on_screen, read, selected, showing};

    #[test]
    fn dragging_over_text_selects_it() {
        let (mut app, id) = showing("text.pdf");
        // "Hello, world. A line of text" is set at 72 points across, its
        // baseline 72 points down; the second line 24 points under it.
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Press(
            on_screen(&app, 0, (74.0, 66.0)),
            true,
        )));
        read(&mut app, id, 0);
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Drag(on_screen(
            &app,
            0,
            (120.0, 90.0),
        ))));
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Release(on_screen(
            &app,
            0,
            (120.0, 90.0),
        ))));
        let text = selected(&app);
        assert!(text.starts_with("Hello, world."), "{text:?}");
        // Copied as running text, the second line goes on after a space.
        assert!(text.contains("text sec"), "{text:?}");
    }

    #[test]
    fn a_double_click_selects_a_word_and_a_click_ends_it() {
        let (mut app, id) = showing("text.pdf");
        read(&mut app, id, 0);
        // On the "o" of "world".
        let at = on_screen(&app, 0, (130.0, 66.0));
        for _ in 0..2 {
            let _: Task<Message> = app.update(Message::Pointer(Pointer::Press(at, true)));
            let _: Task<Message> = app.update(Message::Pointer(Pointer::Release(at)));
        }
        assert_eq!(selected(&app), "world");
        // Long after, a click is a click again, and leaves the selection.
        app.mouse.last_click = None;
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Press(at, true)));
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Release(at)));
        assert!(app.visual.is_none());
    }

    #[test]
    fn clicking_a_link_follows_it_once_the_links_are_found() {
        let (mut app, id) = showing("links.pdf");
        assert_eq!(app.document().unwrap().reader.current_page(), 0);
        // The link to the second page covers 72 to 200 across, 72 to 92
        // down.
        let at = on_screen(&app, 0, (100.0, 80.0));
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Press(at, true)));
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Release(at)));
        let links = block_on(app.document().unwrap().backend.links(vec![0]));
        let _: Task<Message> = app.update(Message::LinksFound(id, links));
        assert_eq!(app.document().unwrap().reader.current_page(), 1);
        // Clicked again with the links known, it is followed at once.
        app.document_mut().unwrap().reader.go_to_page(0);
        let at = on_screen(&app, 0, (100.0, 80.0));
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Press(at, true)));
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Release(at)));
        assert_eq!(app.document().unwrap().reader.current_page(), 1);
    }

    #[test]
    fn the_middle_button_drags_the_view() {
        let (mut app, _) = showing("links.pdf");
        let before = app.document().unwrap().reader.view(false).y;
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Press(
            Point::new(300.0, 400.0),
            false,
        )));
        let far = Point::new(300.0, 300.0);
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Drag(far)));
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Release(far)));
        let after = app.document().unwrap().reader.view(false).y;
        let zoom = app.document().unwrap().reader.zoom();
        assert!(
            (after - before - 100.0 / zoom).abs() < 0.5,
            "{before} -> {after}"
        );
        assert!(app.visual.is_none());
    }

    #[test]
    fn a_click_in_a_list_picks_the_entry() {
        let (mut app, _) = showing("text.pdf");
        let _: Task<Message> = app.update(Message::Clicked(Click::Tab(0)));
        assert_eq!(app.current, 0);
        app.show_tab_list();
        assert!(app.tab_list.is_some());
        let _: Task<Message> = app.update(Message::Clicked(Click::TabList(0)));
        assert!(app.tab_list.is_none());
    }
}
