//! Labels on the links in view: `f` follows one, `F` opens an internal
//! one in a new tab, and `;y` copies one's address. Typing `/` and some of
//! a link's text narrows the labels to the links that hold it.

use std::path::PathBuf;

use iced::Task;

use super::{App, Labelled, Message, Notice, document_by_id, open_externally, tilde};
use crate::backend::{self, Link, Target, TextLine};
use crate::hints::{Hints, Outcome};
use crate::input::LinkGoal;
use crate::keymap::Pending;
use crate::layout::Rect;
use crate::reader::Place;
use crate::visual::Text;

impl App {
    /// Labels the links in view, to do this with the one picked. The links
    /// on pages not seen before are found first, which takes a moment; the
    /// text of the pages is read meanwhile, to narrow the labels by.
    pub(super) fn label_links(&mut self, goal: LinkGoal) -> Task<Message> {
        self.pending = Pending::new();
        self.link_goal = goal;
        let Some(document) = self.document() else {
            return Task::none();
        };
        let id = document.id;
        let visible: Vec<u32> = document.reader.visible_pages().collect();
        let missing: Vec<u32> = visible
            .iter()
            .copied()
            .filter(|page| !document.links.contains_key(page))
            .collect();
        let finding = (!missing.is_empty()).then(|| document.backend.links(missing));
        let reading = self.read_text(visible);
        let Some(finding) = finding else {
            self.show_hints();
            return reading;
        };
        self.hints_wanted = Some(id);
        Task::batch([
            Task::perform(finding, move |result| Message::LinksFound(id, result)),
            reading,
        ])
    }

    pub(super) fn on_links(&mut self, id: u64, result: Result<backend::PageLinks, backend::Error>) {
        let wanted = self.hints_wanted.take() == Some(id);
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return;
        };
        match result {
            Ok(pages) => document.links.extend(pages),
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                return;
            }
        }
        // Only if the document is still the one in view.
        if wanted && self.document().is_some_and(|document| document.id == id) {
            self.show_hints();
        }
    }

    /// Labels the links that show in the window, in reading order, and
    /// for `f` the fields and markings too.
    fn show_hints(&mut self) {
        let Some(document) = self.document() else {
            return;
        };
        let following = self.link_goal == LinkGoal::Follow;
        let (width, height) = (self.area.width, self.area.height);
        let reader = &document.reader;
        let mut links: Vec<(u32, Rect, Labelled)> = reader
            .visible_pages()
            .flat_map(|page| {
                document
                    .links
                    .get(&page)
                    .into_iter()
                    .flatten()
                    .map(move |link| (page, link))
            })
            .filter(|(_, link)| following || !matches!(link.target, Target::Field(_)))
            .filter(|(page, link)| {
                reader.area_on_screen(*page, link.area).is_some_and(|area| {
                    area.right > 0.0 && area.left < width && area.bottom > 0.0 && area.top < height
                })
            })
            .map(|(page, link)| (page, link.area, Labelled::Link(link.clone())))
            .collect();
        if following {
            links.extend(self.markings_in_view().into_iter().filter_map(|marking| {
                let area = *marking.areas.first()?;
                Some((marking.page, area, Labelled::Marking(marking)))
            }));
        }
        links.sort_by(|(page, area, _), (other_page, other, _)| {
            page.cmp(other_page)
                .then(area.top.total_cmp(&other.top))
                .then(area.left.total_cmp(&other.left))
        });
        self.hints = Hints::new(
            links
                .into_iter()
                .map(|(page, _, item)| (page, item))
                .collect(),
            &self.hint_chars,
        );
        self.label_texts();
        if self.hints.is_none() {
            self.notice = Notice::Info(if following {
                "no links, fields or markings in view".to_owned()
            } else {
                "no links in view".to_owned()
            });
        }
    }

    /// Gives each label the text it is narrowed by: the words of its link
    /// and where the link goes, a field's name and value, or the text a
    /// marking covers. Done again as the text of more pages is read.
    pub(super) fn label_texts(&mut self) {
        // The tabs are borrowed apart from the labels.
        let Some(document) = self
            .tabs
            .get(self.current)
            .and_then(|tab| tab.document.as_ref())
        else {
            return;
        };
        let text = &document.text;
        if let Some(hints) = &mut self.hints {
            hints.set_texts(|page, item| match item {
                Labelled::Link(link) => link_text(text, page, link),
                Labelled::Marking(marking) => marking.text.clone(),
            });
        }
    }

    /// Acts on a key typed while the links are labelled.
    pub(super) fn on_hint(&mut self, outcome: Outcome<Labelled>) -> Task<Message> {
        let link = match outcome {
            Outcome::Wait => return Task::none(),
            Outcome::Cancel => {
                self.hints = None;
                return Task::none();
            }
            Outcome::NoMatch => {
                self.hints = None;
                self.notice = Notice::Error("no such label".to_owned());
                return Task::none();
            }
            Outcome::Picked(hint) => {
                self.hints = None;
                match hint.item {
                    Labelled::Link(link) => link,
                    Labelled::Marking(marking) => {
                        self.pick_marking(&marking);
                        return Task::none();
                    }
                }
            }
        };
        match (self.link_goal, link.target) {
            (LinkGoal::Copy, target) => self.copy_link(&target),
            (LinkGoal::NewTab, Target::Page { page, top }) => self.open_beside(page, top),
            (_, Target::Page { page, top }) => {
                if let Some(document) = self.document_mut() {
                    document.jumps.record(document.reader.place());
                    document.reader.go_to_spot(page, top);
                }
                Task::none()
            }
            (_, Target::Uri(uri)) => {
                self.notice = match open_externally(&uri) {
                    Ok(()) => Notice::Info(format!("opening {uri}")),
                    Err(error) => Notice::Error(format!("cannot open {uri}: {error}")),
                };
                Task::none()
            }
            (_, Target::Field(field)) => self.fill_field(field, link.area),
        }
    }

    /// Copies where a link goes: its address, or for a place in the
    /// document, the file with the page after `#page=`, as browsers open
    /// it.
    fn copy_link(&mut self, target: &Target) -> Task<Message> {
        let address = match target {
            Target::Uri(uri) => uri.clone(),
            Target::Page { page, .. } => {
                let Some(document) = self.document() else {
                    return Task::none();
                };
                format!(
                    "{}#page={}",
                    document.path.to_string_lossy(),
                    page.saturating_add(1)
                )
            }
            Target::Field(_) => return Task::none(),
        };
        self.notice = Notice::Info(format!("copied {address}"));
        iced::clipboard::write(address)
    }

    /// Opens the document in view again in a new tab beside it, at a spot
    /// on a page, as a browser opens a link in a new tab. Each tab then
    /// has the document to itself: markings made in one do not show in
    /// the other until saved and opened again.
    fn open_beside(&mut self, page: u32, top: Option<f32>) -> Task<Message> {
        let Some(document) = self.document() else {
            return Task::none();
        };
        let path: PathBuf = document.path.clone();
        let height = document
            .reader
            .page_size(page)
            .map_or(0.0, |size| size.height);
        let down = top
            .filter(|_| height > 0.0)
            .map_or(0.0, |top| top / height);
        let beside = self.current.saturating_add(1);
        let task = self.open_new_tab(path.clone(), beside, Place { page, down });
        self.notice = Notice::Info(format!(
            "opening {} at page {}",
            tilde(&path),
            page.saturating_add(1)
        ));
        task
    }
}

/// The words of a link, as they lie on its page, and where it goes.
fn link_text(text: &Text, page: u32, link: &Link) -> String {
    let words = text
        .get(&page)
        .map(|lines| text_within(lines, link.area))
        .unwrap_or_default();
    match &link.target {
        Target::Uri(uri) => format!("{words} {uri}"),
        Target::Page { page, .. } => format!("{words} page {}", page.saturating_add(1)),
        Target::Field(field) => format!("{words} {} {}", field.name, field.value),
    }
}

/// The characters whose middles lie in an area, a line at a time, the
/// lines separated by spaces.
fn text_within(lines: &[TextLine], area: Rect) -> String {
    let lines: Vec<String> = lines
        .iter()
        .map(|line| {
            line.chars
                .iter()
                .filter(|glyph| {
                    let middle_x = f32::midpoint(glyph.area.left, glyph.area.right);
                    let middle_y = f32::midpoint(glyph.area.top, glyph.area.bottom);
                    (area.left..=area.right).contains(&middle_x)
                        && (area.top..=area.bottom).contains(&middle_y)
                })
                .map(|glyph| glyph.character)
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .filter(|line| !line.is_empty())
        .collect();
    lines.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TextChar;

    fn line(text: &str, top: f32) -> TextLine {
        TextLine {
            chars: text
                .chars()
                .zip(0_u8..)
                .map(|(character, index)| TextChar {
                    character,
                    area: Rect {
                        left: f32::from(index) * 10.0,
                        top,
                        right: f32::from(index).mul_add(10.0, 10.0),
                        bottom: top + 10.0,
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn a_link_holds_the_characters_inside_it() {
        let lines = [line("see the intro", 0.0), line("and then more", 10.0)];
        let area = Rect {
            left: 76.0,
            top: 0.0,
            right: 130.0,
            bottom: 20.0,
        };
        assert_eq!(text_within(&lines, area), "intro more");
        let nowhere = Rect {
            left: 500.0,
            top: 0.0,
            right: 600.0,
            bottom: 10.0,
        };
        assert_eq!(text_within(&lines, nowhere), "");
    }
}
