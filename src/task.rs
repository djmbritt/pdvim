//! Work the app asks for in answer to a message: waiting on the backend,
//! writing or reading the clipboard, going full screen, or quitting. The
//! app's `update` returns a [`Task`] rather than doing it, so that it can be
//! carried out by the window (see `app::run`) or, in the tests, at once.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// A future that gives a message, to run off the window's thread.
type Pending<M> = Pin<Box<dyn Future<Output = M> + Send>>;

/// One thing a task asks for.
pub enum Step<M> {
    /// Wait on this, then hand the message it gives to the app.
    Future(Pending<M>),
    /// Put this text on the clipboard.
    Copy(String),
    /// Read the text on the clipboard, and hand the app the message made
    /// of it.
    Paste(Box<dyn FnOnce(Option<String>) -> M + Send>),
    /// Fill the screen, or stop.
    FullScreen(bool),
    /// Quit.
    Exit,
}

/// What the app asks to have done, in no particular order.
#[must_use = "a task does nothing unless it is carried out"]
pub struct Task<M>(Vec<Step<M>>);

impl<M> fmt::Debug for Task<M> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Task").field(&self.0.len()).finish()
    }
}

impl<M: Send + 'static> Task<M> {
    /// Nothing to do.
    pub const fn none() -> Self {
        Self(Vec::new())
    }

    /// Waits on a future, and hands the app the message made of what it
    /// gives.
    pub fn perform<T>(
        future: impl Future<Output = T> + Send + 'static,
        message: impl FnOnce(T) -> M + Send + 'static,
    ) -> Self {
        Self(vec![Step::Future(Box::pin(async move {
            message(future.await)
        }))])
    }

    /// All of these tasks.
    pub fn batch(tasks: impl IntoIterator<Item = Self>) -> Self {
        Self(tasks.into_iter().flat_map(|task| task.0).collect())
    }

    /// Whether there is nothing to do.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The steps, to carry out.
    pub fn into_steps(self) -> Vec<Step<M>> {
        self.0
    }
}

/// Puts text on the clipboard.
pub fn copy<M>(text: impl Into<String>) -> Task<M> {
    Task(vec![Step::Copy(text.into())])
}

/// Reads the clipboard, and hands the app the message made of its text.
pub fn paste<M>(message: impl FnOnce(Option<String>) -> M + Send + 'static) -> Task<M> {
    Task(vec![Step::Paste(Box::new(message))])
}

/// Fills the screen, or stops.
pub fn full_screen<M>(on: bool) -> Task<M> {
    Task(vec![Step::FullScreen(on)])
}

/// Quits.
pub fn exit<M>() -> Task<M> {
    Task(vec![Step::Exit])
}
