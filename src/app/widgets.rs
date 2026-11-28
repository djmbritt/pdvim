//! The pieces the window's lists and panels are made of, in Rune.

use rune::{Cursor, El, col, el};

use super::{App, Message, pointer};
use crate::theme::Colours;
use crate::ui::Color;

/// Something that does this when clicked, with the pointer showing so.
pub(super) fn clickable(content: El, click: pointer::Click) -> El {
    content
        .cursor(Cursor::Pointer)
        .on_press_tx(move |app: &mut App, tx| app.dispatch(Message::Clicked(click), tx))
}

/// Takes the room left in its row, and cuts off what is too long for it.
pub(super) fn filling(content: El) -> El {
    el().grow(1.0).min_w(0.0).clip_soft(0.0).child(content)
}

/// Exactly so wide, cutting off what is too long for it.
pub(super) fn cut_to(width: f32, content: El) -> El {
    el().w(width).min_w(0.0).clip_soft(0.0).child(content)
}

/// A panel over the pages: the list's colour, framed, its corners rounded
/// by so much.
pub(super) fn panel(content: El, chrome: Colours, radius: f32) -> El {
    content
        .fill(chrome.list.rune())
        .border(1.0, chrome.list_border.rune())
        .radius(radius)
}

/// A background of this colour, or none.
pub(super) fn shaded(content: El, shade: Option<Color>) -> El {
    match shade {
        Some(shade) => content.fill(shade.rune()),
        None => content,
    }
}

/// A layer over the whole of its parent.
pub(super) fn layer() -> El {
    el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
}

/// A layer that holds a panel in the middle of its parent, a margin from
/// its edges.
pub(super) fn centred(panel: El) -> El {
    layer().center().padding(24.0).child(panel)
}

/// A layer that holds a band along the bottom of its parent.
pub(super) fn along_bottom(band: El) -> El {
    col()
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .justify_end()
        .clip_soft(0.0)
        .child(band.w_full())
}
