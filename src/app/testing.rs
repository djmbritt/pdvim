//! Helpers for testing the app with real documents: opened through the
//! backend at once, rather than by a task the runtime would carry out.

use iced::futures::executor::block_on;
use iced::keyboard::{self, Location, Modifiers, key};
use iced::{Point, Size};

use super::{App, Message};
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
