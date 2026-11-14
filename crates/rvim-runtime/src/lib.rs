//! Vim runtime discovery, path lookup and lightweight filetype detection.
//!
//! RVim targets the unmodified Vim 9.2.1338 runtime tree, but the tree is not
//! part of this crate and may be absent. Every API here therefore reports
//! missing data honestly: [`RuntimePaths::vimruntime`] returns `None`, file
//! lookups return `None`/`Vec::new()`, and no path is invented.
//!
//! The crate implements the path semantics the editor needs at startup:
//!
//! * parsing Vim's comma-separated `'runtimepath'` values, including escaped
//!   commas and spaces and empty entries (the current directory);
//! * appending `$VIMRUNTIME` when a user-supplied runtimepath does not contain
//!   it;
//! * locating the first or all matches for a runtime-relative file name, in
//!   exactly the order the directories were configured (including `after`
//!   directories in the position they were given);
//! * expanding `~`, `$VAR` and `${VAR}` when a lookup is performed, without
//!   rewriting [`RuntimePaths::runtimepath`];
//! * a small extension/shebang/first-line filetype detector.
//!
//! What this crate deliberately does **not** do: expand wildcards for
//! `:runtime`, source scripts, parse Vim's `filetype.vim`/`scripts.vim`, or
//! guess a runtime tree from an unknown install layout. Those belong to the
//! script loader and to startup wiring.

pub mod filetype;
pub mod paths;

pub use filetype::detect_filetype;
pub use paths::{split_runtimepath, RuntimePaths};
