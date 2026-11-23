//! Helpers for testing the app with real documents: opened through the
//! backend at once, rather than by a task the runtime would carry out; or
//! driven as a [`Session`], which carries out every task the app returns,
//! as the runtime does, so that a test types keys and finds what they did.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use iced::futures::StreamExt;
use iced::futures::executor::block_on;
use iced::keyboard::{self, Location, Modifiers, key};
use iced::{Point, Size, Task};
use iced_runtime::Action;
use iced_runtime::clipboard;

use super::{App, Message};
use crate::input::Key;
use crate::layout::Rect;

/// The app, with no history, so that tests leave the reader's alone,
/// showing a fixture document in a window half a page high, so that
/// the last page can be scrolled to the top.
pub fn showing(fixture: &str) -> (App, u64) {
    let (mut app, _) = App::boot(Vec::new());
    app.history = None;
    let _ = app.update(Message::Resized(Size::new(612.0, 400.0)));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz/seeds/pdf")
        .join(fixture);
    let _ = app.open_in_tab(path.clone());
    let request = app
        .tabs
        .get(app.current)
        .and_then(|tab| tab.opening.as_ref())
        .map(|(request, _)| *request)
        .unwrap();
    let flow = app.flow_for(&path);
    let opened = block_on(crate::backend::open(path.clone(), None, flow));
    let _ = app.update(Message::Opened(request, path, opened));
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
    let _ = app.update(Message::TextFound(id, vec![page], text));
}

/// Types characters at the app, a key press each, as the keyboard would:
/// a capital with shift held.
pub fn typing(app: &mut App, characters: &str) {
    for character in characters.chars() {
        let typed = keyboard::Key::Character(character.to_string().into());
        let modifiers = if character.is_uppercase() {
            Modifiers::SHIFT
        } else {
            Modifiers::empty()
        };
        let _ = app.update(Message::Keyboard(keyboard::Event::KeyPressed {
            key: typed.clone(),
            modified_key: typed,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers,
            text: Some(character.to_string().into()),
            repeat: false,
        }));
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
        self.send(Message::Keyboard(key_event(key)));
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
            let event = if let Some((name, after)) = named {
                rest = after;
                named_key(name)
            } else {
                rest = rest.get(character.len_utf8()..).unwrap_or_default();
                character_key(character)
            };
            self.send(Message::Keyboard(event));
        }
    }

    /// Carries out a task, and the tasks of the messages it gives.
    pub fn run(&mut self, task: Task<Message>) {
        let mut tasks = vec![task];
        let mut messages: usize = 0;
        while let Some(task) = tasks.pop() {
            let Some(mut stream) = iced_runtime::task::into_stream(task) else {
                continue;
            };
            while let Some(action) = block_on(stream.next()) {
                match action {
                    Action::Output(message) => {
                        messages = messages.saturating_add(1);
                        assert!(messages < MOST_MESSAGES, "the app does not settle");
                        tasks.push(self.app.update(message));
                    }
                    Action::Clipboard(clipboard::Action::Write { contents, .. }) => {
                        self.copied.push(contents);
                    }
                    Action::Clipboard(clipboard::Action::Read { channel, .. }) => {
                        let _ = channel.send(self.clipboard.clone());
                    }
                    Action::Exit => self.exited = true,
                    // The window and its widgets are not there to ask:
                    // what waits on them is let go.
                    _ => {}
                }
            }
        }
    }

    /// The text selected in visual mode.
    pub fn selected(&self) -> String {
        selected(&self.app)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The event of a key as pdvim names it, as the keyboard would report it.
fn key_event(key: Key) -> keyboard::Event {
    use keyboard::key::Named;
    let named = |named, modifiers| press(keyboard::Key::Named(named), modifiers, None);
    match key {
        Key::Char(character) => character_key(character),
        Key::Ctrl(letter) => press(
            keyboard::Key::Character(letter.to_string().into()),
            Modifiers::CTRL,
            None,
        ),
        Key::Escape => named(Named::Escape, Modifiers::empty()),
        Key::Enter => named(Named::Enter, Modifiers::empty()),
        Key::Backspace => named(Named::Backspace, Modifiers::empty()),
        Key::ShiftSpace => named(Named::Space, Modifiers::SHIFT),
        Key::Tab => named(Named::Tab, Modifiers::empty()),
        Key::ShiftTab => named(Named::Tab, Modifiers::SHIFT),
        Key::CtrlTab => named(Named::Tab, Modifiers::CTRL),
        Key::CtrlShiftTab => named(Named::Tab, Modifiers::CTRL | Modifiers::SHIFT),
        Key::Up => named(Named::ArrowUp, Modifiers::empty()),
        Key::Down => named(Named::ArrowDown, Modifiers::empty()),
        Key::Left => named(Named::ArrowLeft, Modifiers::empty()),
        Key::Right => named(Named::ArrowRight, Modifiers::empty()),
        Key::PageUp => named(Named::PageUp, Modifiers::empty()),
        Key::PageDown => named(Named::PageDown, Modifiers::empty()),
        Key::Home => named(Named::Home, Modifiers::empty()),
        Key::End => named(Named::End, Modifiers::empty()),
        Key::Function(number) => {
            let function = [
                Named::F1, Named::F2, Named::F3, Named::F4, Named::F5, Named::F6, Named::F7,
                Named::F8, Named::F9, Named::F10, Named::F11, Named::F12,
            ];
            let which = usize::from(number.clamp(1, 12)).saturating_sub(1);
            named(function[which], Modifiers::empty())
        }
    }
}

/// The event of a key named as [`Session::keys`] names it.
fn named_key(name: &str) -> keyboard::Event {
    use keyboard::key::Named;
    if let Some(letter) = name.strip_prefix("C-").and_then(|rest| rest.chars().next()) {
        return press(
            keyboard::Key::Character(letter.to_string().into()),
            Modifiers::CTRL,
            None,
        );
    }
    let (shift, name) = name
        .strip_prefix("S-")
        .map_or((false, name), |rest| (true, rest));
    let named = match name {
        "Esc" => Named::Escape,
        "Enter" | "CR" => Named::Enter,
        "Tab" => Named::Tab,
        "BS" => Named::Backspace,
        "Space" => Named::Space,
        "Up" => Named::ArrowUp,
        "Down" => Named::ArrowDown,
        "Left" => Named::ArrowLeft,
        "Right" => Named::ArrowRight,
        "PageUp" => Named::PageUp,
        "PageDown" => Named::PageDown,
        "Home" => Named::Home,
        "End" => Named::End,
        "F1" => Named::F1,
        other => panic!("no key named <{other}>"),
    };
    let modifiers = if shift {
        Modifiers::SHIFT
    } else {
        Modifiers::empty()
    };
    let text = (named == Named::Space).then(|| " ".into());
    press(keyboard::Key::Named(named), modifiers, text)
}

/// The event of typing a character, with shift held for a capital.
fn character_key(character: char) -> keyboard::Event {
    let modifiers = if character.is_uppercase() {
        Modifiers::SHIFT
    } else {
        Modifiers::empty()
    };
    press(
        keyboard::Key::Character(character.to_string().into()),
        modifiers,
        Some(character.to_string().into()),
    )
}

/// A key pressed, as the keyboard reports it.
fn press(
    pressed: keyboard::Key,
    modifiers: Modifiers,
    text: Option<iced_runtime::core::SmolStr>,
) -> keyboard::Event {
    keyboard::Event::KeyPressed {
        key: pressed.clone(),
        modified_key: pressed,
        physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
        location: Location::Standard,
        modifiers,
        text,
        repeat: false,
    }
}
