//! UIKit's general pasteboard, with the error shape the arboard adapter reports.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the clipboard is empty")]
    ContentNotAvailable,
    #[error("the iOS pasteboard is unavailable")]
    Unavailable,
}

pub struct Clipboard;

impl Clipboard {
    pub fn new() -> Result<Self, Error> {
        Ok(Self)
    }

    pub fn get_text(&mut self) -> Result<String, Error> {
        launcher_host::desktop::ios::pasteboard_text().ok_or(Error::ContentNotAvailable)
    }

    pub fn set_text(&mut self, text: String) -> Result<(), Error> {
        if launcher_host::desktop::ios::set_pasteboard_text(text) {
            Ok(())
        } else {
            Err(Error::Unavailable)
        }
    }
}
