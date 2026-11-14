//! Presentation layer: tab pages, windows, viewports, statusline, command
//! line and compositing into backend-neutral cells.
//!
//! This crate never imports a terminal or GUI backend and never resolves Vim
//! commands. It reads the model through [`RenderHost`] and writes a [`rvim_ui_api::Frame`].

pub mod arena_map;
pub mod diff;
pub mod fold;
pub mod layout;
pub mod signs;
pub mod statusline;
pub mod textprop;
pub mod view;
pub mod window;

pub use diff::{diff_fill_char, DiffBlock, DiffKind, DiffState};
pub use fold::{fillchars_char, fillchars_opt, FoldState};
pub use layout::{all_windows, compute_layout, LayoutNode, LayoutRect, SplitDir};
pub use signs::{SignCell, SignState};
pub use statusline::{expand_statusline, format_default_statusline, StatuslineOptions};
pub use textprop::{virtual_text_rows, VirtualPos, VirtualText};
pub use view::{display_chunks, display_prefix_width};
pub use view::{Chrome, RenderHost, ViewState};
pub use window::{Viewport, VisualSelection, Window, WindowOptions};
