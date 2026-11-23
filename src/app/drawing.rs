//! The window drawn in each of the states it has, with the renderer that
//! needs no screen: none may fail to draw. What is drawn is not compared
//! with a picture; these find what panics or fails drawing, such as a
//! label drawn outside its page or a list longer than its room.

use super::testing::Session;
use crate::theme::Theme;

/// Opens a fixture, types keys, and draws the window.
fn drawn(fixture: &str, typed: &str) -> Session {
    let mut session = Session::open(fixture);
    session.keys(typed);
    session.draw();
    session
}

#[test]
fn a_page_with_its_bars() {
    drawn("text.pdf", "");
}

#[test]
fn pictures_and_drawings_with_their_highlight() {
    drawn("drawn.pdf", "");
}

#[test]
fn the_labels_of_lines_and_a_selection() {
    let mut session = drawn("text.pdf", "v");
    session.keys("aae");
    session.draw();
}

#[test]
fn the_labels_of_a_jump() {
    drawn("text.pdf", "swo");
}

#[test]
fn the_labels_of_links() {
    drawn("links.pdf", "f");
}

#[test]
fn the_labels_of_figures() {
    drawn("drawn.pdf", "I");
}

#[test]
fn the_table_of_contents() {
    drawn("outline.pdf", "<Tab>");
}

#[test]
fn every_page_small() {
    drawn("outline.pdf", "go");
}

#[test]
fn highlights_notes_and_their_list() {
    let mut session = drawn("text.pdf", "vaaeH");
    session.keys("vaa<Down>n");
    session.keys("a thought<Enter>");
    session.draw();
    session.keys("<S-Tab>");
    session.draw();
}

#[test]
fn the_lists_of_recent_documents_and_of_tabs() {
    let mut session = drawn("text.pdf", "o");
    session.keys("<Esc>");
    session.open_fixture("links.pdf");
    session.keys("b");
    session.draw();
}

#[test]
fn the_help_sheet() {
    drawn("text.pdf", ":help<Enter>");
}

#[test]
fn the_commands_offered_and_matches_as_a_search_is_typed() {
    let mut session = drawn("text.pdf", ":se");
    session.keys("<Esc>/lin");
    session.draw();
}

#[test]
fn a_field_typed_into_and_a_list_to_pick_from() {
    let mut session = drawn("form.pdf", "gi");
    session.keys("<Esc>gI");
    session.draw();
}

#[test]
fn the_question_for_a_password() {
    let mut session = Session::start();
    session.open_fixture("encrypted.pdf");
    session.keys("sec");
    session.draw();
}

#[test]
fn two_pages_side_by_side_recoloured() {
    drawn("outline.pdf", ":set page-mode double<Enter>:set recolor true<Enter>");
}

#[test]
fn every_theme() {
    for name in Theme::names() {
        let mut session = Session::open("text.pdf");
        session.keys(&format!(":theme {name}<Enter>"));
        assert_eq!(session.app.theme.map(Theme::name), Some(name));
        // A mode with labels shows more of the theme's colours.
        session.keys("v");
        session.draw();
    }
}
