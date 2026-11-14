//! Core value types, identifiers, errors and feature flags shared by all RVim crates.
//!
//! This crate deliberately depends on std only. Vim text is byte-oriented:
//! [`VimString`] stores arbitrary bytes, and buffer columns are byte offsets.

pub mod arena;
pub mod error;
pub mod ids;
pub mod pos;
pub mod unicode;
pub mod version;
pub mod vimstring;

pub use arena::Arena;
pub use error::{Result, VimError};
pub use ids::*;
pub use pos::BufPos;
pub use version::{has_feature, RVIM_VERSION, VIM_VERSION};
pub use vimstring::VimString;
