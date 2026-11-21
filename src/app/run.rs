//! Running a command line: the commands in it one after another, as typed
//! after `:` or bound to keys.

use iced::Task;

use super::{App, Message, Notice};
use crate::command;

impl App {
    /// Runs each command in a line in turn, stopping at the first that
    /// cannot be understood. The line is kept for `.` to run again.
    pub(super) fn run_line(&mut self, line: &str) -> Task<Message> {
        if !command::split(line).is_empty() {
            self.last_line = Some(line.to_owned());
        }
        self.run_commands(line)
    }

    /// Runs the last command line again, a number of times over.
    pub(super) fn repeat_line(&mut self, times: u32) -> Task<Message> {
        let Some(line) = self.last_line.clone() else {
            self.notice = Notice::Error("no command to repeat".to_owned());
            return Task::none();
        };
        let tasks: Vec<_> = (0..times.max(1))
            .map(|_| self.run_commands(&line))
            .collect();
        Task::batch(tasks)
    }

    /// Runs each command in a line in turn, stopping at the first that
    /// cannot be understood.
    fn run_commands(&mut self, line: &str) -> Task<Message> {
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
