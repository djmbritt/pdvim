//! Copying pictures to the clipboard, which iced's clipboard cannot: it
//! holds only text.
//!
//! On Linux a program offers what it copied for as long as it keeps the
//! clipboard open, so one is opened the first time a picture is copied
//! and kept until pdvim quits. It is used on a thread of its own, as
//! handing a picture over can take a moment.

use std::sync::{Mutex, PoisonError};

use iced::futures::channel::oneshot;

/// The clipboard, once opened.
static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// Copies a picture, its rows of red, green, blue and alpha bytes, to the
/// clipboard, and answers once it is there, or why it is not.
pub fn copy_picture(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> impl Future<Output = Result<(), String>> {
    let (reply, answer) = oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("pdvim-clipboard".to_owned())
        .spawn(move || {
            let _: Result<(), _> = reply.send(copy_now(width, height, pixels));
        });
    async move {
        spawned.map_err(|error| error.to_string())?;
        answer
            .await
            .unwrap_or_else(|_| Err("the clipboard did not answer".to_owned()))
    }
}

fn copy_now(width: u32, height: u32, pixels: Vec<u8>) -> Result<(), String> {
    let picture = arboard::ImageData {
        width: usize::try_from(width).map_err(|error| error.to_string())?,
        height: usize::try_from(height).map_err(|error| error.to_string())?,
        bytes: pixels.into(),
    };
    let mut opened = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
    let clipboard = match opened.take() {
        Some(clipboard) => clipboard,
        None => arboard::Clipboard::new().map_err(|error| error.to_string())?,
    };
    let clipboard = opened.insert(clipboard);
    let copied = clipboard.set_image(picture);
    drop(opened);
    copied.map_err(|error| error.to_string())
}
