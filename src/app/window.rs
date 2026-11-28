//! The window, as Rune runs it: the app made at start, the keys it is
//! given, and the tasks its messages ask for carried out, the waiting ones
//! on threads of their own, their answers handed back as messages.

use std::path::PathBuf;
use std::sync::mpsc;

use futures::executor::ThreadPool;
use rune::{AppSpec, CloseResponse, Cx, El, Key as RuneKey, KeyEvent, Tx, Waker};

use super::{App, Message};
use crate::input::Key;
use crate::task::{Step, Task};

/// How many threads wait on the backend's answers. They only wait, the
/// backend does the work, so a few are plenty.
const WAITERS: usize = 2;

/// Carries out the waiting part of tasks: each future on the pool, its
/// message sent back to the window, which is woken to take it.
pub(super) struct Runner {
    pool: ThreadPool,
    sender: mpsc::Sender<Message>,
    receiver: mpsc::Receiver<Message>,
    waker: Waker,
}

impl Runner {
    fn new(waker: Waker) -> Option<Self> {
        let pool = ThreadPool::builder()
            .pool_size(WAITERS)
            .name_prefix("pdvim-wait-")
            .create()
            .ok()?;
        let (sender, receiver) = mpsc::channel();
        Some(Self {
            pool,
            sender,
            receiver,
            waker,
        })
    }
}

/// Opens the window, with these documents in tabs, and runs until it is
/// closed.
///
/// # Errors
///
/// When there is no window to be had: no display, or no way to draw.
pub fn run(paths: Vec<PathBuf>) -> Result<(), String> {
    super::launch::start();
    rune::set_app_id("pdvim");
    let spec = AppSpec {
        min_size: Some((200.0, 150.0)),
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        ..AppSpec::new("pdvim", (1024.0, 768.0), App::empty, view)
    };
    rune::App::new(spec)
        // pdvim reads its own command line; Rune is given none of it.
        .args(Vec::new())
        .init(move || {
            super::launch::mark("pdvim.init");
            let (mut app, starting) = App::boot(paths);
            app.starting = Some(starting);
            super::launch::mark("pdvim.init.done");
            app
        })
        .on_frame_drawn(|_| super::launch::frame_drawn())
        .on_close_request(on_close_request)
        .wheel_line(1.0)
        .try_run()
}

/// Has the window draw what the app shows.
pub(super) fn view(app: &App, cx: &mut Cx<App>) -> El {
    app.view(cx)
}

/// Gets ready to carry out tasks, now that there is a window to wake.
fn setup(app: &mut App, waker: Waker) {
    app.runner = Runner::new(waker);
}

/// Hands the app the answers that came in since the last frame, and
/// carries out what it asked for at start.
fn poll(app: &mut App, _: f64, tx: &mut Tx) -> bool {
    let mut changed = app.starting.take().is_some_and(|starting| {
        app.carry_out(starting, tx);
        true
    });
    while let Some(message) = app
        .runner
        .as_ref()
        .and_then(|runner| runner.receiver.try_recv().ok())
    {
        if matches!(message, Message::Rendered(..)) {
            super::launch::page_arrived();
        }
        app.dispatch(message, tx);
        changed = true;
    }
    changed
}

/// Every key goes to the app, none to Rune: nothing in the window takes
/// focus, and pdvim's bindings are its own.
fn on_key(app: &mut App, event: &KeyEvent, tx: &mut Tx) -> bool {
    if let Some(key) = translate(event) {
        app.dispatch(Message::Keyboard(key), tx);
    }
    true
}

/// Asks the app, which may want to save first; it closes the window itself
/// once it is done.
fn on_close_request(app: &mut App, tx: &mut Tx) -> CloseResponse {
    app.dispatch(Message::CloseRequested, tx);
    CloseResponse::Defer
}

impl App {
    /// Acts on a message, and carries out what it asks for. Everything
    /// changes at once: a reader's window moves with its keys, never
    /// gliding after them.
    ///
    /// A message that only moves the view slides the pages there, and the
    /// window is not built again (see `sliding`).
    pub(super) fn dispatch(&mut self, message: Message, tx: &mut Tx) {
        tx.instant();
        let before = Self::may_slide(&message).then(|| self.still()).flatten();
        let task = self.update(message);
        let slid = before.is_some() && task.is_empty() && self.still() == before && self.slide();
        if !slid {
            self.settle();
        }
        self.carry_out(task, tx);
        let title = self.title();
        if self.shown_title.as_deref() != Some(title.as_str()) {
            tx.set_title(title.clone());
            self.shown_title = Some(title);
        }
        if slid {
            tx.live_only();
        }
    }

    /// Carries out a task: what the window does at once, and what has to
    /// wait, on the runner.
    pub(super) fn carry_out(&mut self, task: Task<Message>, tx: &mut Tx) {
        for step in task.into_steps() {
            match step {
                Step::Future(future) => {
                    if let Some(runner) = &self.runner {
                        let sender = runner.sender.clone();
                        let waker = runner.waker.clone();
                        runner.pool.spawn_ok(async move {
                            let message = future.await;
                            // Gone only when the window is.
                            let _: Result<(), _> = sender.send(message);
                            waker();
                        });
                    } else {
                        // Without a window to wake, as in a test that
                        // draws it, the answer is waited for here.
                        let message = futures::executor::block_on(future);
                        self.dispatch(message, tx);
                    }
                }
                Step::Copy(text) => tx.copy(text),
                Step::Paste(message) => {
                    tx.read_clipboard(move |app: &mut Self, content, tx| {
                        let text = Some(content.text().to_owned()).filter(|text| !text.is_empty());
                        app.dispatch(message(text), tx);
                    });
                }
                Step::FullScreen(on) => tx.set_full_screen(on),
                Step::Exit => tx.quit(),
            }
        }
    }
}

/// Reduces a key press to the keys the reader distinguishes.
#[expect(clippy::wildcard_enum_match_arm, reason = "keys pdvim has no use for")]
pub(super) fn translate(event: &KeyEvent) -> Option<Key> {
    let mods = event.mods;
    if mods.alt || mods.logo {
        return None;
    }
    match &event.key {
        RuneKey::Escape => Some(Key::Escape),
        RuneKey::Enter => Some(Key::Enter),
        RuneKey::Backspace => Some(Key::Backspace),
        RuneKey::Up => Some(Key::Up),
        RuneKey::Down => Some(Key::Down),
        RuneKey::Left => Some(Key::Left),
        RuneKey::Right => Some(Key::Right),
        RuneKey::PageUp => Some(Key::PageUp),
        RuneKey::PageDown => Some(Key::PageDown),
        RuneKey::Home => Some(Key::Home),
        RuneKey::End => Some(Key::End),
        RuneKey::Tab if mods.ctrl && mods.shift => Some(Key::CtrlShiftTab),
        RuneKey::Tab if mods.ctrl => Some(Key::CtrlTab),
        RuneKey::Tab if mods.shift => Some(Key::ShiftTab),
        RuneKey::Tab => Some(Key::Tab),
        RuneKey::Function(number @ 1..=12) => Some(Key::Function(*number)),
        // Space is a character, with control held or not.
        RuneKey::Text(typed) if typed == " " && mods.shift => Some(Key::ShiftSpace),
        RuneKey::Text(typed) if typed == " " => Some(Key::Char(' ')),
        RuneKey::Command(' ') if mods.shift => Some(Key::ShiftSpace),
        RuneKey::Command(' ') => Some(Key::Char(' ')),
        RuneKey::Command(letter) if mods.ctrl => Some(Key::Ctrl(letter.to_ascii_lowercase())),
        RuneKey::Text(typed) if mods.ctrl => typed
            .chars()
            .next()
            .map(|letter| Key::Ctrl(letter.to_ascii_lowercase())),
        RuneKey::Text(typed) => typed
            .chars()
            .next()
            .filter(|character| !character.is_control())
            .map(Key::Char),
        _ => None,
    }
}
