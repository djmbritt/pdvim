//! Labels on the links in view: `f` follows one, `F` opens an internal
//! one in a new tab, and `;y` copies one's address. Typing `/` and some of
//! a link's text narrows the labels to the links that hold it. `yi`
//! labels the pictures and figures in view instead, to copy one as a
//! picture, and `I` in visual mode does too, to select one.

use std::path::PathBuf;

use iced::Task;

use super::{
    App, Labelled, Message, Notice, TextFor, Wanted, document_by_id, open_externally, tilde,
};
use crate::backend::{self, Link, Target, TextLine};
use crate::hints::{Hints, Outcome};
use crate::input::LinkGoal;
use crate::keymap::Pending;
use crate::layout::Rect;
use crate::reader::Place;
use crate::visual::{self, Text, Visual};

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
        if matches!(goal, LinkGoal::Picture | LinkGoal::SelectFigure) {
            let missing: Vec<u32> = visible
                .iter()
                .copied()
                .filter(|page| !document.figures.contains_key(page))
                .collect();
            let finding = (!missing.is_empty()).then(|| document.backend.figures(missing));
            let reading = self.read_text(visible);
            let Some(finding) = finding else {
                self.show_hints();
                return reading;
            };
            self.hints_wanted = Some(id);
            return Task::batch([
                Task::perform(finding, move |result| Message::FiguresFound(id, result)),
                reading,
            ]);
        }
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

    pub(super) fn on_figures(
        &mut self,
        id: u64,
        result: Result<backend::PageFigures, backend::Error>,
    ) -> Task<Message> {
        let wanted = self.hints_wanted.take() == Some(id);
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return Task::none();
        };
        let found = match result {
            Ok(pages) => {
                document.figures.extend(pages);
                true
            }
            Err(error) => {
                // None for the pages in view, so that what waits for them,
                // such as the labels on lines, goes on without.
                let visible: Vec<u32> = document.reader.visible_pages().collect();
                for page in visible {
                    document.figures.entry(page).or_default();
                }
                self.notice = Notice::Error(error.to_string());
                false
            }
        };
        let in_view = self.document().is_some_and(|document| document.id == id);
        if wanted && found && in_view {
            self.show_hints();
        }
        // The lines wait for the figures, to be labelled with them.
        if let Some((wanted, TextFor::Lines(kind))) = self.lines_wanted
            && wanted == id
            && in_view
            && self.lines_ready()
        {
            self.lines_wanted = None;
            self.show_line_hints(kind);
        }
        Task::none()
    }

    /// Says whether a picture reached the clipboard.
    pub(super) fn on_picture_copied(&mut self, result: Result<(), String>) -> Task<Message> {
        self.notice = match result {
            Ok(()) => Notice::Info("copied the picture".to_owned()),
            Err(error) => Notice::Error(format!("cannot copy the picture: {error}")),
        };
        Task::none()
    }

    /// Labels the links that show in the window, in reading order, and
    /// for `f` the fields and markings too; or for `yi` the pictures and
    /// figures.
    fn show_hints(&mut self) {
        let Some(document) = self.document() else {
            return;
        };
        let following = self.link_goal == LinkGoal::Follow;
        let pictures = matches!(self.link_goal, LinkGoal::Picture | LinkGoal::SelectFigure);
        let (width, height) = (self.area.width, self.area.height);
        let reader = &document.reader;
        let on_screen = |page: u32, area: Rect| {
            reader.area_on_screen(page, area).is_some_and(|area| {
                area.right > 0.0 && area.left < width && area.bottom > 0.0 && area.top < height
            })
        };
        let figures = reader.visible_pages().flat_map(|page| {
            document
                .figures
                .get(&page)
                .into_iter()
                .flatten()
                .map(move |area| (page, *area, Labelled::Picture(*area)))
        });
        let mut links: Vec<(u32, Rect, Labelled)> = if pictures {
            figures
                .filter(|(page, area, _)| on_screen(*page, *area))
                .collect()
        } else {
            Vec::new()
        };
        links.extend(reader
            .visible_pages()
            .flat_map(|page| {
                document
                    .links
                    .get(&page)
                    .into_iter()
                    .flatten()
                    .map(move |link| (page, link))
            })
            .filter(|_| !pictures)
            .filter(|(_, link)| following || !matches!(link.target, Target::Field(_)))
            .filter(|(page, link)| on_screen(*page, link.area))
            .map(|(page, link)| (page, link.area, Labelled::Link(link.clone()))));
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
            self.notice = Notice::Info(
                match self.link_goal {
                    LinkGoal::Follow => "no links, fields or markings in view",
                    LinkGoal::Picture | LinkGoal::SelectFigure => {
                        "no pictures or figures in view"
                    }
                    LinkGoal::NewTab | LinkGoal::Copy => "no links in view",
                }
                .to_owned(),
            );
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
                Labelled::Picture(area) => text
                    .get(&page)
                    .map(|lines| text_within(lines, *area))
                    .unwrap_or_default(),
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
            Outcome::Figures => {
                self.hints = None;
                return self.label_links(LinkGoal::SelectFigure);
            }
            Outcome::Picked(hint) => {
                self.hints = None;
                match hint.item {
                    Labelled::Link(link) => link,
                    Labelled::Marking(marking) => {
                        self.pick_marking(&marking);
                        return Task::none();
                    }
                    Labelled::Picture(area) if self.link_goal == LinkGoal::SelectFigure => {
                        return self.select_figure(hint.page, area);
                    }
                    Labelled::Picture(area) => return self.copy_picture(hint.page, area),
                }
            }
        };
        self.follow_link(link)
    }

    /// Does with a link what the labels were shown for: following it, as a
    /// click on it does too, opening it in a new tab, or copying it.
    pub(super) fn follow_link(&mut self, link: Link) -> Task<Message> {
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

    /// Selects a picture or figure in visual mode, starting it if need
    /// be with the cursor on the line of text nearest the figure, for a
    /// motion to go on from.
    pub(super) fn select_figure(&mut self, page: u32, area: Rect) -> Task<Message> {
        if let Some(visual) = &mut self.visual {
            visual.select_figure(page, area);
            return Task::none();
        }
        let Some(document) = self.document() else {
            return Task::none();
        };
        let mut visual = Visual::cursor_at(visual::spot_near(&document.text, page, area));
        visual.select_figure(page, area);
        self.visual = Some(visual);
        // Known, so that the bottom line can show the note at the cursor.
        self.with_markings(Wanted::Refresh)
    }

    /// Copies a picture or figure to the clipboard, as it is printed, at
    /// least twice as sharp as a point to a pixel, and turned as it is
    /// shown.
    pub(super) fn copy_picture(&mut self, page: u32, area: Rect) -> Task<Message> {
        let scale_factor = self.scale_factor;
        let Some(document) = self.document() else {
            return Task::none();
        };
        // A little room around it, as shapes are measured to their middles.
        let room = 2.0;
        let clip = Rect {
            left: (area.left - room).max(0.0),
            top: (area.top - room).max(0.0),
            right: area.right + room,
            bottom: area.bottom + room,
        };
        let rendering = document.backend.render(backend::RenderJob {
            page,
            scale: (document.reader.zoom() * scale_factor).max(2.0),
            rotation: document.reader.rotation(),
            clip: Some(clip),
            palette: None,
        });
        self.notice = Notice::Info("copying the picture".to_owned());
        Task::perform(
            async move {
                let picture = rendering.await.map_err(|error| error.to_string())?;
                crate::clipboard::copy_picture(picture.width, picture.height, picture.pixels)
                    .await
            },
            Message::PictureCopied,
        )
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
pub(super) fn text_within(lines: &[TextLine], area: Rect) -> String {
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
    use iced::futures::executor::block_on;

    use super::*;
    use crate::app::mode::Mode;
    use crate::app::testing::{read, showing, typing};
    use crate::backend::TextChar;
    use crate::visual::Spot;

    /// Labels the figures, as `I` does, once the backend has found them,
    /// and types the first label.
    fn select_first_figure(app: &mut App, id: u64) {
        typing(app, "I");
        pick_first_figure(app, id);
    }

    /// Gives the app the figures of the first page, which the labels wait
    /// for, and types the first label.
    fn pick_first_figure(app: &mut App, id: u64) {
        let figures = block_on(app.document().unwrap().backend.figures(vec![0]));
        let _: Task<Message> = app.update(Message::FiguresFound(id, figures));
        let hints = app.hints.as_ref().unwrap();
        let label: String = hints.shown().next().unwrap().label.iter().collect();
        typing(app, &label);
    }

    #[test]
    fn a_figure_is_selected_from_visual_mode_by_its_label() {
        let (mut app, id) = showing("drawn.pdf");
        read(&mut app, id, 0);
        app.visual = Some(Visual::cursor_at(Spot::line_start(0, 0)));
        select_first_figure(&mut app, id);
        assert!(app.hints.is_none());
        let (page, area) = app.visual.as_ref().unwrap().figure().unwrap();
        assert_eq!(page, 0);
        assert!(area.right > area.left && area.bottom > area.top);
        assert_eq!(app.mode(), Mode::Figure);
        // A motion goes back to the text, from the cursor.
        typing(&mut app, "l");
        assert!(app.visual.as_ref().unwrap().figure().is_none());
        assert_ne!(app.mode(), Mode::Figure);
    }

    #[test]
    fn a_figure_is_selected_from_normal_mode_and_visual_mode_starts() {
        let (mut app, id) = showing("drawn.pdf");
        read(&mut app, id, 0);
        assert!(app.visual.is_none());
        select_first_figure(&mut app, id);
        assert_eq!(app.mode(), Mode::Figure);
        let visual = app.visual.as_ref().unwrap();
        assert!(visual.figure().is_some());
        // The cursor is on the page's text, for a motion to go on from.
        assert_eq!(visual.cursor(), Spot::line_start(0, 0));
        typing(&mut app, "l");
        assert_eq!(app.mode(), Mode::Cursor);
    }

    /// Gives the app the figures of the first page, as the backend
    /// answers.
    fn found_figures(app: &mut App, id: u64) {
        let figures = block_on(app.document().unwrap().backend.figures(vec![0]));
        let _: Task<Message> = app.update(Message::FiguresFound(id, figures));
    }

    #[test]
    fn v_labels_the_figures_with_the_lines_once_both_are_known() {
        let (mut app, id) = showing("drawn.pdf");
        read(&mut app, id, 0);
        typing(&mut app, "v");
        // The text is read; the figures are still being found.
        assert!(app.line_hints.is_none());
        found_figures(&mut app, id);
        let (_, hints) = app.line_hints.as_ref().unwrap();
        let figure = hints
            .shown()
            .find(|hint| matches!(hint.item, crate::app::Start::Figure(_)))
            .unwrap();
        assert!(hints.shown().any(|hint| matches!(hint.item, crate::app::Start::Text(_))));
        let label: String = figure.label.iter().collect();
        typing(&mut app, &label);
        assert!(app.line_hints.is_none());
        assert_eq!(app.mode(), Mode::Figure);
    }

    #[test]
    fn the_labels_on_lines_give_way_to_the_figures() {
        let (mut app, id) = showing("drawn.pdf");
        read(&mut app, id, 0);
        typing(&mut app, "v");
        found_figures(&mut app, id);
        assert!(app.line_hints.is_some());
        typing(&mut app, "I");
        assert!(app.line_hints.is_none());
        pick_first_figure(&mut app, id);
        assert_eq!(app.mode(), Mode::Figure);
    }

    #[test]
    fn a_figure_selected_is_copied_as_a_picture_and_leaves() {
        let (mut app, id) = showing("drawn.pdf");
        read(&mut app, id, 0);
        app.visual = Some(Visual::cursor_at(Spot::line_start(0, 0)));
        select_first_figure(&mut app, id);
        typing(&mut app, "y");
        assert!(app.visual.is_none());
        assert!(matches!(&app.notice, Notice::Info(said) if said == "copying the picture"));
    }

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
