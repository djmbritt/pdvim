//! The window: what is drawn, and what happens on each key press.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use iced::widget::canvas::{self, Canvas};
use iced::widget::{Column, Row, column, container, image, row, stack, text};
use iced::{
    Color, Element, Event, Fill, Font, Point, Rectangle, Renderer, Size, Subscription, Task, Theme,
    keyboard, mouse, window,
};

use crate::backend::{
    self, Annotation, Backend, Hit, Link, Marking, Markup, MarkupKind, Opened, RenderJob, Target,
};
use crate::bindings::{self, Keymaps};
use crate::command::{self, Command, Recolor, Setting};
use crate::complete::{self, Candidate};
use crate::config;
use crate::help::{Help, Line};
use crate::hints::{Hints, Outcome};
use crate::history::{Fit, History, View};
use crate::input::{self, Action, ConfirmBinding, Key, MarkVerb, Prompt, PromptBinding};
use crate::jumps::Jumps;
use crate::keymap::{self, Counts, Fed, Match, Pending};
use crate::layout::Rect;
use crate::num;
use crate::outline::{self, Outline};
use crate::reader::{Place, Reader};
use crate::recent::{self, Choice, Picker};
use crate::recolor::{Palette, Rgb};
use crate::theme::{self, Colours};
use crate::visual::{self, Kind, Spot, Visual};

mod info;
mod markings;

use markings::{Edit, Step, Wanted};

/// The colour laid over every search result.
const HIT: Color = Color::from_rgba8(0xff, 0xd0, 0x00, 0.35);
/// The colour laid over the search result last jumped to.
const HIT_CURRENT: Color = Color::from_rgba8(0xff, 0x70, 0x00, 0.5);
/// The colour laid over a link while links are labelled.
const LINK: Color = Color::from_rgba8(0x40, 0x90, 0xff, 0.25);
/// The colour behind a link's label, and of its letters.
const LABEL: Color = Color::from_rgb8(0xff, 0xd0, 0x00);
const LABEL_TEXT: Color = Color::from_rgb8(0x10, 0x10, 0x10);
/// The size of a link's label, in pixels.
const LABEL_SIZE: f32 = 12.0;
/// The colour laid over selected text, and over the character the cursor
/// is on.
const SELECTION: Color = Color::from_rgba8(0x40, 0x90, 0xff, 0.35);
const CURSOR: Color = Color::from_rgba8(0xff, 0x70, 0x00, 0.6);

/// The height of one line of the cheat sheet, in pixels.
const HELP_ROW: f32 = 20.0;
/// The width of the table of contents sidebar, in pixels.
const SIDEBAR_WIDTH: f32 = 380.0;
/// How far each level of the table of contents is set in, in pixels.
const SIDEBAR_INDENT: f32 = 14.0;
/// The room the cheat sheet takes besides its lines: the margin around it,
/// its padding, and the line at its foot, in pixels.
const HELP_CHROME: f32 = 24.0 * 2.0 + 8.0 * 2.0 + HELP_ROW + 4.0;
/// The widest the cheat sheet grows, in pixels.
const HELP_WIDTH: f32 = 900.0;
/// The width of the keys and of the action names on the cheat sheet.
const HELP_KEYS_WIDTH: f32 = 200.0;
const HELP_ACTION_WIDTH: f32 = 150.0;

/// How many completions the list above the prompt shows at a time.
const COMPLETIONS_SHOWN: usize = 60;

/// The size of status bar text, in pixels.
const BAR_TEXT_SIZE: f32 = 13.0;
/// The size of text in the list of recent documents, in pixels.
const LIST_TEXT_SIZE: f32 = 14.0;
/// The widest the list of recent documents grows, in pixels.
const LIST_WIDTH: f32 = 720.0;

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
    /// The system's preference for a light or dark theme became known, or
    /// changed.
    SystemTheme(iced::theme::Mode),
    /// A document finished opening, or failed to, for the request with
    /// this number.
    Opened(u64, PathBuf, Result<Opened, backend::Error>),
    /// A page of the document with this number finished rendering, or
    /// failed to.
    Rendered(u64, u32, RenderKey, Result<image::Handle, backend::Error>),
    /// A search in the document with this number finished, or failed.
    Found(u64, Search, Result<Vec<Hit>, backend::Error>),
    /// The margins of every page of the document with this number were
    /// measured, or could not be.
    Measured(u64, Result<Vec<Rect>, backend::Error>),
    /// The links on some pages of the document with this number were
    /// found, or could not be.
    LinksFound(u64, Result<backend::PageLinks, backend::Error>),
    /// The table of contents of the document with this number was read, or
    /// could not be.
    OutlineFound(u64, Result<Vec<backend::Chapter>, backend::Error>),
    /// A change to the markings of the document with this number was
    /// made, taken back or put back, or failed to be: the change as it was
    /// asked for, and as it is once made.
    Edited(u64, Edit, Step, Result<Edit, backend::Error>),
    /// The markings of the document with this number were found, as it
    /// was at this edition, or could not be; and what they were wanted for.
    MarkingsFound(u64, u32, Wanted, Result<Vec<Marking>, backend::Error>),
    /// The changes to the document with this number were saved, or not:
    /// into its own file or as another, the number of changes made by then,
    /// and what to do next.
    Saved(
        u64,
        Option<PathBuf>,
        usize,
        Result<(), backend::Error>,
        AfterSave,
    ),
    /// The text on some pages of the document with this number was read,
    /// or could not be.
    TextFound(u64, Vec<u32>, Result<backend::PageText, backend::Error>),
}

/// What to do once a document's changes are saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterSave {
    Nothing,
    /// Close the document's tab.
    Close,
    /// Close the program, once every document being saved is.
    Quit,
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
    /// How many times the document had been changed, as by highlighting.
    edition: u32,
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
    /// Tells answers meant for this document apart from those meant for
    /// another, or for one since closed.
    id: u64,
    path: PathBuf,
    backend: Backend,
    /// What the document says about itself.
    info: Box<backend::Info>,
    /// The number printed on each page, when the document gives them.
    labels: Option<Vec<String>>,
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
    /// How many times the document has been changed, as by highlighting.
    /// Pages rendered before the last change are rendered again.
    edition: u32,
    /// The changes made to the file, the last made last, to undo.
    undo: Vec<Edit>,
    /// The markings, once found, until the next change.
    markings: Option<Vec<Marking>>,
    /// The marking last gone to, which `dh` takes away.
    marking: Option<Annotation>,
    /// The changes undone, the last undone last, to redo.
    redo: Vec<Edit>,
    /// The rendered pages, and what each was rendered for.
    pages: HashMap<u32, (image::Handle, RenderKey)>,
    /// What each page being rendered right now is rendered for.
    rendering: HashMap<u32, RenderKey>,
    /// What each page that failed to render was rendered for, so the same
    /// render is not tried again and again.
    failed: HashMap<u32, RenderKey>,
    results: Results,
    /// The table of contents, once read.
    chapters: Option<Vec<backend::Chapter>>,
    /// The links on each page whose links have been asked for.
    links: HashMap<u32, Vec<Link>>,
    /// The lines of text on each page read so far.
    text: visual::Text,
    /// The pages whose text is being read right now.
    reading: std::collections::HashSet<u32>,
    /// The places big moves left from, to go back to.
    jumps: Jumps,
    /// How many changes the document had when it was last saved, counted
    /// as the undo list's length; none when that state is gone for good.
    saved: Option<usize>,
    /// The marks of this document, a to z.
    marks: HashMap<char, Place>,
    /// A place to go once a trimmed view is restored, which would move
    /// away from it otherwise.
    arriving: Option<Place>,
}

/// A tab: a document, or none yet, and the document being opened in it.
#[derive(Default)]
struct Tab {
    document: Option<Document>,
    /// The request for the document being opened in this tab, and its
    /// path. The document already in the tab stays until it is ready.
    opening: Option<(u64, PathBuf)>,
}

impl Tab {
    /// The name of the tab: what its document goes by, or the file name
    /// of the one on its way.
    fn name(&self) -> String {
        if let Some(document) = &self.document {
            return document.name();
        }
        let path = self.opening.as_ref().map(|(_, path)| path);
        path.and_then(|path| path.file_name()).map_or_else(
            || "[empty]".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    /// Whether this document is in the tab, or on its way.
    fn holds(&self, path: &Path) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| document.path == path)
            || self
                .opening
                .as_ref()
                .is_some_and(|(_, opening)| opening == path)
    }

    /// Whether nothing is in the tab, nor on its way.
    const fn is_empty(&self) -> bool {
        self.document.is_none() && self.opening.is_none()
    }
}

/// The completions on offer for the word being typed at the prompt.
struct Completing {
    /// Where the word starts in the line, in bytes.
    start: usize,
    candidates: Vec<Candidate>,
    /// The candidate now in the line, once Tab has been pressed again.
    selected: Option<usize>,
    /// Whether the list was narrowed by typing since Tab was last pressed,
    /// so that the next Tab completes afresh rather than going through it.
    typed: bool,
}

impl Document {
    /// Whether the document has changes not saved into its file.
    fn modified(&self) -> bool {
        self.saved != Some(self.undo.len())
    }
}

/// The colours text is marked in. The built-in configuration sets them.
#[derive(Debug, Clone, Copy)]
struct MarkupColours {
    highlight: Rgb,
    underline: Rgb,
    strike_out: Rgb,
    squiggly: Rgb,
}

impl Default for MarkupColours {
    fn default() -> Self {
        let yellow = Rgb(0xff, 0xff, 0x00);
        Self {
            highlight: yellow,
            underline: yellow,
            strike_out: yellow,
            squiggly: yellow,
        }
    }
}

impl MarkupColours {
    const fn of(self, kind: MarkupKind) -> Rgb {
        match kind {
            MarkupKind::Highlight => self.highlight,
            MarkupKind::Underline => self.underline,
            MarkupKind::StrikeOut => self.strike_out,
            MarkupKind::Squiggly => self.squiggly,
        }
    }
}

/// When to ask before closing, and whether to save changes at once.
#[derive(Debug, Clone, Copy, Default)]
struct Care {
    /// Whether to ask before quitting.
    confirm_quit: bool,
    /// Whether to ask before closing a tab, when others are left.
    confirm_tab_close: bool,
    /// Whether changes are saved as soon as they are made.
    autosave: bool,
}

/// How a document with no view in the history starts.
#[derive(Debug, Clone, Copy, Default)]
struct Fresh {
    recolor: bool,
    /// Whether the margins are trimmed.
    trim: bool,
}

/// A question the bottom line asks before doing something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Question {
    Quit,
    CloseTab,
    /// Close the tab in view, whose document has changes not saved.
    DiscardTab,
    /// Quit, though documents have changes not saved.
    DiscardAll,
}

/// What a sidebar over the left of the pages lists.
enum Sidebar {
    /// The table of contents.
    Outline(Outline),
    /// The highlights and other markings.
    Markings(crate::markings::List),
}

/// What the bottom line says when nothing is being typed.
enum Notice {
    None,
    Info(String),
    Error(String),
}

/// The state of the whole program.
pub struct App {
    /// The tabs, never fewer than one.
    tabs: Vec<Tab>,
    /// The index of the tab in view.
    current: usize,
    /// The last number given to a document or a request to open one.
    last_id: u64,
    history: Option<History>,
    /// The size of the area for pages, in logical pixels.
    area: Size,
    /// Physical pixels per logical pixel.
    scale_factor: f32,
    modifiers: keyboard::Modifiers,
    keymaps: Keymaps,
    /// What has been typed of a command in normal mode.
    pending: Pending<input::Binding>,
    /// What has been typed of a sequence bound in the prompt.
    prompt_pending: Pending<PromptBinding>,
    /// The prompt being typed into, and the text so far.
    prompt: Option<(Prompt, String)>,
    /// The completions shown above the prompt, while there are any.
    completing: Option<Completing>,
    /// The list of recent documents, while it is shown.
    picker: Option<Picker>,
    /// The cheat sheet, while it is shown.
    help: Option<Help>,
    /// What the document in view says about itself, by name, while it is
    /// shown.
    info: Option<(String, Vec<(&'static str, String)>)>,
    /// The sidebar, while it is shown.
    sidebar: Option<Sidebar>,
    /// The labels on the links in view, while they are shown.
    hints: Option<Hints<Link>>,
    /// The document whose links are being found so that they can be
    /// labelled, by its number.
    hints_wanted: Option<u64>,
    /// The labels on the lines in view, while they are shown, and how text
    /// will be selected from the line picked.
    line_hints: Option<(Kind, Hints<Spot>)>,
    /// The document whose text is being read so that its lines can be
    /// labelled, by its number, and how text will be selected.
    lines_wanted: Option<(u64, Kind)>,
    /// The selection, in visual mode.
    visual: Option<Visual>,
    notice: Notice,
    /// What recoloured paper and ink become, when set rather than taken
    /// from the theme.
    paper: Option<Rgb>,
    ink: Option<Rgb>,
    /// The theme, when set rather than following the system's.
    theme: Option<theme::Theme>,
    /// The themes followed while the system's is dark, and light.
    theme_dark: theme::Theme,
    theme_light: theme::Theme,
    /// The colour text is highlighted in.
    /// The colours text is marked in, by kind of marking.
    markup_colours: MarkupColours,
    /// How far one press of `j` scrolls, in logical pixels.
    scroll_step: f32,
    /// How a document with no view in the history starts.
    fresh: Fresh,
    /// When to ask, and when to save.
    care: Care,
    /// How many saves are still to finish before quitting, after `:wqa`.
    quitting_after: Option<usize>,
    /// Whether documents are recoloured to match the system's theme.
    recolor_auto: bool,
    /// Whether the system prefers a dark theme, once known, if it has a
    /// preference.
    system_dark: Option<bool>,
    /// What the bottom line is asking, waiting for `y` or anything else.
    asking: Option<Question>,
    /// Whether the next key is the letter of a mark to set, or to go to.
    awaiting_mark: Option<MarkVerb>,
    /// The marks A to Z: a document and a place in it. Kept in the history
    /// too, when there is one.
    global_marks: HashMap<char, (PathBuf, Place)>,
    /// A document being opened to go to a mark in it, and the place.
    arriving: Option<(PathBuf, Place)>,
}

/// Runs the reader, opening each document given in a tab of its own.
///
/// # Errors
///
/// When the window cannot be opened.
pub fn run(paths: Vec<PathBuf>) -> iced::Result {
    iced::application(move || App::boot(paths.clone()), App::update, App::view)
        .title(App::title)
        .subscription(|_: &App| App::subscription())
        .theme(|app: &App| Some(app.theme()))
        .exit_on_close_request(false)
        .window(window::Settings {
            platform_specific: platform_specific(),
            ..window::Settings::default()
        })
        .run()
}

impl App {
    fn boot(paths: Vec<PathBuf>) -> (Self, Task<Message>) {
        let (history, notice) = match History::open() {
            Ok(history) => (Some(history), Notice::None),
            Err(error) => (None, Notice::Error(format!("history is off: {error}"))),
        };
        let mut app = Self {
            tabs: vec![Tab::default()],
            current: 0,
            last_id: 0,
            history,
            area: Size::ZERO,
            scale_factor: 1.0,
            modifiers: keyboard::Modifiers::default(),
            // Bindings and settings come from the built-in configuration,
            // carried out in `configure`; these hold only until then.
            keymaps: Keymaps::empty(),
            pending: Pending::new(),
            prompt_pending: Pending::new(),
            prompt: None,
            completing: None,
            picker: None,
            help: None,
            info: None,
            sidebar: None,
            hints: None,
            hints_wanted: None,
            line_hints: None,
            lines_wanted: None,
            visual: None,
            notice,
            paper: None,
            ink: None,
            theme: None,
            theme_dark: theme::Theme::PdvimDark,
            theme_light: theme::Theme::PdvimLight,
            markup_colours: MarkupColours::default(),
            scroll_step: 40.0,
            fresh: Fresh::default(),
            care: Care::default(),
            quitting_after: None,
            recolor_auto: false,
            system_dark: None,
            asking: None,
            awaiting_mark: None,
            global_marks: HashMap::new(),
            arriving: None,
        };
        // Started without a document, it offers the ones read lately.
        if paths.is_empty() {
            app.picker = Picker::new(app.recent_documents());
        }
        let tasks: Vec<_> = paths
            .into_iter()
            .map(|path| app.open_in_tab(path))
            .collect();
        app.current = 0;
        // The documents are still opening, so the defaults set here apply
        // to them, and a problem here is shown over the opening notice.
        app.configure();
        // The theme is also told on every change, but not always at start.
        let theme = iced::system::theme().map(Message::SystemTheme);
        (app, Task::batch(tasks.into_iter().chain([theme])))
    }

    /// Recolours every open document, or stops, to match the system's
    /// theme, when following it and the theme is known.
    fn follow_system(&mut self) {
        let Some(dark) = self.system_dark.filter(|_| self.recolor_auto) else {
            return;
        };
        for document in self.tabs.iter_mut().filter_map(|tab| tab.document.as_mut()) {
            document.recolor = dark;
        }
    }

    /// Carries out the built-in configuration, then the configuration file
    /// over it, and shows the file's first problem.
    fn configure(&mut self) {
        // The built-in configuration is tested to have no problems.
        let (defaults, _) = config::parse(bindings::DEFAULTS);
        for command in defaults {
            // No document is open yet, so nothing here starts a task.
            let _ = self.run_command(command);
        }
        let text = match config::read() {
            Ok(text) => text,
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                return;
            }
        };
        let (commands, problems) = config::parse(&text);
        for command in commands {
            // No document is open yet, so nothing here starts a task.
            let _ = self.run_command(command);
        }
        for problem in &problems {
            eprintln!("pdvim: config.toml: {problem}");
        }
        if let Some(first) = problems.first() {
            let more = match problems.len() {
                1 => String::new(),
                count => format!(" (and {} more)", count.saturating_sub(1)),
            };
            self.notice = Notice::Error(format!("config.toml: {first}{more}"));
        }
    }

    fn title(&self) -> String {
        self.document().map_or_else(
            || "pdvim".to_owned(),
            |document| {
                let unsaved = if document.modified() { " [+]" } else { "" };
                format!("{}{unsaved} - pdvim", document.name())
            },
        )
    }

    fn subscription() -> Subscription<Message> {
        Subscription::batch([
            keyboard::listen().map(Message::Keyboard),
            window::close_requests().map(|_| Message::CloseRequested),
            iced::system::theme_changes().map(Message::SystemTheme),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message {
            Message::Keyboard(event) => self.on_keyboard(event),
            Message::Resized(area) => {
                self.area = area;
                // The labels were placed for the old size.
                self.hints = None;
                self.line_hints = None;
                for document in self.tabs.iter_mut().filter_map(|tab| tab.document.as_mut()) {
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
                // The labels stay where the links were.
                self.hints = None;
                self.line_hints = None;
                self.on_wheel(delta);
                Task::none()
            }
            // Asked a second time, as when a window manager insists, the
            // window closes without asking again.
            Message::CloseRequested
                if matches!(self.asking, Some(Question::Quit | Question::DiscardAll)) =>
            {
                self.exit()
            }
            Message::CloseRequested => self.quit(),
            Message::SystemTheme(mode) => {
                self.system_dark = match mode {
                    iced::theme::Mode::Dark => Some(true),
                    iced::theme::Mode::Light => Some(false),
                    iced::theme::Mode::None => None,
                };
                self.follow_system();
                Task::none()
            }
            Message::Opened(request, path, result) => self.on_opened(request, path, result),
            Message::Rendered(id, page, key, result) => {
                self.on_rendered(id, page, key, result);
                Task::none()
            }
            Message::TextFound(id, pages, result) => {
                self.on_text(id, &pages, result);
                Task::none()
            }
            Message::Edited(id, edit, step, result) => self.on_edited(id, edit, step, result),
            Message::MarkingsFound(id, edition, wanted, result) => {
                self.on_markings(id, edition, wanted, result)
            }
            Message::Saved(id, target, changes, result, after) => {
                self.on_saved(id, target.as_deref(), changes, result, after)
            }
            Message::OutlineFound(id, result) => {
                self.on_outline(id, result);
                Task::none()
            }
            Message::LinksFound(id, result) => {
                self.on_links(id, result);
                Task::none()
            }
            Message::Measured(id, result) => {
                self.on_measured(id, result);
                Task::none()
            }
            Message::Found(id, search, result) => {
                self.on_found(id, search, result);
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
                if let Some(question) = self.asking.take() {
                    // A yes goes ahead; any other key thinks better of it.
                    let yes =
                        self.keymaps.confirm.lookup(&[key]) == Match::Bound(ConfirmBinding::Yes);
                    match question {
                        Question::Quit | Question::DiscardAll if yes => self.exit(),
                        Question::CloseTab | Question::DiscardTab if yes => self.close_tab_now(),
                        Question::Quit
                        | Question::CloseTab
                        | Question::DiscardTab
                        | Question::DiscardAll => Task::none(),
                    }
                } else if self.info.take().is_some() {
                    // Any key puts it away.
                    Task::none()
                } else if self.visual.is_some() {
                    self.on_visual_key(key)
                } else if let Some((kind, hints)) = &mut self.line_hints {
                    let kind = *kind;
                    let outcome = hints.feed(key, &self.keymaps.hints);
                    self.on_line_hint(kind, outcome)
                } else if let Some(hints) = &mut self.hints {
                    let outcome = hints.feed(key, &self.keymaps.hints);
                    self.on_hint(outcome);
                    Task::none()
                } else if self.sidebar.is_some() {
                    self.on_sidebar_key(key)
                } else if self.help.is_some() {
                    let rows = self.help_rows();
                    if let Some(help) = &mut self.help
                        && !help.feed(key, &self.keymaps.help, rows)
                    {
                        self.help = None;
                    }
                    Task::none()
                } else if self.picker.is_some() {
                    self.on_picker_key(key)
                } else if self.prompt.is_some() {
                    self.on_prompt_key(key)
                } else if let Some(verb) = self.awaiting_mark.take() {
                    self.on_mark(verb, key)
                } else {
                    match self.pending.feed(key, &self.keymaps.normal, Counts::Taken) {
                        Fed::Bound(binding, count) => self.act(binding.action(count)),
                        Fed::Waiting | Fed::Unbound(_) => Task::none(),
                    }
                }
            }
        }
    }

    /// Moves through the list of recent documents, or opens one.
    fn on_picker_key(&mut self, key: Key) -> Task<Message> {
        let Some(picker) = &mut self.picker else {
            return Task::none();
        };
        match picker.feed(key, &self.keymaps.recent) {
            Some(Choice::Open(path)) => {
                self.picker = None;
                self.open_in_tab(path)
            }
            Some(Choice::Close) => {
                self.picker = None;
                Task::none()
            }
            None => Task::none(),
        }
    }

    /// Labels the links in view. The links on pages not seen before are
    /// found first, which takes a moment.
    fn label_links(&mut self) -> Task<Message> {
        self.pending = Pending::new();
        let Some(document) = self.document() else {
            return Task::none();
        };
        let id = document.id;
        let missing: Vec<u32> = document
            .reader
            .visible_pages()
            .filter(|page| !document.links.contains_key(page))
            .collect();
        if missing.is_empty() {
            self.show_hints();
            return Task::none();
        }
        let finding = document.backend.links(missing);
        self.hints_wanted = Some(id);
        Task::perform(finding, move |result| Message::LinksFound(id, result))
    }

    fn on_links(&mut self, id: u64, result: Result<backend::PageLinks, backend::Error>) {
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

    /// Labels the links that show in the window, in reading order.
    fn show_hints(&mut self) {
        let Some(document) = self.document() else {
            return;
        };
        let (width, height) = (self.area.width, self.area.height);
        let reader = &document.reader;
        let mut links: Vec<(u32, Link)> = reader
            .visible_pages()
            .flat_map(|page| {
                document
                    .links
                    .get(&page)
                    .into_iter()
                    .flatten()
                    .map(move |link| (page, link))
            })
            .filter(|(page, link)| {
                reader.area_on_screen(*page, link.area).is_some_and(|area| {
                    area.right > 0.0 && area.left < width && area.bottom > 0.0 && area.top < height
                })
            })
            .map(|(page, link)| (page, link.clone()))
            .collect();
        links.sort_by(|(page, link), (other_page, other)| {
            page.cmp(other_page)
                .then(link.area.top.total_cmp(&other.area.top))
                .then(link.area.left.total_cmp(&other.area.left))
        });
        self.hints = Hints::new(links);
        if self.hints.is_none() {
            self.notice = Notice::Info("no links in view".to_owned());
        }
    }

    /// Acts on a key typed while the links are labelled.
    fn on_hint(&mut self, outcome: Outcome<Link>) {
        match outcome {
            Outcome::Wait => {}
            Outcome::Cancel => self.hints = None,
            Outcome::NoMatch => {
                self.hints = None;
                self.notice = Notice::Error("no such link".to_owned());
            }
            Outcome::Picked(hint) => {
                self.hints = None;
                match hint.item.target {
                    Target::Page { page, top } => {
                        if let Some(document) = self.document_mut() {
                            document.jumps.record(document.reader.place());
                            document.reader.go_to_spot(page, top);
                        }
                    }
                    Target::Uri(uri) => {
                        self.notice = match open_externally(&uri) {
                            Ok(()) => Notice::Info(format!("opening {uri}")),
                            Err(error) => Notice::Error(format!("cannot open {uri}: {error}")),
                        };
                    }
                }
            }
        }
    }

    /// Labels the lines in view, to start selecting text at one. The text
    /// of pages not read before is read first.
    fn label_lines(&mut self, kind: Kind) -> Task<Message> {
        self.pending = Pending::new();
        let Some(document) = self.document() else {
            return Task::none();
        };
        let id = document.id;
        let pages: Vec<u32> = document.reader.visible_pages().collect();
        if pages.iter().all(|page| document.text.contains_key(page)) {
            self.show_line_hints(kind);
            return Task::none();
        }
        self.lines_wanted = Some((id, kind));
        self.read_text(pages)
    }

    /// Reads the text of those of some pages of the document in view not
    /// read nor being read.
    fn read_text(&mut self, pages: Vec<u32>) -> Task<Message> {
        let Some(document) = self.document_mut() else {
            return Task::none();
        };
        let missing: Vec<u32> = pages
            .into_iter()
            .filter(|page| !document.text.contains_key(page) && !document.reading.contains(page))
            .collect();
        if missing.is_empty() {
            return Task::none();
        }
        document.reading.extend(missing.iter().copied());
        let id = document.id;
        let reading = document.backend.text(missing.clone());
        Task::perform(reading, move |result| {
            Message::TextFound(id, missing, result)
        })
    }

    fn on_text(
        &mut self,
        id: u64,
        pages: &[u32],
        result: Result<backend::PageText, backend::Error>,
    ) {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return;
        };
        for page in pages {
            document.reading.remove(page);
        }
        match result {
            Ok(text) => document.text.extend(text),
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                return;
            }
        }
        if let Some((wanted, kind)) = self.lines_wanted
            && wanted == id
            && self.document().is_some_and(|document| document.id == id)
            && let Some(document) = self.document()
            && document
                .reader
                .visible_pages()
                .all(|page| document.text.contains_key(&page))
        {
            self.lines_wanted = None;
            self.show_line_hints(kind);
        }
    }

    /// Labels the lines that show in the window, in reading order.
    fn show_line_hints(&mut self, kind: Kind) {
        let Some(document) = self.document() else {
            return;
        };
        let (width, height) = (self.area.width, self.area.height);
        let reader = &document.reader;
        let mut lines: Vec<(Rect, Spot)> = Vec::new();
        for page in reader.visible_pages() {
            for (index, line) in document.text.get(&page).into_iter().flatten().enumerate() {
                let Some(first) = line.chars.first() else {
                    continue;
                };
                let area = line.chars.iter().fold(first.area, |area, character| Rect {
                    left: area.left.min(character.area.left),
                    top: area.top.min(character.area.top),
                    right: area.right.max(character.area.right),
                    bottom: area.bottom.max(character.area.bottom),
                });
                let shows = reader.area_on_screen(page, area).is_some_and(|area| {
                    area.right > 0.0 && area.left < width && area.bottom > 0.0 && area.top < height
                });
                if shows {
                    lines.push((area, Spot::line_start(page, index)));
                }
            }
        }
        lines.sort_by(|(area, spot), (other_area, other)| {
            spot.page
                .cmp(&other.page)
                .then(area.top.total_cmp(&other_area.top))
                .then(area.left.total_cmp(&other_area.left))
        });
        let spots = lines
            .into_iter()
            .map(|(_, spot)| (spot.page, spot))
            .collect();
        match Hints::new(spots) {
            Some(hints) => self.line_hints = Some((kind, hints)),
            None => self.notice = Notice::Info("no text in view".to_owned()),
        }
    }

    /// Acts on a key typed while the lines are labelled.
    fn on_line_hint(&mut self, kind: Kind, outcome: Outcome<Spot>) -> Task<Message> {
        match outcome {
            Outcome::Wait => Task::none(),
            Outcome::Cancel => {
                self.line_hints = None;
                Task::none()
            }
            Outcome::NoMatch => {
                self.line_hints = None;
                self.notice = Notice::Error("no such line".to_owned());
                Task::none()
            }
            Outcome::Picked(hint) => {
                self.line_hints = None;
                self.visual = Some(Visual::new(kind, hint.item));
                self.follow_cursor()
            }
        }
    }

    /// Moves the selection, copies it, or leaves visual mode.
    fn on_visual_key(&mut self, key: Key) -> Task<Message> {
        let (Some(visual), Some(document)) = (
            &mut self.visual,
            self.tabs
                .get(self.current)
                .and_then(|tab| tab.document.as_ref()),
        ) else {
            self.visual = None;
            return Task::none();
        };
        match visual.feed(key, &self.keymaps.visual, &document.text) {
            visual::Outcome::Stay => self.follow_cursor(),
            visual::Outcome::Leave => {
                self.visual = None;
                Task::none()
            }
            visual::Outcome::Mark(kind, areas) => {
                self.visual = None;
                self.highlight(kind, areas)
            }
            visual::Outcome::Unmark(areas) => {
                self.visual = None;
                self.with_markings(Wanted::DeleteIn(areas))
            }
            visual::Outcome::Copy(text) => {
                self.visual = None;
                let count = text.chars().count();
                self.notice = Notice::Info(format!(
                    "copied {count} character{}",
                    if count == 1 { "" } else { "s" }
                ));
                iced::clipboard::write(text)
            }
        }
    }

    /// Highlights text in the file of the document in view, a line's area
    /// at a time.
    fn highlight(&mut self, kind: MarkupKind, areas: Vec<(u32, Rect)>) -> Task<Message> {
        let colour = self.markup_colours.of(kind);
        let Some(document) = self.document() else {
            return Task::none();
        };
        let mut pages: Vec<(u32, Vec<Rect>)> = Vec::new();
        for (page, area) in areas {
            match pages.last_mut() {
                Some((last, lines)) if *last == page => lines.push(area),
                _ => pages.push((page, vec![area])),
            }
        }
        if pages.is_empty() {
            return Task::none();
        }
        let id = document.id;
        let markup = Markup {
            kind,
            pages,
            colour,
        };
        self.notice = Notice::Info("marking".to_owned());
        self.start_edit(
            id,
            Edit::Marked {
                markup,
                annotations: Vec::new(),
            },
            Step::Do,
        )
    }

    /// Says that a document changed, and saves it if that is wanted.
    fn changed(&mut self, id: u64, done: &str) -> Task<Message> {
        if self.care.autosave {
            self.notice = Notice::Info(format!("{done}; saving"));
            return self.save(id, None, AfterSave::Nothing);
        }
        self.notice = Notice::Info(format!("{done}; :w saves it into the file"));
        Task::none()
    }

    /// Saves the changes to a document, into its own file or as a new file,
    /// then does what follows.
    fn save(&mut self, id: u64, target: Option<PathBuf>, after: AfterSave) -> Task<Message> {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return Task::none();
        };
        let changes = document.undo.len();
        let saving = document.backend.save(target.clone());
        Task::perform(saving, move |result| {
            Message::Saved(id, target, changes, result, after)
        })
    }

    fn on_saved(
        &mut self,
        id: u64,
        target: Option<&Path>,
        changes: usize,
        result: Result<(), backend::Error>,
        after: AfterSave,
    ) -> Task<Message> {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return Task::none();
        };
        if let Err(error) = result {
            self.notice = Notice::Error(error.to_string());
            // Quitting would lose what failed to be saved.
            self.quitting_after = None;
            return Task::none();
        }
        let written = if let Some(path) = target {
            tilde(path)
        } else {
            document.saved = Some(changes);
            tilde(&document.path)
        };
        self.notice = Notice::Info(format!("saved {written}"));
        match after {
            AfterSave::Nothing => Task::none(),
            AfterSave::Close => self.close_document(id),
            AfterSave::Quit => {
                let left = self.quitting_after.map(|left| left.saturating_sub(1));
                self.quitting_after = left;
                if left == Some(0) {
                    return self.exit();
                }
                Task::none()
            }
        }
    }

    /// Saves the document in view, for `:w`: into its own file, or as a new
    /// file that must not exist already unless forced.
    fn write(&mut self, target: Option<PathBuf>, force: bool) -> Task<Message> {
        let Some(document) = self.document() else {
            return Task::none();
        };
        let id = document.id;
        // A target that is the document's own file is a save into it.
        let target = target.filter(|target| {
            std::fs::canonicalize(target).map_or(true, |real| real != document.path)
        });
        if let Some(target) = &target
            && target.exists()
            && !force
        {
            self.notice = Notice::Error(format!(
                "{} exists: :w! {} overwrites it",
                tilde(target),
                target.display()
            ));
            return Task::none();
        }
        self.notice = Notice::Info("saving".to_owned());
        self.save(id, target, AfterSave::Nothing)
    }

    /// Saves the document in view if it has changes, then closes its tab,
    /// for `:wq`.
    fn write_and_close(&mut self) -> Task<Message> {
        let Some(document) = self.document() else {
            return self.close_tab();
        };
        if !document.modified() {
            return self.close_tab_now();
        }
        let id = document.id;
        self.save(id, None, AfterSave::Close)
    }

    /// Saves every document with changes not saved, then does what follows.
    fn write_all(&mut self, after: AfterSave) -> Task<Message> {
        let ids: Vec<u64> = self
            .tabs
            .iter()
            .filter_map(|tab| tab.document.as_ref())
            .filter(|document| document.modified())
            .map(|document| document.id)
            .collect();
        if after == AfterSave::Quit {
            if ids.is_empty() {
                return self.exit();
            }
            self.quitting_after = Some(ids.len());
        }
        if ids.is_empty() {
            self.notice = Notice::Info("nothing to save".to_owned());
            return Task::none();
        }
        let tasks: Vec<_> = ids
            .into_iter()
            .map(|id| self.save(id, None, after))
            .collect();
        Task::batch(tasks)
    }

    /// Closes the tab of a document, wherever it is, without asking.
    fn close_document(&mut self, id: u64) -> Task<Message> {
        let Some(index) = self.tabs.iter().position(|tab| {
            tab.document
                .as_ref()
                .is_some_and(|document| document.id == id)
        }) else {
            return Task::none();
        };
        if index == self.current {
            return self.close_tab_now();
        }
        if let Some(document) = self.tabs.get_mut(index).and_then(|tab| tab.document.take()) {
            self.remember(&document);
        }
        self.remove_tab(index);
        Task::none()
    }

    /// Keeps the cursor in view, and reads the pages it may move into next.
    fn follow_cursor(&mut self) -> Task<Message> {
        let Some(cursor) = self.visual.as_ref().map(Visual::cursor) else {
            return Task::none();
        };
        let Some(document) = self.document_mut() else {
            return Task::none();
        };
        if let Some(area) = visual::character(&document.text, cursor) {
            document.reader.reveal(cursor.page, area);
        }
        let neighbours = self
            .visual
            .as_ref()
            .map(|visual| {
                visual.neighbours(
                    self.document()
                        .map_or(0, |document| document.reader.page_count()),
                )
            })
            .unwrap_or_default();
        self.read_text(neighbours)
    }

    /// Shows the table of contents in the sidebar, reading it first the
    /// first time.
    fn show_outline(&mut self) -> Task<Message> {
        self.pending = Pending::new();
        let Some(document) = self.document() else {
            return Task::none();
        };
        if let Some(chapters) = &document.chapters {
            let page = document.reader.current_page();
            self.sidebar = Outline::new(chapters, page).map(Sidebar::Outline);
            if self.sidebar.is_none() {
                self.notice = Notice::Info("this document has no table of contents".to_owned());
            }
            return Task::none();
        }
        let id = document.id;
        let reading = document.backend.outline();
        Task::perform(reading, move |result| Message::OutlineFound(id, result))
    }

    fn on_outline(&mut self, id: u64, result: Result<Vec<backend::Chapter>, backend::Error>) {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return;
        };
        match result {
            Ok(chapters) => document.chapters = Some(chapters),
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                return;
            }
        }
        // Only if the document is still the one in view.
        if self.document().is_some_and(|document| document.id == id) {
            let _ = self.show_outline();
        }
    }

    /// Goes to the mark a document was opened to go to, if it was. Answers
    /// with the place again when the view is still to be restored, after
    /// its margins are measured, which would move away from it.
    fn arrive(&mut self, path: &Path, reader: &mut Reader, returning: bool) -> Option<Place> {
        let (_, place) = self.arriving.take_if(|(to, _)| to == path)?;
        reader.go_to_place(place);
        returning.then_some(place)
    }

    /// The marks a to z of a document, from the history.
    fn load_marks(&self, path: &Path, notice: &mut Notice) -> HashMap<char, Place> {
        match self.history.as_ref().map(|history| history.marks(path)) {
            Some(Ok(marks)) => marks
                .into_iter()
                .filter(|(name, _)| name.is_ascii_lowercase())
                .collect(),
            Some(Err(error)) => {
                *notice = Notice::Error(error.to_string());
                HashMap::new()
            }
            None => HashMap::new(),
        }
    }

    /// Goes back through the places jumped from, or forward again, a number
    /// of times.
    fn step_jumps(&mut self, back: bool, times: u32) {
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return;
        };
        let here = document.reader.place();
        let place = if back {
            document.jumps.back(here, times)
        } else {
            document.jumps.forward(times)
        };
        match place {
            Some(place) => document.reader.go_to_place(place),
            None if back => self.notice = Notice::Error("no earlier jump".to_owned()),
            None => self.notice = Notice::Error("no later jump".to_owned()),
        }
    }

    /// Takes the letter after `m`, or after `` ` ``. Any key that is not a
    /// character gives up.
    fn on_mark(&mut self, verb: MarkVerb, key: Key) -> Task<Message> {
        let Key::Char(name) = key else {
            return Task::none();
        };
        match verb {
            MarkVerb::Set => {
                self.set_mark(name);
                Task::none()
            }
            MarkVerb::Go => self.go_to_mark(name),
        }
    }

    /// Marks the place in view with a letter: a to z for this document, A
    /// to Z for any.
    fn set_mark(&mut self, name: char) {
        if !name.is_ascii_alphabetic() {
            self.notice = Notice::Error(format!(
                "not a mark: {name}; a to z mark this document, A to Z any"
            ));
            return;
        }
        let Some(document) = self.document_mut() else {
            return;
        };
        let place = document.reader.place();
        let path = document.path.clone();
        if name.is_ascii_lowercase() {
            document.marks.insert(name, place);
        } else {
            self.global_marks.insert(name, (path.clone(), place));
        }
        let saved = self
            .history
            .as_ref()
            .map(|history| history.set_mark(&path, name, place));
        self.notice = match saved {
            Some(Err(error)) => Notice::Error(error.to_string()),
            _ => Notice::Info(format!(
                "mark {name} set on page {}",
                place.page.saturating_add(1)
            )),
        };
    }

    /// Goes to a mark: in this document, in whichever has it for A to Z,
    /// or, for `` ` `` and `'`, where the last jump left from.
    fn go_to_mark(&mut self, name: char) -> Task<Message> {
        if name.is_ascii_uppercase() {
            return self.go_to_global_mark(name);
        }
        let Some(document) = self.document_mut() else {
            return Task::none();
        };
        let back = matches!(name, '`' | '\'');
        let place = if back {
            document.jumps.before()
        } else {
            document.marks.get(&name).copied()
        };
        match place {
            Some(place) => {
                document.jumps.record(document.reader.place());
                document.reader.go_to_place(place);
            }
            None if back => self.notice = Notice::Error("no jump to go back to".to_owned()),
            None => self.notice = Notice::Error(format!("mark {name} is not set")),
        }
        Task::none()
    }

    /// Goes to a mark A to Z, opening its document if need be.
    fn go_to_global_mark(&mut self, name: char) -> Task<Message> {
        let mark = match self.global_marks.get(&name) {
            Some(mark) => Some(mark.clone()),
            None => match self
                .history
                .as_ref()
                .map(|history| history.global_mark(name))
            {
                Some(Ok(mark)) => mark,
                Some(Err(error)) => {
                    self.notice = Notice::Error(error.to_string());
                    return Task::none();
                }
                None => None,
            },
        };
        let Some((path, place)) = mark else {
            self.notice = Notice::Error(format!("mark {name} is not set"));
            return Task::none();
        };
        if let Some(document) = self.document_mut() {
            document.jumps.record(document.reader.place());
        }
        let task = self.open_in_tab(path.clone());
        // Open already, it is in view now; otherwise it goes there once
        // it has opened.
        match self.document_mut() {
            Some(document) if document.path == path => document.reader.go_to_place(place),
            _ => self.arriving = Some((path, place)),
        }
        task
    }

    /// Says which marks are set: this document's, and A to Z.
    fn list_marks(&mut self) {
        let mut listed: Vec<String> = Vec::new();
        if let Some(document) = self.document() {
            let mut marks: Vec<_> = document.marks.iter().collect();
            marks.sort_by_key(|(name, _)| **name);
            listed.extend(
                marks
                    .into_iter()
                    .map(|(name, place)| format!("{name} p{}", place.page.saturating_add(1))),
            );
        }
        let mut global: Vec<(char, PathBuf)> = self
            .global_marks
            .iter()
            .map(|(name, (path, _))| (*name, path.clone()))
            .collect();
        if let Some(Ok(saved)) = self.history.as_ref().map(History::global_marks) {
            for (name, path) in saved {
                if !global.iter().any(|(known, _)| *known == name) {
                    global.push((name, path));
                }
            }
        }
        global.sort();
        listed.extend(global.into_iter().map(|(name, path)| {
            let file = path
                .file_name()
                .map_or_else(|| tilde(&path), |file| file.to_string_lossy().into_owned());
            format!("{name} {file}")
        }));
        self.notice = Notice::Info(if listed.is_empty() {
            "no marks".to_owned()
        } else {
            format!("marks: {}", listed.join("  "))
        });
    }

    /// Takes marks away: these letters, or all of this document's a to z.
    fn delete_marks(&mut self, names: Option<Vec<char>>) {
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return;
        };
        let names = names.unwrap_or_else(|| document.marks.keys().copied().collect());
        let path = document.path.clone();
        for name in &names {
            document.marks.remove(name);
            self.global_marks.remove(name);
        }
        if let Some(Err(error)) = self
            .history
            .as_ref()
            .map(|history| history.delete_marks(&path, &names))
        {
            self.notice = Notice::Error(error.to_string());
        }
    }

    /// Moves through the table of contents, or goes to a chapter; or
    /// through the markings.
    fn on_sidebar_key(&mut self, key: Key) -> Task<Message> {
        let rows = self.sidebar_rows();
        let sidebar = match &mut self.sidebar {
            Some(Sidebar::Outline(sidebar)) => sidebar,
            Some(Sidebar::Markings(list)) => {
                let outcome = list.feed(key, &self.keymaps.markings, rows);
                return self.on_marking_list(outcome);
            }
            None => return Task::none(),
        };
        match sidebar.feed(key, &self.keymaps.outline, rows) {
            outline::Outcome::Stay => {}
            outline::Outcome::Close => self.sidebar = None,
            outline::Outcome::Go(page, top) => {
                self.sidebar = None;
                if let Some(document) = self.document_mut() {
                    document.jumps.record(document.reader.place());
                    document.reader.go_to_spot(page, top);
                }
            }
        }
        Task::none()
    }

    /// How many entries of the table of contents fit in the window.
    fn sidebar_rows(&self) -> usize {
        // Room is kept for the heading and the line at the foot.
        let rows = num::round(((self.area.height - 16.0) / HELP_ROW).floor() - 2.0);
        usize::try_from(rows).unwrap_or(1).max(1)
    }

    /// Shows the list of recent documents, the cheat sheet, or what the
    /// document says about itself.
    fn show_panel(&mut self, action: Action) {
        match action {
            Action::ShowRecent => self.show_recent(),
            Action::ShowHelp => self.show_help(),
            Action::ShowInfo => self.show_info(),
            _ => {}
        }
    }

    /// Shows the cheat sheet for the bindings in force.
    fn show_help(&mut self) {
        self.pending = Pending::new();
        self.picker = None;
        self.help = Some(Help::new(&self.keymaps));
    }

    /// How many lines of the cheat sheet fit in the window.
    fn help_rows(&self) -> usize {
        let rows = num::round(((self.area.height - HELP_CHROME) / HELP_ROW).floor());
        usize::try_from(rows).unwrap_or(1).max(1)
    }

    /// Shows the list of recent documents, or says there are none.
    fn show_recent(&mut self) {
        self.pending = Pending::new();
        self.picker = Picker::new(self.recent_documents());
        if self.picker.is_none() && matches!(self.notice, Notice::None) {
            self.notice = Notice::Info("no recent documents".to_owned());
        }
    }

    /// The documents read lately that still exist, newest first, leaving
    /// out the one open now.
    fn recent_documents(&mut self) -> Vec<PathBuf> {
        let Some(history) = &self.history else {
            return Vec::new();
        };
        let paths = match history.recent() {
            Ok(paths) => paths,
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                return Vec::new();
            }
        };
        let open = self.document().map(|document| &document.path);
        paths
            .into_iter()
            .filter(|path| Some(path) != open && path.is_file())
            .take(recent::SHOWN)
            .collect()
    }

    /// Edits the prompt, or runs it once enter is pressed.
    fn on_prompt_key(&mut self, key: Key) -> Task<Message> {
        let Some((prompt, mut line)) = self.prompt.take() else {
            return Task::none();
        };
        let completing = self.completing.take();
        let before = line.clone();
        match self
            .prompt_pending
            .feed(key, &self.keymaps.prompt, Counts::NotTaken)
        {
            Fed::Bound(
                binding @ (PromptBinding::Complete | PromptBinding::CompletePrevious),
                _,
            ) if prompt == Prompt::Command => {
                let forward = binding == PromptBinding::Complete;
                self.completing = complete_line(&mut line, completing, forward);
                self.prompt = Some((prompt, line));
                return Task::none();
            }
            Fed::Bound(PromptBinding::Submit, _) => return self.submit(prompt, &line),
            Fed::Bound(PromptBinding::Cancel, _) => return Task::none(),
            Fed::Bound(PromptBinding::RubOut, _) => {
                // Rubbing out the sigil itself leaves the prompt, as in vi.
                if line.pop().is_none() {
                    return Task::none();
                }
            }
            Fed::Bound(PromptBinding::ClearLine, _) => line.clear(),
            Fed::Bound(PromptBinding::RubOutWord, _) => {
                let kept = line.trim_end().rfind(char::is_whitespace);
                line.truncate(kept.map_or(0, |index| index.saturating_add(1)));
            }
            // Keys bound to nothing type themselves.
            Fed::Unbound(keys) => {
                for key in keys {
                    match key {
                        Key::Char(character) => line.push(character),
                        Key::ShiftSpace => line.push(' '),
                        _ => {}
                    }
                }
            }
            Fed::Bound(PromptBinding::Complete | PromptBinding::CompletePrevious, _)
            | Fed::Waiting => {}
        }
        // A list once shown follows what is typed, until nothing fits.
        self.completing = if line == before {
            completing
        } else {
            completing.and_then(|_| narrowed(&line))
        };
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
            Command::Quit => return self.close_tab(),
            Command::QuitAll => return self.quit(),
            Command::ForceQuit => return self.close_tab_now(),
            Command::ForceQuitAll => return self.exit(),
            Command::Write { target, force } => return self.write(target, force),
            Command::WriteQuit => return self.write_and_close(),
            Command::WriteAll => return self.write_all(AfterSave::Nothing),
            Command::WriteQuitAll => return self.write_all(AfterSave::Quit),
            Command::Open(path) | Command::TabOpen(Some(path)) => return self.open_in_tab(path),
            Command::Recent => self.show_recent(),
            Command::Marks => self.list_marks(),
            Command::DeleteMarks(names) => self.delete_marks(names),
            Command::Version => self.notice = Notice::Info(crate::ABOUT.to_owned()),
            Command::Info => self.show_info(),
            Command::ShowTheme => {
                let theme = self.current_theme().name();
                self.notice = Notice::Info(if self.theme.is_some() {
                    format!("theme {theme}")
                } else {
                    format!("theme {theme}, following the system (auto)")
                });
            }
            Command::Help => self.show_help(),
            Command::TabOpen(None) => {
                self.tabs.push(Tab::default());
                self.go_to_tab(self.tabs.len().saturating_sub(1));
                self.show_recent();
            }
            Command::TabClose => {
                if self.tabs.len() > 1 {
                    return self.close_tab();
                }
                self.notice = Notice::Error("cannot close the last tab".to_owned());
            }
            Command::TabNext(Some(tab)) => self.go_to_tab(index(tab)),
            Command::TabNext(None) => self.step_tab(1),
            Command::TabPrevious => self.step_tab(-1),
            Command::Goto(page) => {
                if let Some(document) = self.document_mut() {
                    leap(document, page.saturating_sub(1));
                }
            }
            Command::GotoLabel(label) => self.go_to_label(&label),
            Command::NoHighlight => {
                if let Some(document) = self.document_mut() {
                    document.results.shown = false;
                }
            }
            Command::Map(keys, bound) => self.keymaps.bind(keys, bound),
            Command::Unmap(mode, keys) => {
                if !self.keymaps.unbind(mode, &keys) {
                    self.notice = Notice::Error(format!(
                        "no such mapping in {} mode: {}",
                        mode.name(),
                        keymap::written(&keys)
                    ));
                }
            }
            Command::MapClear(mode) => self.keymaps.clear(mode),
            Command::Run(binding, count) => return self.act(binding.action(count)),
            // With no document in view, these set what documents not seen
            // before start with.
            Command::Set(setting) => match setting {
                Setting::Recolor(Recolor::Auto) => {
                    self.recolor_auto = true;
                    self.follow_system();
                }
                Setting::Recolor(recolor) => {
                    self.recolor_auto = false;
                    let recolor = recolor == Recolor::On;
                    match self.document_mut() {
                        Some(document) => document.recolor = recolor,
                        None => self.fresh.recolor = recolor,
                    }
                }
                Setting::Trim(trim) => {
                    if self.document().is_some() {
                        return self.set_trim(self.current, trim);
                    }
                    self.fresh.trim = trim;
                }
                Setting::RecolorPaper(paper) => self.paper = paper,
                Setting::RecolorInk(ink) => self.ink = ink,
                Setting::Theme(theme) => self.theme = theme,
                Setting::ThemeDark(theme) => self.theme_dark = theme,
                Setting::ThemeLight(theme) => self.theme_light = theme,
                Setting::HighlightColour(colour) => self.markup_colours.highlight = colour,
                Setting::UnderlineColour(colour) => self.markup_colours.underline = colour,
                Setting::StrikeOutColour(colour) => self.markup_colours.strike_out = colour,
                Setting::SquigglyColour(colour) => self.markup_colours.squiggly = colour,
                Setting::ScrollStep(step) => self.scroll_step = step,
                Setting::ConfirmQuit(confirm) => self.care.confirm_quit = confirm,
                Setting::Autosave(autosave) => self.care.autosave = autosave,
                Setting::ConfirmTabClose(confirm) => self.care.confirm_tab_close = confirm,
            },
        }
        Task::none()
    }

    /// Carries out an action from the keyboard.
    fn act(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Quit => return self.close_tab(),
            Action::NextTab => {
                self.step_tab(1);
                return Task::none();
            }
            Action::PreviousTab(count) => {
                let back = i64::from(count).saturating_neg();
                self.step_tab(back);
                return Task::none();
            }
            Action::GotoTab(tab) => {
                self.go_to_tab(index(tab));
                return Task::none();
            }
            Action::OpenPrompt(prompt) => {
                self.prompt = Some((prompt, String::new()));
                self.prompt_pending = Pending::new();
                return Task::none();
            }
            Action::ShowRecent | Action::ShowHelp | Action::ShowInfo => {
                self.show_panel(action);
                return Task::none();
            }
            Action::FollowLink => return self.label_links(),
            Action::Undo => return self.undo(),
            Action::Redo => return self.redo(),
            Action::Marking(what) => return self.on_marking_action(what),
            Action::Visual { lines } => {
                let kind = if lines { Kind::Lines } else { Kind::Characters };
                return self.label_lines(kind);
            }
            Action::ShowOutline => return self.show_outline(),
            Action::Mark(verb) => {
                self.awaiting_mark = Some(verb);
                return Task::none();
            }
            Action::JumpBack(times) | Action::JumpForward(times) => {
                self.step_jumps(matches!(action, Action::JumpBack(_)), times);
                return Task::none();
            }
            Action::SearchNext => return self.step_search(true),
            Action::SearchPrevious => return self.step_search(false),
            Action::ToggleTrim => {
                let trim = self.document().is_some_and(|document| !document.trim);
                return self.set_trim(self.current, trim);
            }
            _ => {}
        }
        let step = self.scroll_step;
        let Some(document) = self.document_mut() else {
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
            Action::GotoPage(page) => leap(document, page.saturating_sub(1)),
            Action::GotoFirst => leap(document, 0),
            Action::GotoLast => leap(document, u32::MAX),
            Action::PageTop => reader.page_top(),
            Action::PageBottom => reader.page_bottom(),
            Action::ZoomIn => reader.zoom_in(),
            Action::ZoomOut => reader.zoom_out(),
            Action::ZoomTo(percent) => reader.zoom_to(num::float(percent) / 100.0),
            Action::FitWidth => reader.fit(Fit::Width),
            Action::FitPage => reader.fit(Fit::Page),
            Action::Rotate => reader.rotate(),
            Action::ToggleRecolor => document.recolor = !document.recolor,
            Action::Abort => hide_marks(document),
            Action::Quit
            | Action::OpenPrompt(_)
            | Action::ShowRecent
            | Action::ShowHelp
            | Action::ShowInfo
            | Action::ShowOutline
            | Action::Mark(_)
            | Action::JumpBack(_)
            | Action::JumpForward(_)
            | Action::FollowLink
            | Action::Visual { .. }
            | Action::Undo
            | Action::Redo
            | Action::Marking(_)
            | Action::NextTab
            | Action::PreviousTab(_)
            | Action::GotoTab(_)
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
        let Some(document) = self.document_mut() else {
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

    /// Starts opening a document in a new tab, or in the tab in view if
    /// that is empty. A document already open, or on its way, is brought
    /// into view instead.
    fn open_in_tab(&mut self, path: PathBuf) -> Task<Message> {
        // The history is keyed by the real path, so that a file reached by
        // two routes is one document.
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if let Some(index) = self.tabs.iter().position(|tab| tab.holds(&path)) {
            self.go_to_tab(index);
            return Task::none();
        }
        if !self.tabs.get(self.current).is_some_and(Tab::is_empty) {
            self.tabs.push(Tab::default());
            self.go_to_tab(self.tabs.len().saturating_sub(1));
        }
        let request = self.next_id();
        let Some(tab) = self.tabs.get_mut(self.current) else {
            return Task::none();
        };
        tab.opening = Some((request, path.clone()));
        self.notice = Notice::Info(format!("opening {}", tilde(&path)));
        Task::perform(backend::open(path.clone()), move |result| {
            Message::Opened(request, path, result)
        })
    }

    /// Puts a document that finished opening in the tab that asked for it.
    fn on_opened(
        &mut self,
        request: u64,
        path: PathBuf,
        result: Result<Opened, backend::Error>,
    ) -> Task<Message> {
        let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.opening.as_ref().is_some_and(|(id, _)| *id == request))
        else {
            return Task::none();
        };
        let opened = match result {
            Ok(opened) => opened,
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                if let Some(tab) = self.tabs.get_mut(index) {
                    tab.opening = None;
                }
                // A tab opened for this document alone has no use now.
                if self.tabs.len() > 1 && self.tabs.get(index).is_some_and(Tab::is_empty) {
                    self.remove_tab(index);
                }
                return Task::none();
            }
        };
        let id = self.next_id();
        let mut reader = Reader::new(opened.sizes, (self.area.width, self.area.height));
        let Fresh {
            mut recolor,
            mut trim,
        } = self.fresh;
        let mut returning = None;
        let mut notice = Notice::None;
        match self.history.as_ref().map(|history| history.load(&path)) {
            Some(Ok(Some(view))) => {
                reader.restore(view);
                recolor = view.recolor;
                trim = view.trim;
                // A trimmed view only makes sense once the margins are
                // known, so it is restored again when they are.
                returning = view.trim.then_some(view);
            }
            Some(Err(error)) => notice = Notice::Error(error.to_string()),
            Some(Ok(None)) | None => {}
        }
        let marks = self.load_marks(&path, &mut notice);
        // Opened to go to a mark: there, rather than where it was left.
        let arriving = self.arrive(&path, &mut reader, returning.is_some());
        // Following the system, its theme wins over how the document was
        // left.
        if self.recolor_auto
            && let Some(dark) = self.system_dark
        {
            recolor = dark;
        }
        let Some(tab) = self.tabs.get_mut(index) else {
            return Task::none();
        };
        tab.opening = None;
        if index == self.current {
            // The selection and labels were for the document leaving.
            self.visual = None;
            self.line_hints = None;
        }
        let Some(tab) = self.tabs.get_mut(index) else {
            return Task::none();
        };
        let left = tab.document.replace(Document {
            id,
            path,
            backend: opened.backend,
            info: opened.info,
            labels: opened.labels,
            reader,
            recolor,
            trim: false,
            contents: None,
            measuring: false,
            returning,
            edition: 0,
            undo: Vec::new(),
            markings: None,
            marking: None,
            redo: Vec::new(),
            pages: HashMap::new(),
            rendering: HashMap::new(),
            failed: HashMap::new(),
            results: Results::default(),
            chapters: None,
            links: HashMap::new(),
            text: HashMap::new(),
            reading: std::collections::HashSet::new(),
            jumps: Jumps::default(),
            saved: Some(0),
            marks,
            arriving,
        });
        // Only the notice that the document was opening is done with.
        if matches!(notice, Notice::Error(_)) {
            self.notice = notice;
        } else if matches!(self.notice, Notice::Info(_)) {
            self.notice = Notice::None;
        }
        if let Some(left) = &left {
            self.remember(left);
        }
        if trim {
            self.set_trim(index, true)
        } else {
            Task::none()
        }
    }

    /// Hides or shows the blank margins of every page. The margins are
    /// measured the first time they are hidden, which takes a moment.
    fn set_trim(&mut self, tab: usize, trim: bool) -> Task<Message> {
        let Some(document) = self.tabs.get_mut(tab).and_then(|tab| tab.document.as_mut()) else {
            return Task::none();
        };
        let id = document.id;
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
            Message::Measured(id, result)
        })
    }

    fn on_measured(&mut self, id: u64, result: Result<Vec<Rect>, backend::Error>) {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
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
                    if let Some(place) = document.arriving.take() {
                        document.reader.go_to_place(place);
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
        id: u64,
        page: u32,
        key: RenderKey,
        result: Result<image::Handle, backend::Error>,
    ) {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
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
        let palette = self.palette();
        let scale_factor = self.scale_factor;
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return Task::none();
        };
        let id = document.id;
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
            edition: document.edition,
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
                Message::Rendered(id, page, wanted, handle)
            }));
        }
        Task::batch(tasks)
    }

    /// Starts a search. An empty needle repeats the last one.
    fn search(&mut self, needle: &str, forward: bool) -> Task<Message> {
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return Task::none();
        };
        let id = document.id;
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
        Task::perform(finding, move |result| Message::Found(id, search, result))
    }

    fn on_found(&mut self, id: u64, search: Search, result: Result<Vec<Hit>, backend::Error>) {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
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
        jump_to(document, current, &mut self.notice);
    }

    /// Moves to the next hit in the direction of the last search, or
    /// against it.
    fn step_search(&mut self, same_direction: bool) -> Task<Message> {
        let Some(document) = current_document(&mut self.tabs, self.current) else {
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
        jump_to(document, Some(next), &mut self.notice);
        Task::none()
    }

    /// The document in the tab in view.
    fn document(&self) -> Option<&Document> {
        self.tabs
            .get(self.current)
            .and_then(|tab| tab.document.as_ref())
    }

    fn document_mut(&mut self) -> Option<&mut Document> {
        current_document(&mut self.tabs, self.current)
    }

    /// A number not given to any document or request before.
    const fn next_id(&mut self) -> u64 {
        self.last_id = self.last_id.saturating_add(1);
        self.last_id
    }

    /// Brings a tab into view. A tab past the last brings the last.
    fn go_to_tab(&mut self, index: usize) {
        self.current = index.min(self.tabs.len().saturating_sub(1));
    }

    /// Moves a number of tabs forward, or back when negative, going round
    /// from the last to the first and the other way.
    fn step_tab(&mut self, by: i64) {
        let count = i64::try_from(self.tabs.len()).unwrap_or(i64::MAX);
        let current = i64::try_from(self.current).unwrap_or(0);
        let next = current
            .saturating_add(by)
            .checked_rem_euclid(count)
            .unwrap_or(0);
        self.go_to_tab(usize::try_from(next).unwrap_or(0));
    }

    /// Closes the tab in view, and the program with its last tab.
    /// Closes the tab in view, and the program with its last tab, asking
    /// first when that is wanted. Closing the last tab is quitting, so only
    /// `confirm-quit` asks about it.
    fn close_tab(&mut self) -> Task<Message> {
        if self.tabs.len() <= 1 {
            return self.quit();
        }
        if self.document().is_some_and(Document::modified) {
            self.asking = Some(Question::DiscardTab);
            self.pending = Pending::new();
            return Task::none();
        }
        if self.care.confirm_tab_close {
            self.asking = Some(Question::CloseTab);
            self.pending = Pending::new();
            return Task::none();
        }
        self.close_tab_now()
    }

    /// Closes the tab in view without asking, and the program with its
    /// last tab.
    fn close_tab_now(&mut self) -> Task<Message> {
        if self.tabs.len() <= 1 {
            return self.exit();
        }
        if let Some(document) = self
            .tabs
            .get_mut(self.current)
            .and_then(|tab| tab.document.take())
        {
            self.remember(&document);
        }
        self.remove_tab(self.current);
        Task::none()
    }

    /// Takes a tab away. The tab after it comes into view in its place, or
    /// the one before when it was the last.
    fn remove_tab(&mut self, index: usize) {
        if index >= self.tabs.len() || self.tabs.len() <= 1 {
            return;
        }
        self.tabs.remove(index);
        if self.current > index {
            self.current = self.current.saturating_sub(1);
        }
        self.go_to_tab(self.current);
    }

    /// Records where a document is being left.
    fn remember(&mut self, document: &Document) {
        let Some(history) = &self.history else {
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

    /// Quits, or asks first when that is wanted.
    fn quit(&mut self) -> Task<Message> {
        if self.unsaved().next().is_some() {
            self.asking = Some(Question::DiscardAll);
            self.pending = Pending::new();
            return Task::none();
        }
        if self.care.confirm_quit {
            self.asking = Some(Question::Quit);
            self.pending = Pending::new();
            return Task::none();
        }
        self.exit()
    }

    /// What the bottom line asks.
    fn question(&self, question: Question) -> String {
        let yes = self.keymaps.confirm.first(ConfirmBinding::Yes);
        let name = self
            .tabs
            .get(self.current)
            .map(Tab::name)
            .unwrap_or_default();
        match question {
            Question::Quit => format!("quit pdvim? ({yes}/n)"),
            Question::CloseTab => format!("close tab {name}? ({yes}/n)"),
            Question::DiscardTab => {
                format!("{name} has unsaved highlights: close it anyway? ({yes}/n)")
            }
            Question::DiscardAll => {
                let unsaved: Vec<_> = self.unsaved().collect();
                let what = match unsaved.as_slice() {
                    [one] => one
                        .path
                        .file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                    many => format!("{} documents", many.len()),
                };
                let has = if unsaved.len() == 1 { "has" } else { "have" };
                format!("{what} {has} unsaved highlights: quit anyway? ({yes}/n)")
            }
        }
    }

    /// The documents with changes not saved.
    fn unsaved(&self) -> impl Iterator<Item = &Document> {
        self.tabs
            .iter()
            .filter_map(|tab| tab.document.as_ref())
            .filter(|document| document.modified())
    }

    /// Records where every document is being left, and closes the program.
    fn exit(&mut self) -> Task<Message> {
        let documents: Vec<_> = self
            .tabs
            .iter_mut()
            .filter_map(|tab| tab.document.take())
            .collect();
        for document in &documents {
            self.remember(document);
        }
        iced::exit()
    }

    /// The theme in force: the one set, or the one for the system's
    /// theme, dark unless it is known to be light.
    fn current_theme(&self) -> theme::Theme {
        let followed = if self.system_dark == Some(false) {
            self.theme_light
        } else {
            self.theme_dark
        };
        self.theme.unwrap_or(followed)
    }

    /// The colours around the pages.
    fn chrome(&self) -> Colours {
        *self.current_theme().colours()
    }

    /// The iced theme, for what is not given colours of its own.
    fn theme(&self) -> Theme {
        self.current_theme().iced()
    }

    /// What recoloured pages are drawn in: the colours set, or the
    /// theme's.
    fn palette(&self) -> Palette {
        let themed = self.current_theme().colours().recolor;
        Palette {
            paper: self.paper.unwrap_or(themed.paper),
            ink: self.ink.unwrap_or(themed.ink),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let chrome = self.chrome();
        let pages = Canvas::new(Pages(self)).width(Fill).height(Fill);
        let marks = Canvas::new(Marks(self)).width(Fill).height(Fill);
        let mut layers = stack![pages, marks];
        if let Some(picker) = &self.picker {
            layers = layers.push(list(picker, chrome));
        }
        if let Some(completing) = &self.completing {
            layers = layers.push(completions(completing, chrome));
        }
        match &self.sidebar {
            Some(Sidebar::Outline(sidebar)) => {
                layers = layers.push(contents(
                    sidebar,
                    &self.keymaps,
                    chrome,
                    self.sidebar_rows(),
                ));
            }
            Some(Sidebar::Markings(list)) => {
                layers = layers.push(markings::list(
                    list,
                    &self.keymaps,
                    chrome,
                    self.sidebar_rows(),
                ));
            }
            None => {}
        }
        if let Some((name, facts)) = &self.info {
            layers = layers.push(info::panel(name, facts, chrome));
        }
        if let Some(help) = &self.help {
            layers = layers.push(cheat_sheet(help, &self.keymaps, chrome, self.help_rows()));
        }
        match self.tab_bar() {
            Some(tabs) => column![tabs, layers, self.bar()].into(),
            None => column![layers, self.bar()].into(),
        }
    }

    /// The line at the top naming each tab, when there is more than one.
    fn tab_bar(&self) -> Option<Element<'_, Message>> {
        let chrome = self.chrome();
        if self.tabs.len() < 2 {
            return None;
        }
        let tabs = self.tabs.iter().enumerate().map(|(index, tab)| {
            let current = index == self.current;
            let unsaved = if tab.document.as_ref().is_some_and(Document::modified) {
                " [+]"
            } else {
                ""
            };
            let name = format!(
                "{} {}{unsaved}",
                index.saturating_add(1),
                info::tab_name(&tab.name())
            );
            let label = text(name)
                .size(BAR_TEXT_SIZE)
                .font(Font::MONOSPACE)
                .color(if current {
                    chrome.bar_text
                } else {
                    chrome.tab_text
                })
                .wrapping(text::Wrapping::None);
            container(label)
                .padding([3, 8])
                .width(Fill)
                .clip(true)
                .style(move |_| {
                    container::background(if current {
                        chrome.tab_current
                    } else {
                        chrome.tabs
                    })
                })
                .into()
        });
        Some(
            container(Row::with_children(tabs).spacing(1))
                .width(Fill)
                .style(move |_| container::background(chrome.bar))
                .into(),
        )
    }

    /// The line at the bottom: the prompt while typing, otherwise a notice
    /// or the file name on the left and the position on the right.
    fn bar(&self) -> Element<'_, Message> {
        let chrome = self.chrome();
        let label = |content: String, colour: Color| {
            text(content)
                .size(BAR_TEXT_SIZE)
                .font(Font::MONOSPACE)
                .color(colour)
                .wrapping(text::Wrapping::None)
        };
        let left = match (&self.prompt, &self.notice) {
            _ if let Some(question) = self.asking => {
                label(self.question(question), chrome.bar_error)
            }
            _ if self.visual.is_some() => {
                let visual = self.visual.as_ref();
                let kind = match visual.map(|visual| visual.kind) {
                    Some(Kind::Lines) => "-- VISUAL LINE --",
                    _ => "-- VISUAL --",
                };
                let typed = visual.map(Visual::typed).unwrap_or_default();
                let typed = if typed.is_empty() {
                    typed
                } else {
                    format!("  {typed}")
                };
                let keymap = &self.keymaps.visual;
                let yank = keymap.first(visual::Binding::Yank);
                let leave = keymap.first(visual::Binding::Leave);
                label(
                    format!("{kind}{typed}    {yank} copies, {leave} leaves"),
                    chrome.bar_text,
                )
            }
            _ if self.line_hints.is_some() => {
                let typed = self
                    .line_hints
                    .as_ref()
                    .and_then(|(_, hints)| hints.typed())
                    .map(String::from)
                    .unwrap_or_default();
                label(
                    format!("select from line: {typed}\u{2588}"),
                    chrome.bar_text,
                )
            }
            _ if self.hints.is_some() => {
                let typed = self
                    .hints
                    .as_ref()
                    .and_then(Hints::typed)
                    .map(String::from)
                    .unwrap_or_default();
                label(format!("follow link: {typed}\u{2588}"), chrome.bar_text)
            }
            (Some((prompt, line)), _) => {
                label(format!("{}{line}\u{2588}", prompt.sigil()), chrome.bar_text)
            }
            (None, Notice::Error(message)) => label(message.clone(), chrome.bar_error),
            (None, Notice::Info(message)) => label(message.clone(), chrome.bar_text),
            (None, Notice::None) => label(
                self.document()
                    .map(|document| {
                        let unsaved = if document.modified() { " [+]" } else { "" };
                        format!("{}{unsaved}", tilde(&document.path))
                    })
                    .unwrap_or_default(),
                chrome.bar_text,
            ),
        };
        let position = self.document().map_or_else(String::new, |document| {
            let reader = &document.reader;
            format!(
                "{}  {}%",
                document.page_position(),
                num::round(reader.zoom() * 100.0),
            )
        });
        let typed = match self.awaiting_mark {
            Some(MarkVerb::Set) => self.keymaps.normal.first(input::Binding::SetMark),
            Some(MarkVerb::Go) => self.keymaps.normal.first(input::Binding::GoToMark),
            None => self.pending.display(),
        };
        let right = label(format!("{typed}  {position}"), chrome.bar_text);
        // A long notice is cut short rather than pushing the position away.
        let left = container(left).width(Fill).clip(true);
        container(row![left, right].spacing(16))
            .padding([3, 8])
            .width(Fill)
            .style(move |_| container::background(chrome.bar))
            .into()
    }
}

/// Completes the word at the end of a command line. The first press
/// completes as far as every candidate agrees, and lists them if there are
/// several; the presses after go through them in turn. Returns what is
/// listed, if anything.
fn complete_line(
    line: &mut String,
    completing: Option<Completing>,
    forward: bool,
) -> Option<Completing> {
    if let Some(mut completing) = completing.filter(|completing| !completing.typed) {
        let count = completing.candidates.len();
        let last = count.saturating_sub(1);
        let next = match (completing.selected, forward) {
            (None, true) => 0,
            (None, false) => last,
            (Some(index), true) => index.saturating_add(1).checked_rem(count).unwrap_or(0),
            (Some(index), false) => index.checked_sub(1).unwrap_or(last),
        };
        completing.selected = Some(next);
        if let Some(candidate) = completing.candidates.get(next) {
            line.truncate(completing.start);
            line.push_str(&candidate.text);
        }
        return Some(completing);
    }
    let complete::Completion { start, candidates } = complete::complete(line);
    let typed = line.len().saturating_sub(start);
    match candidates.as_slice() {
        [] => None,
        [only] => {
            line.truncate(start);
            line.push_str(&only.text);
            if only.whole {
                line.push(' ');
            }
            None
        }
        _ => {
            let shared = complete::common_start(&candidates);
            if shared.len() > typed {
                let shared = shared.to_owned();
                line.truncate(start);
                line.push_str(&shared);
            }
            Some(Completing {
                start,
                candidates,
                selected: None,
                typed: false,
            })
        }
    }
}

/// The list for a line that was typed into while a list was shown, or
/// none when nothing fits it.
fn narrowed(line: &str) -> Option<Completing> {
    let complete::Completion { start, candidates } = complete::complete(line);
    (!candidates.is_empty()).then_some(Completing {
        start,
        candidates,
        selected: None,
        typed: true,
    })
}

/// The completions on offer, in a band over the bottom of the pages. A
/// long list is shown a part at a time, the part with the selected one.
fn completions(completing: &Completing, chrome: Colours) -> Element<'_, Message> {
    let selected = completing.selected;
    let first = selected.map_or(0, |index| {
        index
            .checked_div(COMPLETIONS_SHOWN)
            .unwrap_or(0)
            .saturating_mul(COMPLETIONS_SHOWN)
    });
    let shown = completing
        .candidates
        .iter()
        .enumerate()
        .skip(first)
        .take(COMPLETIONS_SHOWN)
        .map(|(index, candidate)| {
            let current = selected == Some(index);
            container(
                text(candidate.label.as_str())
                    .size(BAR_TEXT_SIZE)
                    .font(Font::MONOSPACE)
                    .color(if current {
                        chrome.bar_text
                    } else {
                        chrome.tab_text
                    })
                    .wrapping(text::Wrapping::None),
            )
            .padding([1, 4])
            .style(move |_| {
                if current {
                    container::background(chrome.tab_current)
                } else {
                    container::Style::default()
                }
            })
            .into()
        });
    let mut entries = Row::with_children(shown).spacing(8);
    let count = completing.candidates.len();
    if count > COMPLETIONS_SHOWN {
        let place = selected.map_or(0, |index| index.saturating_add(1));
        entries = entries.push(
            text(format!("({place} of {count})"))
                .size(BAR_TEXT_SIZE)
                .font(Font::MONOSPACE)
                .color(chrome.list_number),
        );
    }
    let band = container(entries.wrap().vertical_spacing(2))
        .padding([4, 8])
        .width(Fill)
        .style(move |_| container::background(chrome.bar));
    container(band).align_bottom(Fill).clip(true).into()
}

/// The cheat sheet, drawn over the pages: as many lines as fit, and a
/// line at the foot saying where they are and how to move.
fn cheat_sheet<'a>(
    help: &'a Help,
    keymaps: &Keymaps,
    chrome: Colours,
    rows: usize,
) -> Element<'a, Message> {
    let label = |content: &str, colour: Color| {
        text(content.to_owned())
            .size(LIST_TEXT_SIZE)
            .font(Font::MONOSPACE)
            .color(colour)
            .wrapping(text::Wrapping::None)
    };
    let lines = help.shown(rows).iter().map(|line| {
        let content: Element<'_, Message> = match line {
            Line::Heading(heading) => label(heading, chrome.bar_text)
                .font(Font {
                    weight: iced::font::Weight::Bold,
                    ..Font::MONOSPACE
                })
                .into(),
            Line::Entry {
                keys,
                action,
                description,
                ..
            } => {
                let keys = if keys.is_empty() { "-" } else { keys };
                row![
                    container(label(keys, chrome.bar_text))
                        .width(HELP_KEYS_WIDTH)
                        .clip(true),
                    container(label(action, chrome.list_number))
                        .width(HELP_ACTION_WIDTH)
                        .clip(true),
                    container(label(description, chrome.tab_text))
                        .width(Fill)
                        .clip(true),
                ]
                .spacing(12)
                .into()
            }
            Line::Blank => text("").into(),
        };
        container(content)
            .height(HELP_ROW)
            .width(Fill)
            .align_y(iced::alignment::Vertical::Center)
            .into()
    });
    let (top, count) = help.position();
    let last = top.saturating_add(rows).min(count);
    let hint = {
        use crate::help::Binding as B;
        let key = |binding| keymaps.help.first(binding);
        format!(
            "{}/{}, {}/{}, {}/{} to scroll; {} to close",
            key(B::Down),
            key(B::Up),
            key(B::HalfDown),
            key(B::HalfUp),
            key(B::Top),
            key(B::Bottom),
            key(B::Close),
        )
    };
    let foot = row![
        container(label(&hint, chrome.list_number))
            .width(Fill)
            .clip(true),
        label(
            &format!("{}-{last} of {count}", top.saturating_add(1)),
            chrome.list_number
        ),
    ]
    .spacing(12);
    let body = Column::with_children(lines);
    let panel = container(column![
        body,
        container(foot).padding(iced::Padding::ZERO.top(4))
    ])
    .padding(8)
    .max_width(HELP_WIDTH)
    .style(move |_| {
        container::background(chrome.list).border(iced::Border {
            color: chrome.list_border,
            width: 1.0,
            radius: 4.0.into(),
        })
    });
    container(panel)
        .center_x(Fill)
        .center_y(Fill)
        .padding(24)
        .into()
}

/// The table of contents, in a sidebar over the left of the pages: as many
/// entries as fit, each set in by its depth, the selected one marked.
fn contents<'a>(
    sidebar: &'a Outline,
    keymaps: &Keymaps,
    chrome: Colours,
    rows: usize,
) -> Element<'a, Message> {
    let label = |content: String, colour: Color| {
        text(content)
            .size(LIST_TEXT_SIZE)
            .font(Font::MONOSPACE)
            .color(colour)
            .wrapping(text::Wrapping::None)
    };
    let entries = sidebar.shown(rows).into_iter().map(|shown| {
        let entry = shown.entry;
        let marker = match (entry.has_sections, shown.unfolded) {
            (false, _) => " ",
            (true, false) => "\u{25b8}",
            (true, true) => "\u{25be}",
        };
        let page = entry
            .page
            .map(|page| page.saturating_add(1).to_string())
            .unwrap_or_default();
        let indent = SIDEBAR_INDENT * num::float(u32::try_from(entry.depth).unwrap_or(u32::MAX));
        let line = row![
            iced::widget::Space::new().width(indent),
            label(marker.to_owned(), chrome.list_number),
            container(label(
                entry.title.clone(),
                if shown.selected {
                    chrome.bar_text
                } else {
                    chrome.tab_text
                }
            ))
            .width(Fill)
            .clip(true),
            label(page, chrome.list_number),
        ]
        .spacing(6);
        let selected = shown.selected;
        container(line)
            .height(HELP_ROW)
            .width(Fill)
            .padding([0, 6])
            .align_y(iced::alignment::Vertical::Center)
            .style(move |_| {
                if selected {
                    container::background(chrome.tab_current)
                } else {
                    container::Style::default()
                }
            })
            .into()
    });
    let (top, count) = sidebar.position(rows);
    let last = top.saturating_add(rows).min(count);
    let heading = container(label("Contents".to_owned(), chrome.bar_text).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::MONOSPACE
    }))
    .height(HELP_ROW)
    .padding([0, 6]);
    let foot = container(label(
        format!(
            "{}-{last} of {count}   {}: go   {}/{}: open/close",
            top.saturating_add(1),
            keymaps.outline.first(outline::Binding::Go),
            keymaps.outline.first(outline::Binding::Open),
            keymaps.outline.first(outline::Binding::Fold),
        ),
        chrome.list_number,
    ))
    .height(HELP_ROW)
    .padding([0, 6])
    .clip(true);
    let panel = container(column![
        heading,
        Column::with_children(entries).height(Fill),
        foot
    ])
    .padding(8)
    .width(SIDEBAR_WIDTH)
    .height(Fill)
    .style(move |_| {
        container::background(chrome.list).border(iced::Border {
            color: chrome.list_border,
            width: 1.0,
            radius: 0.0.into(),
        })
    });
    container(panel).width(Fill).height(Fill).into()
}

/// The list of recent documents, drawn over the pages. The first nine are
/// numbered, for the digit that opens each.
fn list(picker: &Picker, chrome: Colours) -> Element<'_, Message> {
    let label = |content: String, colour: Color| {
        text(content)
            .size(LIST_TEXT_SIZE)
            .font(Font::MONOSPACE)
            .color(colour)
            .wrapping(text::Wrapping::None)
    };
    let heading = container(label("Recent documents".to_owned(), chrome.bar_text)).padding([4, 10]);
    let entries = picker.entries().iter().enumerate().map(|(index, path)| {
        let number = index
            .checked_add(1)
            .filter(|number| *number <= 9)
            .map_or_else(|| " ".to_owned(), |number| number.to_string());
        let selected = index == picker.selected();
        let line = row![
            label(number, chrome.list_number),
            container(label(tilde(path), chrome.bar_text))
                .width(Fill)
                .clip(true),
        ]
        .spacing(12);
        container(line)
            .padding([3, 10])
            .width(Fill)
            .style(move |_| {
                if selected {
                    container::background(chrome.list_selected)
                } else {
                    container::Style::default()
                }
            })
            .into()
    });
    let body = Column::with_children(entries).spacing(1);
    let panel = container(column![heading, body].spacing(4))
        .padding(8)
        .max_width(LIST_WIDTH)
        .style(move |_| {
            container::background(chrome.list).border(iced::Border {
                color: chrome.list_border,
                width: 1.0,
                radius: 4.0.into(),
            })
        });
    container(panel)
        .center_x(Fill)
        .center_y(Fill)
        .padding(24)
        .into()
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
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), app.chrome().backdrop);
        let Some(document) = app.document() else {
            return vec![frame.into_geometry()];
        };
        // Until a page is rendered, a blank sheet stands in for it.
        let paper = if document.recolor {
            colour(app.palette().paper)
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
        let Some(document) = self.0.document() else {
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
        markings::draw_current(&mut frame, document);
        if let Some(visual) = &self.0.visual {
            for (page, area) in visual.areas(&document.text) {
                if let Some(area) = document.reader.area_on_screen(page, area) {
                    let area = rectangle(area);
                    frame.fill_rectangle(area.position(), area.size(), SELECTION);
                }
            }
            let cursor = visual.cursor();
            if let Some(area) = visual::character(&document.text, cursor)
                .and_then(|area| document.reader.area_on_screen(cursor.page, area))
            {
                let area = rectangle(area);
                frame.fill_rectangle(area.position(), area.size(), CURSOR);
            }
        }
        if let Some(hints) = &self.0.hints {
            for hint in hints.shown() {
                let Some(area) = document.reader.area_on_screen(hint.page, hint.item.area) else {
                    continue;
                };
                let area = rectangle(area);
                frame.fill_rectangle(area.position(), area.size(), LINK);
                // The label sits on the link's top-left corner.
                draw_label(&mut frame, hint.label, Point::new(area.x, area.y));
            }
        }
        if let Some((_, hints)) = &self.0.line_hints {
            for hint in hints.shown() {
                let Some(area) = visual::character(&document.text, hint.item)
                    .and_then(|area| document.reader.area_on_screen(hint.page, area))
                else {
                    continue;
                };
                // The label sits just left of the line's first character.
                let size = label_size();
                draw_label(
                    &mut frame,
                    hint.label,
                    Point::new(area.left - size.width - 2.0, area.top),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

/// The size of the box a label of two letters is drawn in.
const fn label_size() -> Size {
    Size::new(LABEL_SIZE * 1.4, LABEL_SIZE * 1.35)
}

/// Draws a label of two letters with its top-left corner at a point, kept
/// inside the window.
fn draw_label(frame: &mut canvas::Frame, label: [char; 2], at: Point) {
    let corner = Point::new(at.x.max(0.0), at.y.max(0.0));
    let size = label_size();
    frame.fill_rectangle(corner, size, LABEL);
    frame.fill_text(canvas::Text {
        content: label.iter().collect(),
        position: Point::new(corner.x + size.width / 2.0, corner.y + size.height / 2.0),
        color: LABEL_TEXT,
        size: LABEL_SIZE.into(),
        font: Font {
            weight: iced::font::Weight::Bold,
            ..Font::MONOSPACE
        },
        align_x: iced::widget::text::Alignment::Center,
        align_y: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
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
            Named::Tab if modifiers.control() && modifiers.shift() => Some(Key::CtrlShiftTab),
            Named::Tab if modifiers.control() => Some(Key::CtrlTab),
            Named::Tab if modifiers.shift() => Some(Key::ShiftTab),
            Named::Tab => Some(Key::Tab),
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

/// The document in the tab in view, taking only the tabs, so that other
/// parts of the app stay free to change alongside.
fn current_document(tabs: &mut [Tab], current: usize) -> Option<&mut Document> {
    tabs.get_mut(current).and_then(|tab| tab.document.as_mut())
}

/// The open document with this number, in whichever tab it is.
fn document_by_id(tabs: &mut [Tab], id: u64) -> Option<&mut Document> {
    tabs.iter_mut()
        .filter_map(|tab| tab.document.as_mut())
        .find(|document| document.id == id)
}

/// Makes a hit the current one and brings it into view.
fn jump_to(document: &mut Document, index: Option<usize>, notice: &mut Notice) {
    let Some((index, hit)) =
        index.and_then(|index| Some((index, *document.results.hits.get(index)?)))
    else {
        return;
    };
    document.results.current = Some(index);
    document.jumps.record(document.reader.place());
    document.reader.reveal(hit.page, hit.area);
    if !matches!(notice, Notice::Error(_)) {
        *notice = Notice::Info(format!(
            "match {} of {}",
            index.saturating_add(1),
            document.results.hits.len()
        ));
    }
}

/// Hides the search results and the frame around the marking gone to.
const fn hide_marks(document: &mut Document) {
    document.results.shown = false;
    document.marking = None;
}

/// Goes to the top of a page, noting where it left from in the jump list.
fn leap(document: &mut Document, page: u32) {
    document.jumps.record(document.reader.place());
    document.reader.go_to_page(page);
}

/// What only some systems take: on Linux, the name the window goes by, which
/// the desktop matches with `pdvim.desktop`.
fn platform_specific() -> window::settings::PlatformSpecific {
    #[cfg(target_os = "linux")]
    {
        window::settings::PlatformSpecific {
            application_id: "pdvim".to_owned(),
            ..window::settings::PlatformSpecific::default()
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        window::settings::PlatformSpecific::default()
    }
}

/// Has the desktop's chosen program open a link, such as a web page in the
/// browser, without waiting for it.
fn open_externally(uri: &str) -> std::io::Result<()> {
    // A link comes from the document, which may have been made to harm:
    // only web and mail addresses are handed on, never files or programs.
    if !openable(uri) {
        return Err(std::io::Error::other(
            "only http, https and mailto links are opened",
        ));
    }
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        // Not through `cmd`, which would take `&` and `|` in the address
        // as commands of its own.
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    let mut child = command
        .arg(uri)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    // Collect it once it is done, so it does not linger.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Whether a link's address is one to hand to another program: a web or
/// mail address.
fn openable(uri: &str) -> bool {
    let Some((scheme, _)) = uri.split_once(':') else {
        return false;
    };
    ["http", "https", "mailto"]
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
}

/// The index of a tab counted from 1.
fn index(tab: u32) -> usize {
    usize::try_from(tab.saturating_sub(1)).unwrap_or(usize::MAX)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Presses Tab or Shift-Tab on a line, in order, and returns the line
    /// after each press and whether a list is shown.
    fn presses(start: &str, forward: &[bool]) -> Vec<(String, bool)> {
        let mut line = start.to_owned();
        let mut completing = None;
        forward
            .iter()
            .map(|&forward| {
                completing = complete_line(&mut line, completing.take(), forward);
                (line.clone(), completing.is_some())
            })
            .collect()
    }

    #[test]
    fn only_web_and_mail_links_are_opened() {
        assert!(openable("https://example.com/a?b=c&d"));
        assert!(openable("HTTP://example.com"));
        assert!(openable("mailto:someone@example.com"));
        assert!(!openable("file:///etc/passwd"));
        assert!(!openable("javascript:alert(1)"));
        assert!(!openable("--help"));
        assert!(!openable("/usr/bin/xterm"));
    }

    #[test]
    fn one_candidate_is_put_in_whole() {
        assert_eq!(presses("rec", &[true]), [("recent ".to_owned(), false)]);
        assert_eq!(
            presses("set trim f", &[true]),
            [("set trim false ".to_owned(), false)]
        );
    }

    #[test]
    fn several_are_completed_as_far_as_they_agree_then_gone_through() {
        let lines: Vec<_> = presses("tabn", &[true, true, true, false, false])
            .into_iter()
            .map(|(line, listed)| {
                assert!(listed);
                line
            })
            .collect();
        assert_eq!(lines, ["tabne", "tabnew", "tabnext", "tabnew", "tabnext"]);
    }

    #[test]
    fn going_back_from_the_start_wraps_to_the_end() {
        let (line, _) = presses("tabn", &[true, false]).pop().unwrap();
        assert_eq!(line, "tabnext");
    }

    #[test]
    fn typing_narrows_the_list_and_tab_then_completes_afresh() {
        let mut line = "tab".to_owned();
        let completing = complete_line(&mut line, None, true);
        assert_eq!(line, "tab");
        line.push_str("ne");
        let completing = completing.and_then(|_| narrowed(&line));
        let labels: Vec<_> = completing
            .as_ref()
            .unwrap()
            .candidates
            .iter()
            .map(|candidate| candidate.label.as_str())
            .collect();
        assert_eq!(labels, ["tabnew", "tabnext"]);
        // Tab now starts over: no further shared start, so the list stays
        // and the next Tab picks the first.
        let completing = complete_line(&mut line, completing, true);
        assert_eq!(line, "tabne");
        complete_line(&mut line, completing, true);
        assert_eq!(line, "tabnew");
        // Nothing fits: no list.
        assert!(narrowed("tabz").is_none());
    }

    #[test]
    fn nothing_to_complete_changes_nothing() {
        assert_eq!(presses("zzz", &[true]), [("zzz".to_owned(), false)]);
        assert_eq!(
            presses("set scroll-step ", &[true]),
            [("set scroll-step ".to_owned(), false)]
        );
    }
}
