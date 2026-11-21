//! Going from one chapter to the next, from the table of contents.

use iced::Task;

use super::{App, Message, Notice, current_document};
use crate::layout::Rect;
use crate::outline;

impl App {
    /// Goes a number of chapters or sections on from the top of the
    /// window, or back, reading the table of contents first if need be.
    pub(super) fn step_chapter(&mut self, forward: bool, times: u32) -> Task<Message> {
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return Task::none();
        };
        let Some(chapters) = &document.chapters else {
            // Read it first; this is done once it is read.
            self.chapter_wanted = Some((forward, times));
            let id = document.id;
            let reading = document.backend.outline();
            return Task::perform(reading, move |result| Message::OutlineFound(id, result));
        };
        let starts = outline::starts(chapters);
        if starts.is_empty() {
            self.notice = Notice::Info("this document has no table of contents".to_owned());
            return Task::none();
        }
        let reader = &document.reader;
        let offsets: Vec<f32> = starts
            .iter()
            .map(|start| {
                let top = start.top.unwrap_or(0.0);
                let spot = Rect {
                    left: 0.0,
                    top,
                    right: 0.0,
                    bottom: top,
                };
                reader
                    .area_on_screen(start.page, spot)
                    .map_or(f32::NAN, |area| area.top)
            })
            .collect();
        let Some(start) =
            outline::step(&offsets, forward, times).and_then(|index| starts.get(index))
        else {
            self.notice = Notice::Info(
                if forward {
                    "no chapter after this one"
                } else {
                    "no chapter before this one"
                }
                .to_owned(),
            );
            return Task::none();
        };
        document.jumps.record(document.reader.place());
        document.reader.go_to_spot(start.page, start.top);
        self.notice = Notice::Info(start.title.clone());
        Task::none()
    }
}
