//! Editing primitives independent of the UI: motions, ranges and registers.
//!
//! These functions operate on [`rvim_text::TextBuffer`] and [`rvim_types::BufPos`]. The
//! editor owns modes, operators and undo grouping; this crate owns the geometry
//! of cursor movement and text ranges.

pub mod motion;
pub mod range;
pub mod register;

pub use motion::{byte_col, move_cursor, screen_col, word_class, CharClass, Motion};
pub use range::{RangeKind, TextRange};
pub use register::{RegisterContent, RegisterKind, RegisterName, Registers};
