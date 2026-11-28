//! Helpers for testing the app with real documents: opened through the
//! backend at once, rather than by a task the window would carry out; or
//! driven as a [`Session`], which carries out every task the app returns,
//! as the window does, so that a test types keys and finds what they did.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::executor::block_on;

use super::{App, Message};
use crate::input::Key;
use crate::layout::Rect;
use crate::task::{Step, Task};
use crate::ui::{Point, Size};

/// The app, with no history, so that tests leave the reader's alone,
/// showing a fixture document in a window half a page high, so that
/// the last page can be scrolled to the top.
pub fn showing(fixture: &str) -> (App, u64) {
    let (mut app, _) = App::boot(Vec::new());
    app.history = None;
    let _: Task<Message> = app.update(Message::Resized(Size::new(612.0, 400.0)));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz/seeds/pdf")
        .join(fixture);
    let _: Task<Message> = app.open_in_tab(path.clone());
    let request = app
        .tabs
        .get(app.current)
        .and_then(|tab| tab.opening.as_ref())
        .map(|(request, _)| *request)
        .unwrap();
    let flow = app.flow_for(&path);
    let opened = block_on(crate::backend::open(path.clone(), None, flow));
    let _: Task<Message> = app.update(Message::Opened(request, path, opened));
    // Started without a file, the app lists the recent documents.
    app.picker = None;
    let id = app.document().unwrap().id;
    (app, id)
}

/// Where a spot of a page is in the window.
pub fn on_screen(app: &App, page: u32, (across, down): (f32, f32)) -> Point {
    let spot = Rect {
        left: across,
        top: down,
        right: across,
        bottom: down,
    };
    let area = app
        .document()
        .unwrap()
        .reader
        .area_on_screen(page, spot)
        .unwrap();
    Point::new(area.left, area.top)
}

/// Gives the app the text of a page, as the backend answers.
pub fn read(app: &mut App, id: u64, page: u32) {
    let text = block_on(app.document().unwrap().backend.text(vec![page]));
    let _: Task<Message> = app.update(Message::TextFound(id, vec![page], text));
}

/// Types characters at the app, a key press each, as the keyboard would.
pub fn typing(app: &mut App, characters: &str) {
    for character in characters.chars() {
        let _: Task<Message> = app.update(Message::Keyboard(Key::Char(character)));
    }
}

pub fn selected(app: &App) -> String {
    let document = app.document().unwrap();
    app.visual.as_ref().unwrap().text(&document.text)
}

/// The app with a document open, its tasks carried out as the runtime
/// would: each message's task is run to its end, and the messages it gives
/// go back into the app, until there are no more. What the app asked of
/// the system, which a test has none of, is kept for the test to look at:
/// what it copied, and whether it quit. Reading the clipboard gives
/// [`Session::clipboard`].
///
/// The app has the built-in configuration only, not the reader's, and
/// opens copies of the fixtures in a directory of its own, removed when
/// the session ends, so that saving into them leaves the originals be.
pub struct Session {
    pub app: App,
    /// Where the fixtures are copied to, and anything else the test
    /// writes.
    pub dir: PathBuf,
    /// The document's id.
    pub id: u64,
    /// What the app copied, in turn.
    pub copied: Vec<String>,
    /// What reading the clipboard gives.
    pub clipboard: Option<String>,
    /// Whether the app asked to quit.
    pub exited: bool,
}

/// The most messages one key or message may lead to, so that a test of
/// something that never settles fails rather than hangs.
const MOST_MESSAGES: usize = 10_000;

impl Session {
    /// The app showing a fixture document from `fuzz/seeds/pdf`, opened as
    /// `:open` opens one, in a window half a page high, with no history so
    /// that the tests leave the reader's alone.
    pub fn open(fixture: &str) -> Self {
        let mut session = Self::start();
        session.open_fixture(fixture);
        session.id = session.app.document().expect("the fixture opens").id;
        session
    }

    /// The app started without a document, in a window half a page high,
    /// with no history.
    pub fn start() -> Self {
        static SESSIONS: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "pdvim-test-{}-{}",
            std::process::id(),
            SESSIONS.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (mut app, boot) = App::boot(Vec::new());
        app.history = None;
        // The built-in configuration alone, whatever the reader's says.
        let empty = dir.join("config.toml");
        std::fs::write(&empty, "").unwrap();
        app.read_configuration(Some(&empty));
        let mut session = Self {
            app,
            dir,
            id: 0,
            copied: Vec::new(),
            clipboard: None,
            exited: false,
        };
        session.run(boot);
        session.send(Message::Resized(Size::new(612.0, 400.0)));
        // Started without a file, the app lists the recent documents.
        session.app.picker = None;
        session
    }

    /// Opens a fixture document from `fuzz/seeds/pdf` in a new tab, as
    /// `:open` does, and carries out what that leads to: a locked one asks
    /// for its password.
    pub fn open_fixture(&mut self, fixture: &str) {
        let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fuzz/seeds/pdf")
            .join(fixture);
        let path = self.dir.join(fixture);
        std::fs::copy(original, &path).unwrap();
        let opening = self.app.open_in_tab(path);
        self.run(opening);
    }

    /// Presses a key, as pdvim names it.
    pub fn press(&mut self, key: Key) {
        // The key that stands for it unless the test sets another.
        let key = if key == Key::Leader {
            crate::keymap::DEFAULT_LEADER
        } else {
            key
        };
        self.send(Message::Keyboard(key));
    }

    /// Gives the app a message, and carries out all it leads to.
    pub fn send(&mut self, message: Message) {
        let task = self.app.update(message);
        self.run(task);
    }

    /// Types keys, each carried out in full before the next: characters as
    /// they are, and named keys in vim's way, as `<Esc>`, `<Enter>`,
    /// `<Tab>`, `<S-Tab>`, `<BS>`, `<Space>`, `<S-Space>`, `<Up>`,
    /// `<Down>`, `<Left>`, `<Right>`, `<PageUp>`, `<PageDown>`, `<Home>`,
    /// `<End>`, `<F1>` and `<C-o>` for a letter with control held.
    pub fn keys(&mut self, typed: &str) {
        let mut rest = typed;
        while let Some(character) = rest.chars().next() {
            let named = rest
                .strip_prefix('<')
                .and_then(|after| after.split_once('>'))
                .filter(|(name, _)| !name.is_empty() && !name.contains('<'));
            let key = if let Some((name, after)) = named {
                rest = after;
                named_key(name)
            } else {
                rest = rest.get(character.len_utf8()..).unwrap_or_default();
                Key::Char(character)
            };
            self.press(key);
        }
    }

    /// Carries out a task, and the tasks of the messages it gives.
    pub fn run(&mut self, task: Task<Message>) {
        let mut steps: Vec<Step<Message>> = task.into_steps();
        let mut messages: usize = 0;
        while let Some(step) = steps.pop() {
            let message = match step {
                Step::Future(future) => block_on(future),
                Step::Copy(text) => {
                    self.copied.push(text);
                    continue;
                }
                Step::Paste(message) => message(self.clipboard.clone()),
                Step::Exit => {
                    self.exited = true;
                    continue;
                }
                // There is no window to fill the screen with.
                Step::FullScreen(_) => continue,
            };
            messages = messages.saturating_add(1);
            assert!(messages < MOST_MESSAGES, "the app does not settle");
            steps.extend(self.app.update(message).into_steps());
        }
    }

    /// Lays out and builds the window as it is now, its pages and all that
    /// is drawn over them, as the window would, with no screen, so that
    /// what would fail drawing fails the test.
    pub fn draw(&mut self) {
        let app = std::mem::replace(&mut self.app, App::empty());
        let env = rune::Env {
            system_dark: true,
            scale: 1.0,
        };
        let mut window = rune::Runtime::new(app, super::window::view, (612.0, 400.0), env);
        window.frame(0.0);
        window.frame(1.0 / 60.0);
        assert!(!window.elements().is_empty(), "the window draws");
        self.app = std::mem::replace(&mut window.state, App::empty());
    }

    /// The text selected in visual mode.
    pub fn selected(&self) -> String {
        selected(&self.app)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _: std::io::Result<()> = std::fs::remove_dir_all(&self.dir);
    }
}

/// The key named as [`Session::keys`] names it.
fn named_key(name: &str) -> Key {
    if let Some(letter) = name.strip_prefix("C-").and_then(|rest| rest.chars().next()) {
        return Key::Ctrl(letter.to_ascii_lowercase());
    }
    match name {
        "Esc" => Key::Escape,
        "Enter" | "CR" => Key::Enter,
        "Tab" => Key::Tab,
        "S-Tab" => Key::ShiftTab,
        "BS" => Key::Backspace,
        "Space" => Key::Char(' '),
        "S-Space" => Key::ShiftSpace,
        "Up" => Key::Up,
        "Down" => Key::Down,
        "Left" => Key::Left,
        "Right" => Key::Right,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Home" => Key::Home,
        "End" => Key::End,
        "F1" => Key::Function(1),
        other => panic!("no key named <{other}>"),
    }
}
