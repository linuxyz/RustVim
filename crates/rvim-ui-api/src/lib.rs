//! Backend-neutral cell protocol shared by the editor, TUI, GUI and headless
//! drivers.
//!
//! The backend never learns what a Vim window, highlight group or statusline
//! item is. It receives immutable frames of cells and returns semantic events.

pub mod attr;
pub mod backend;
pub mod clipboard;
pub mod event;
pub mod frame;
pub mod geometry;

pub use attr::{Attr, AttrFlags, Color};
pub use backend::{UiBackend, UiCapabilities, UiError};
pub use clipboard::{ClipboardCaps, ClipboardKind, ClipboardRequest, ClipboardResult, RequestId};
pub use event::{
    ImeEvent, Key, KeyEvent, KeyKind, Mods, MouseButton, MouseCaps, MouseEvent, MouseKind,
    QuitReason, TermResponse, UiEvent,
};
pub use frame::{
    ActionId, ActionKind, Cell, ClickArea, ClickRegion, Cursor, CursorShape, Frame, Grid, GridId,
    HitRegion, Layer, LayerKind, MouseShape, UiMode, UI_PROTOCOL_VERSION,
};
pub use geometry::{Rect, Size};
