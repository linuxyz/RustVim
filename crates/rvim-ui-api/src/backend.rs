use crate::clipboard::ClipboardRequest;
use crate::event::UiEvent;
use crate::frame::{Frame, UiMode};
use crate::geometry::Size;
use std::fmt;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorDepth {
    #[default]
    Mono,
    Ansi16,
    Ansi256,
    TrueColor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelMetrics {
    pub cell_width: f32,
    pub cell_height: f32,
    pub dpi: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiCapabilities {
    pub color: ColorDepth,
    pub unicode: bool,
    pub mouse: crate::event::MouseCaps,
    pub bracket_paste: bool,
    pub focus_events: bool,
    pub kitty_keyboard: bool,
    pub modify_other_keys: bool,
    pub title: bool,
    pub clipboard: crate::clipboard::ClipboardCaps,
    pub ime: bool,
    pub pixels: Option<PixelMetrics>,
    pub images: bool,
    pub sync_output: bool,
    pub in_band_resize: bool,
}

impl UiCapabilities {
    pub fn ansi256() -> Self {
        Self {
            color: ColorDepth::Ansi256,
            unicode: true,
            mouse: crate::event::MouseCaps {
                click: true,
                drag: true,
                motion: true,
                wheel: true,
                sgr: true,
                urxvt: false,
            },
            bracket_paste: true,
            focus_events: true,
            kitty_keyboard: false,
            modify_other_keys: false,
            title: true,
            clipboard: crate::clipboard::ClipboardCaps {
                unnamed: true,
                selection: true,
                plus: true,
            },
            ime: true,
            pixels: None,
            images: false,
            sync_output: false,
            in_band_resize: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiError(pub String);

impl UiError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UiError {}

impl From<std::io::Error> for UiError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

pub trait UiBackend {
    fn capabilities(&self) -> UiCapabilities;

    fn init(&mut self) -> Result<(), UiError>;
    fn present(&mut self, frame: &Frame) -> Result<(), UiError>;
    fn poll_event(&mut self, timeout: Duration) -> Result<Option<UiEvent>, UiError>;
    fn resize(&mut self, size: Size) -> Result<(), UiError>;
    fn set_title(&mut self, title: &str) -> Result<(), UiError>;
    fn clipboard(&mut self, request: ClipboardRequest) -> Result<(), UiError>;
    fn suspend(&mut self) -> Result<(), UiError>;
    fn resume(&mut self) -> Result<(), UiError>;
    fn shutdown(&mut self) -> Result<(), UiError>;

    /// Informational mode for backends that change cursor shape without a
    /// frame (GUI only). Default backends can ignore it.
    fn set_mode(&mut self, _mode: UiMode) -> Result<(), UiError> {
        Ok(())
    }
}
