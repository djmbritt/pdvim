//! Running a command line: the commands in it one after another, as typed
//! after `:` or bound to keys.

use iced::Task;

use super::{App, Message, Notice};
use crate::command;

impl App {
    /// Runs each command in a line in turn, stopping at the first that
    /// cannot be understood.
    pub(super) fn run_line(&mut self, line: &str) -> Task<Message> {
        let mut tasks = Vec::new();
        for part in command::split(line) {
            match command::parse(&part) {
                Ok(Some(command)) => tasks.push(self.run_command(command)),
                Ok(None) => {}
                Err(error) => {
                    self.notice = Notice::Error(error.to_string());
                    break;
                }
            }
        }
        Task::batch(tasks)
    }
}
