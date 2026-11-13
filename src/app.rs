//! The window: what is drawn, and what happens on each key press.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use iced::widget::canvas::{self, Canvas};
use iced::widget::{column, container, image, row, stack, text};
use iced::{
    Color, Element, Event, Fill, Font, Point, Rectangle, Renderer, Size, Subscription, Task, Theme,
    keyboard, mouse, window,
};

use crate::backend::{self, Backend, Hit, Opened, RenderJob};
use crate::command::{self, Command, Setting};
use crate::history::{Fit, History, View};
use crate::input::{Action, Key, Pending, Prompt};
use crate::layout::Rect;
use crate::num;
use crate::reader::Reader;
use crate::recolor::{Palette, Rgb};

/// The colour behind the pages.
const BACKDROP: Color = Color::from_rgb8(0x10, 0x10, 0x10);
/// The colour of the status bar.
const BAR: Color = Color::from_rgb8(0x20, 0x20, 0x20);
/// The colour of text in the status bar.
const BAR_TEXT: Color = Color::from_rgb8(0xd0, 0xd0, 0xd0);
/// The colour of an error in the status bar.
const BAR_ERROR: Color = Color::from_rgb8(0xff, 0x6b, 0x6b);
/// The colour laid over every search result.
const HIT: Color = Color::from_rgba8(0xff, 0xd0, 0x00, 0.35);
/// The colour laid over the search result last jumped to.
const HIT_CURRENT: Color = Color::from_rgba8(0xff, 0x70, 0x00, 0.5);

/// The size of status bar text, in pixels.
const BAR_TEXT_SIZE: f32 = 13.0;

/// How far one notch of a mouse wheel scrolls, in scroll steps.
const WHEEL_STEPS: f32 = 3.0;

/// How many pages beyond the visible ones are rendered ahead of need.
const RENDER_AHEAD: u32 = 1;
/// How many pages beyond the visible ones are kept once rendered.
const KEEP_AROUND: u32 = 2;

/// Something that happened, for [`App::update`] to act on.
#[derive(Debug, Clone)]
pub enum Message {
    Keyboard(keyboard::Event),
    /// The area for pages changed size.
    Resized(Size),
    /// The display's pixels per logical pixel became known.
    Scaled(f32),
    /// The mouse wheel turned over the pages.
    Wheel(mouse::ScrollDelta),
    /// The window was asked to close.
    CloseRequested,
    /// A document finished opening, or failed to.
    Opened(u64, PathBuf, Result<Opened, backend::Error>),
    /// A page finished rendering, or failed to.
    Rendered(u64, u32, RenderKey, Result<image::Handle, backend::Error>),
    /// A search finished, or failed.
    Found(u64, Search, Result<Vec<Hit>, backend::Error>),
    /// The margins of every page were measured, or could not be.
    Measured(u64, Result<Vec<Rect>, backend::Error>),
}

/// What a rendered page was rendered for. A cached page whose key differs
/// from the one wanted now is out of date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderKey {
    /// The bits of the scale, in pixels per point.
    scale: u32,
    rotation: crate::layout::Rotation,
    palette: Option<Palette>,
    /// Whether only the part of the page inside its margins was rendered.
    trimmed: bool,
}

/// A search that was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    needle: String,
    forward: bool,
}

/// The results of the last search.
#[derive(Debug, Default)]
struct Results {
    /// The search that produced them, kept for `n` and `N`.
    search: Option<Search>,
    hits: Vec<Hit>,
    /// The hit last jumped to.
    current: Option<usize>,
    /// Whether the hits are highlighted.
    shown: bool,
}

/// An open document.
struct Document {
    path: PathBuf,
    backend: Backend,
    reader: Reader,
    recolor: bool,
    /// Whether the margins should be trimmed. The reader only trims them
    /// once they have been measured.
    trim: bool,
    /// The content of every page, once measured.
    contents: Option<Vec<Rect>>,
    /// Whether the margins are being measured right now.
    measuring: bool,
    /// The view to go back to once the margins are measured, for a document
    /// that was left with its margins trimmed.
    returning: Option<View>,
    /// The rendered pages, and what each was rendered for.
    pages: HashMap<u32, (image::Handle, RenderKey)>,
    /// What each page being rendered right now is rendered for.
    rendering: HashMap<u32, RenderKey>,
    /// What each page that failed to render was rendered for, so the same
    /// render is not tried again and again.
    failed: HashMap<u32, RenderKey>,
    results: Results,
}

/// What the bottom line says when nothing is being typed.
enum Notice {
    None,
    Info(String),
    Error(String),
}

/// The state of the whole program.
pub struct App {
    document: Option<Document>,
    /// Counts opened documents, so answers meant for an earlier one can be
    /// told apart and dropped.
    generation: u64,
    history: Option<History>,
    /// The size of the area for pages, in logical pixels.
    area: Size,
    /// Physical pixels per logical pixel.
    scale_factor: f32,
    modifiers: keyboard::Modifiers,
    pending: Pending,
    /// The prompt being typed into, and the text so far.
    prompt: Option<(Prompt, String)>,
    notice: Notice,
    palette: Palette,
    /// How far one press of `j` scrolls, in logical pixels.
    scroll_step: f32,
}

/// Runs the reader, opening a document first if one is given.
pub fn run(path: Option<PathBuf>) -> iced::Result {
    iced::application(move || App::boot(path.clone()), App::update, App::view)
        .title(App::title)
        .subscription(|_: &App| App::subscription())
        .theme(|_: &App| Some(Theme::Dark))
        .exit_on_close_request(false)
        .window(window::Settings {
            platform_specific: window::settings::PlatformSpecific {
                application_id: "pdvim".to_owned(),
                ..window::settings::PlatformSpecific::default()
            },
            ..window::Settings::default()
        })
        .run()
}

impl App {
    fn boot(path: Option<PathBuf>) -> (Self, Task<Message>) {
        let (history, notice) = match History::open() {
            Ok(history) => (Some(history), Notice::None),
            Err(error) => (None, Notice::Error(format!("history is off: {error}"))),
        };
        let mut app = Self {
            document: None,
            generation: 0,
            history,
            area: Size::ZERO,
            scale_factor: 1.0,
            modifiers: keyboard::Modifiers::default(),
            pending: Pending::default(),
            prompt: None,
            notice,
            palette: Palette::default(),
            scroll_step: 40.0,
        };
        let task = path.map_or_else(Task::none, |path| app.open(path));
        (app, task)
    }

    fn title(&self) -> String {
        self.document
            .as_ref()
            .and_then(|document| document.path.file_name())
            .map_or_else(
                || "pdvim".to_owned(),
                |name| format!("{} - pdvim", name.to_string_lossy()),
            )
    }

    fn subscription() -> Subscription<Message> {
        Subscription::batch([
            keyboard::listen().map(Message::Keyboard),
            window::close_requests().map(|_| Message::CloseRequested),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message {
            Message::Keyboard(event) => self.on_keyboard(event),
            Message::Resized(area) => {
                self.area = area;
                if let Some(document) = &mut self.document {
                    document.reader.set_window((area.width, area.height));
                }
                // The window may have moved to a display with another scale.
                window::latest()
                    .and_then(window::scale_factor)
                    .map(Message::Scaled)
            }
            Message::Scaled(scale_factor) => {
                if scale_factor.is_finite() && scale_factor > 0.0 {
                    self.scale_factor = scale_factor;
                }
                Task::none()
            }
            Message::Wheel(delta) => {
                self.on_wheel(delta);
                Task::none()
            }
            Message::CloseRequested => self.quit(),
            Message::Opened(generation, path, result) => {
                if generation == self.generation {
                    self.on_opened(path, result);
                    let trimmed = self
                        .document
                        .as_ref()
                        .is_some_and(|document| document.returning.is_some());
                    if trimmed {
                        return Task::batch([self.set_trim(true), self.render_missing()]);
                    }
                }
                Task::none()
            }
            Message::Rendered(generation, page, key, result) => {
                if generation == self.generation {
                    self.on_rendered(page, key, result);
                }
                Task::none()
            }
            Message::Measured(generation, result) => {
                if generation == self.generation {
                    self.on_measured(result);
                }
                Task::none()
            }
            Message::Found(generation, search, result) => {
                if generation == self.generation {
                    self.on_found(search, result);
                }
                Task::none()
            }
        };
        Task::batch([task, self.render_missing()])
    }

    fn on_keyboard(&mut self, event: keyboard::Event) -> Task<Message> {
        match event {
            keyboard::Event::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            keyboard::Event::KeyReleased { .. } => Task::none(),
            keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            } => {
                let Some(key) = translate(&key, modifiers, text.as_deref()) else {
                    return Task::none();
                };
                self.notice = Notice::None;
                if self.prompt.is_some() {
                    self.on_prompt_key(key)
                } else {
                    self.pending
                        .feed(key)
                        .map_or_else(Task::none, |action| self.act(action))
                }
            }
        }
    }

    /// Edits the prompt, or runs it once enter is pressed.
    fn on_prompt_key(&mut self, key: Key) -> Task<Message> {
        let Some((prompt, mut line)) = self.prompt.take() else {
            return Task::none();
        };
        match key {
            Key::Enter => return self.submit(prompt, &line),
            Key::Escape | Key::Ctrl('c' | '[') => return Task::none(),
            Key::Backspace | Key::Ctrl('h') => {
                // Rubbing out the sigil itself leaves the prompt, as in vi.
                if line.pop().is_none() {
                    return Task::none();
                }
            }
            Key::Ctrl('u') => line.clear(),
            Key::Ctrl('w') => {
                let kept = line.trim_end().rfind(char::is_whitespace);
                line.truncate(kept.map_or(0, |index| index.saturating_add(1)));
            }
            Key::Char(character) => line.push(character),
            Key::ShiftSpace => line.push(' '),
            _ => {}
        }
        self.prompt = Some((prompt, line));
        Task::none()
    }

    /// Runs what was typed into a prompt.
    fn submit(&mut self, prompt: Prompt, line: &str) -> Task<Message> {
        match prompt {
            Prompt::Command => match command::parse(line) {
                Ok(Some(command)) => self.run_command(command),
                Ok(None) => Task::none(),
                Err(error) => {
                    self.notice = Notice::Error(error.to_string());
                    Task::none()
                }
            },
            Prompt::SearchForward => self.search(line, true),
            Prompt::SearchBackward => self.search(line, false),
        }
    }

    fn run_command(&mut self, command: Command) -> Task<Message> {
        match command {
            Command::Quit => return self.quit(),
            Command::Open(path) => return self.open(path),
            Command::Goto(page) => {
                if let Some(document) = &mut self.document {
                    document.reader.go_to_page(page.saturating_sub(1));
                }
            }
            Command::NoHighlight => {
                if let Some(document) = &mut self.document {
                    document.results.shown = false;
                }
            }
            Command::Set(setting) => match setting {
                Setting::Recolor(recolor) => {
                    if let Some(document) = &mut self.document {
                        document.recolor = recolor;
                    }
                }
                Setting::Trim(trim) => return self.set_trim(trim),
                Setting::RecolorPaper(paper) => self.palette.paper = paper,
                Setting::RecolorInk(ink) => self.palette.ink = ink,
                Setting::ScrollStep(step) => self.scroll_step = step,
            },
        }
        Task::none()
    }

    /// Carries out an action from the keyboard.
    fn act(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Quit => return self.quit(),
            Action::OpenPrompt(prompt) => {
                self.prompt = Some((prompt, String::new()));
                return Task::none();
            }
            Action::SearchNext => return self.step_search(true),
            Action::SearchPrevious => return self.step_search(false),
            Action::ToggleTrim => {
                let trim = self
                    .document
                    .as_ref()
                    .is_some_and(|document| !document.trim);
                return self.set_trim(trim);
            }
            _ => {}
        }
        let step = self.scroll_step;
        let Some(document) = &mut self.document else {
            return Task::none();
        };
        let reader = &mut document.reader;
        match action {
            Action::Scroll { right, down } => {
                reader.scroll_by(steps(right) * step, steps(down) * step);
            }
            Action::HalfScreen(count) => reader.scroll_screens(steps(count) / 2.0),
            Action::FullScreen(count) => reader.scroll_screens(steps(count)),
            Action::Page(count) => reader.page_by(count),
            Action::GotoPage(page) => reader.go_to_page(page.saturating_sub(1)),
            Action::GotoFirst => reader.go_to_page(0),
            Action::GotoLast => reader.go_to_page(u32::MAX),
            Action::PageTop => reader.page_top(),
            Action::PageBottom => reader.page_bottom(),
            Action::ZoomIn => reader.zoom_in(),
            Action::ZoomOut => reader.zoom_out(),
            Action::ZoomTo(percent) => reader.zoom_to(num::float(percent) / 100.0),
            Action::FitWidth => reader.fit(Fit::Width),
            Action::FitPage => reader.fit(Fit::Page),
            Action::Rotate => reader.rotate(),
            Action::ToggleRecolor => document.recolor = !document.recolor,
            Action::Abort => document.results.shown = false,
            Action::Quit
            | Action::OpenPrompt(_)
            | Action::SearchNext
            | Action::SearchPrevious
            | Action::ToggleTrim => {}
        }
        Task::none()
    }

    fn on_wheel(&mut self, delta: mouse::ScrollDelta) {
        let line = self.scroll_step * WHEEL_STEPS;
        let (right, down) = match delta {
            mouse::ScrollDelta::Lines { x, y } => (-x * line, -y * line),
            mouse::ScrollDelta::Pixels { x, y } => (-x, -y),
        };
        let zooming = self.modifiers.control();
        let Some(document) = &mut self.document else {
            return;
        };
        if !zooming {
            document.reader.scroll_by(right, down);
        } else if down < 0.0 {
            document.reader.zoom_in();
        } else if down > 0.0 {
            document.reader.zoom_out();
        }
    }

    /// Starts opening a document. The one on screen stays until the new one
    /// is ready.
    fn open(&mut self, path: PathBuf) -> Task<Message> {
        // The history is keyed by the real path, so that a file reached by
        // two routes is one document.
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        self.notice = Notice::Info(format!("opening {}", tilde(&path)));
        Task::perform(backend::open(path.clone()), move |result| {
            Message::Opened(generation, path, result)
        })
    }

    fn on_opened(&mut self, path: PathBuf, result: Result<Opened, backend::Error>) {
        let opened = match result {
            Ok(opened) => opened,
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                return;
            }
        };
        self.remember();
        self.notice = Notice::None;
        let mut reader = Reader::new(opened.sizes, (self.area.width, self.area.height));
        let mut recolor = false;
        let mut returning = None;
        match self.history.as_ref().map(|history| history.load(&path)) {
            Some(Ok(Some(view))) => {
                reader.restore(view);
                recolor = view.recolor;
                // A trimmed view only makes sense once the margins are
                // known, so it is restored again when they are.
                returning = view.trim.then_some(view);
            }
            Some(Err(error)) => self.notice = Notice::Error(error.to_string()),
            Some(Ok(None)) | None => {}
        }
        self.document = Some(Document {
            path,
            backend: opened.backend,
            reader,
            recolor,
            trim: false,
            contents: None,
            measuring: false,
            returning,
            pages: HashMap::new(),
            rendering: HashMap::new(),
            failed: HashMap::new(),
            results: Results::default(),
        });
    }

    /// Hides or shows the blank margins of every page. The margins are
    /// measured the first time they are hidden, which takes a moment.
    fn set_trim(&mut self, trim: bool) -> Task<Message> {
        let generation = self.generation;
        let Some(document) = &mut self.document else {
            return Task::none();
        };
        document.trim = trim;
        if !trim {
            document.returning = None;
            document.reader.set_crops(None);
            return Task::none();
        }
        if let Some(contents) = &document.contents {
            document.reader.set_crops(Some(contents.clone()));
            return Task::none();
        }
        if document.measuring {
            return Task::none();
        }
        document.measuring = true;
        self.notice = Notice::Info("measuring margins".to_owned());
        Task::perform(document.backend.measure(), move |result| {
            Message::Measured(generation, result)
        })
    }

    fn on_measured(&mut self, result: Result<Vec<Rect>, backend::Error>) {
        let Some(document) = &mut self.document else {
            return;
        };
        document.measuring = false;
        let returning = document.returning.take();
        match result {
            Ok(contents) => {
                if document.trim {
                    document.reader.set_crops(Some(contents.clone()));
                    if let Some(view) = returning {
                        document.reader.restore(view);
                    }
                }
                document.contents = Some(contents);
                if matches!(self.notice, Notice::Info(_)) {
                    self.notice = Notice::None;
                }
            }
            Err(error) => {
                document.trim = false;
                self.notice = Notice::Error(error.to_string());
            }
        }
    }

    fn on_rendered(
        &mut self,
        page: u32,
        key: RenderKey,
        result: Result<image::Handle, backend::Error>,
    ) {
        let Some(document) = &mut self.document else {
            return;
        };
        document.rendering.remove(&page);
        match result {
            Ok(handle) => {
                document.pages.insert(page, (handle, key));
            }
            Err(error) => {
                document.failed.insert(page, key);
                self.notice = Notice::Error(error.to_string());
            }
        }
    }

    /// Asks for every page that is in view, or about to be, and is not yet
    /// rendered the way it is wanted now. Pages far from view are dropped.
    fn render_missing(&mut self) -> Task<Message> {
        let generation = self.generation;
        let palette = self.palette;
        let scale_factor = self.scale_factor;
        let Some(document) = &mut self.document else {
            return Task::none();
        };
        if self.area.width < 1.0 || self.area.height < 1.0 {
            return Task::none();
        }
        let scale = document.reader.zoom() * scale_factor;
        let job = RenderJob {
            page: 0,
            scale,
            rotation: document.reader.rotation(),
            clip: None,
            palette: document.recolor.then_some(palette),
        };
        let trimmed = document.reader.crop(0).is_some();
        let wanted = RenderKey {
            scale: scale.to_bits(),
            rotation: job.rotation,
            palette: job.palette,
            trimmed,
        };

        let visible = document.reader.visible_pages();
        let kept =
            visible.start.saturating_sub(KEEP_AROUND)..visible.end.saturating_add(KEEP_AROUND);
        document.pages.retain(|page, _| kept.contains(page));
        document.failed.retain(|page, _| kept.contains(page));

        let last = document.reader.page_count();
        let ahead = visible.start.saturating_sub(RENDER_AHEAD)
            ..visible.end.saturating_add(RENDER_AHEAD).min(last);
        // The pages in view go first, then their neighbours.
        let order = visible
            .clone()
            .chain(ahead.filter(|page| !visible.contains(page)));
        let mut tasks = Vec::new();
        for page in order {
            let current = document
                .pages
                .get(&page)
                .is_some_and(|(_, key)| *key == wanted);
            let failed = document.failed.get(&page) == Some(&wanted);
            // One render at a time for each page: a newer one is asked for
            // when the one under way comes back.
            if current || failed || document.rendering.contains_key(&page) {
                continue;
            }
            document.rendering.insert(page, wanted);
            let clip = document.reader.crop(page);
            let render = document.backend.render(RenderJob { page, clip, ..job });
            tasks.push(Task::perform(render, move |result| {
                let handle = result.map(|bitmap| {
                    image::Handle::from_rgba(bitmap.width, bitmap.height, bitmap.pixels)
                });
                Message::Rendered(generation, page, wanted, handle)
            }));
        }
        Task::batch(tasks)
    }

    /// Starts a search. An empty needle repeats the last one.
    fn search(&mut self, needle: &str, forward: bool) -> Task<Message> {
        let generation = self.generation;
        let Some(document) = &mut self.document else {
            return Task::none();
        };
        let needle = if needle.is_empty() {
            match &document.results.search {
                Some(last) => last.needle.clone(),
                None => return Task::none(),
            }
        } else {
            needle.to_owned()
        };
        self.notice = Notice::Info(format!("searching for {needle}"));
        let finding = document.backend.search(needle.clone());
        let search = Search { needle, forward };
        Task::perform(finding, move |result| {
            Message::Found(generation, search, result)
        })
    }

    fn on_found(&mut self, search: Search, result: Result<Vec<Hit>, backend::Error>) {
        let Some(document) = &mut self.document else {
            return;
        };
        let hits = match result {
            Ok(hits) => hits,
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                return;
            }
        };
        self.notice = if hits.is_empty() {
            Notice::Error(format!("not found: {}", search.needle))
        } else {
            Notice::None
        };
        // Start from the page in view: the first hit on or after it when
        // searching forward, the last hit on or before it otherwise.
        let here = document.reader.current_page();
        let current = if search.forward {
            hits.iter().position(|hit| hit.page >= here).or(Some(0))
        } else {
            hits.iter()
                .rposition(|hit| hit.page <= here)
                .or_else(|| hits.len().checked_sub(1))
        };
        document.results = Results {
            search: Some(search),
            hits,
            current: None,
            shown: true,
        };
        self.jump_to(current);
    }

    /// Moves to the next hit in the direction of the last search, or
    /// against it.
    fn step_search(&mut self, same_direction: bool) -> Task<Message> {
        let Some(document) = &mut self.document else {
            return Task::none();
        };
        let results = &mut document.results;
        let Some(search) = &results.search else {
            return Task::none();
        };
        results.shown = true;
        let count = results.hits.len();
        if count == 0 {
            self.notice = Notice::Error(format!("not found: {}", search.needle));
            return Task::none();
        }
        let forward = search.forward == same_direction;
        let next = match (results.current, forward) {
            (None, true) => 0,
            (None, false) => count.saturating_sub(1),
            (Some(current), true) => current.saturating_add(1).checked_rem(count).unwrap_or(0),
            (Some(current), false) => current
                .checked_sub(1)
                .unwrap_or_else(|| count.saturating_sub(1)),
        };
        self.jump_to(Some(next));
        Task::none()
    }

    /// Makes a hit the current one and brings it into view.
    fn jump_to(&mut self, index: Option<usize>) {
        let Some(document) = &mut self.document else {
            return;
        };
        let Some((index, hit)) =
            index.and_then(|index| Some((index, *document.results.hits.get(index)?)))
        else {
            return;
        };
        document.results.current = Some(index);
        document.reader.reveal(hit.page, hit.area);
        if !matches!(self.notice, Notice::Error(_)) {
            self.notice = Notice::Info(format!(
                "match {} of {}",
                index.saturating_add(1),
                document.results.hits.len()
            ));
        }
    }

    /// Records where the open document is being left.
    fn remember(&mut self) {
        let (Some(history), Some(document)) = (&self.history, &self.document) else {
            return;
        };
        // A view still waiting for the margins is the one to keep.
        let view = document
            .returning
            .unwrap_or_else(|| document.reader.view(document.recolor));
        if let Err(error) = history.save(&document.path, view) {
            self.notice = Notice::Error(error.to_string());
        }
    }

    fn quit(&mut self) -> Task<Message> {
        self.remember();
        iced::exit()
    }

    fn view(&self) -> Element<'_, Message> {
        let pages = Canvas::new(Pages(self)).width(Fill).height(Fill);
        let marks = Canvas::new(Marks(self)).width(Fill).height(Fill);
        column![stack![pages, marks], self.bar()].into()
    }

    /// The line at the bottom: the prompt while typing, otherwise a notice
    /// or the file name on the left and the position on the right.
    fn bar(&self) -> Element<'_, Message> {
        let label = |content: String, colour: Color| {
            text(content)
                .size(BAR_TEXT_SIZE)
                .font(Font::MONOSPACE)
                .color(colour)
                .wrapping(text::Wrapping::None)
        };
        let left = match (&self.prompt, &self.notice) {
            (Some((prompt, line)), _) => {
                label(format!("{}{line}\u{2588}", prompt.sigil()), BAR_TEXT)
            }
            (None, Notice::Error(message)) => label(message.clone(), BAR_ERROR),
            (None, Notice::Info(message)) => label(message.clone(), BAR_TEXT),
            (None, Notice::None) => label(
                self.document
                    .as_ref()
                    .map(|document| tilde(&document.path))
                    .unwrap_or_default(),
                BAR_TEXT,
            ),
        };
        let position = self.document.as_ref().map_or_else(String::new, |document| {
            let reader = &document.reader;
            format!(
                "{}/{}  {}%",
                reader
                    .current_page()
                    .saturating_add(1)
                    .min(reader.page_count()),
                reader.page_count(),
                num::round(reader.zoom() * 100.0),
            )
        });
        let right = label(format!("{}  {position}", self.pending.display()), BAR_TEXT);
        // A long notice is cut short rather than pushing the position away.
        let left = container(left).width(Fill).clip(true);
        container(row![left, right].spacing(16))
            .padding([3, 8])
            .width(Fill)
            .style(|_| container::background(BAR))
            .into()
    }
}

/// The layer that draws the pages, and listens to the mouse.
struct Pages<'a>(&'a App);

impl canvas::Program<Message> for Pages<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let app = self.0;
        // The canvas is the one place that knows how much room the pages
        // have, so it reports every change of size.
        let area = bounds.size();
        if (area.width - app.area.width).abs() > 0.5 || (area.height - app.area.height).abs() > 0.5
        {
            return Some(canvas::Action::publish(Message::Resized(area)));
        }
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                Some(canvas::Action::publish(Message::Wheel(*delta)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let app = self.0;
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), BACKDROP);
        let Some(document) = &app.document else {
            return vec![frame.into_geometry()];
        };
        // Until a page is rendered, a blank sheet stands in for it.
        let paper = if document.recolor {
            colour(app.palette.paper)
        } else {
            Color::WHITE
        };
        for page in document.reader.visible_pages() {
            let Some(area) = document.reader.page_on_screen(page) else {
                continue;
            };
            let area = rectangle(area);
            frame.fill_rectangle(area.position(), area.size(), paper);
            // A page rendered for another shape of page would be drawn
            // distorted, so the blank sheet stays until the new one is in.
            let rotation = document.reader.rotation();
            let trimmed = document.reader.crop(page).is_some();
            if let Some((handle, key)) = document.pages.get(&page)
                && key.rotation == rotation
                && key.trimmed == trimmed
            {
                frame.draw_image(area, canvas::Image::new(handle.clone()).snap(true));
            }
        }
        vec![frame.into_geometry()]
    }
}

/// The layer over the pages that draws the search highlights. Within one
/// layer images are drawn over shapes, so the highlights need their own.
struct Marks<'a>(&'a App);

impl canvas::Program<Message> for Marks<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let Some(document) = &self.0.document else {
            return vec![frame.into_geometry()];
        };
        let results = &document.results;
        if results.shown {
            let visible = document.reader.visible_pages();
            for (index, hit) in results.hits.iter().enumerate() {
                if !visible.contains(&hit.page) {
                    continue;
                }
                let Some(area) = document.reader.area_on_screen(hit.page, hit.area) else {
                    continue;
                };
                let area = rectangle(area);
                let fill = if results.current == Some(index) {
                    HIT_CURRENT
                } else {
                    HIT
                };
                frame.fill_rectangle(area.position(), area.size(), fill);
            }
        }
        vec![frame.into_geometry()]
    }
}

/// Reduces a key press to the keys the reader distinguishes.
fn translate(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    text: Option<&str>,
) -> Option<Key> {
    use keyboard::key::Named;

    if modifiers.alt() || modifiers.logo() {
        return None;
    }
    match key.as_ref() {
        keyboard::Key::Named(named) => match named {
            Named::Escape => Some(Key::Escape),
            Named::Enter => Some(Key::Enter),
            Named::Backspace => Some(Key::Backspace),
            Named::ArrowUp => Some(Key::Up),
            Named::ArrowDown => Some(Key::Down),
            Named::ArrowLeft => Some(Key::Left),
            Named::ArrowRight => Some(Key::Right),
            Named::PageUp => Some(Key::PageUp),
            Named::PageDown => Some(Key::PageDown),
            Named::Home => Some(Key::Home),
            Named::End => Some(Key::End),
            Named::Space if modifiers.shift() => Some(Key::ShiftSpace),
            Named::Space => Some(Key::Char(' ')),
            _ => None,
        },
        keyboard::Key::Character(character) if modifiers.control() => character
            .chars()
            .next()
            .map(|letter| Key::Ctrl(letter.to_ascii_lowercase())),
        _ => text
            .and_then(|typed| typed.chars().next())
            .filter(|character| !character.is_control())
            .map(Key::Char),
    }
}

/// A count of steps as a factor.
fn steps(count: i32) -> f32 {
    let size = num::float(count.unsigned_abs());
    if count < 0 { -size } else { size }
}

const fn colour(Rgb(red, green, blue): Rgb) -> Color {
    Color::from_rgb8(red, green, blue)
}

fn rectangle(area: Rect) -> Rectangle {
    Rectangle {
        x: area.left,
        y: area.top,
        width: area.right - area.left,
        height: area.bottom - area.top,
    }
}

/// A path with the home directory written as `~`.
fn tilde(path: &Path) -> String {
    std::env::var_os("HOME")
        .and_then(|home| path.strip_prefix(home).ok())
        .map_or_else(
            || path.display().to_string(),
            |rest| format!("~/{}", rest.display()),
        )
}
