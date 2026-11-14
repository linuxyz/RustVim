//! RVim editor core: state ownership, input modes, Ex dispatch, files and
//! rendering orchestration.
//!
//! The editor owns buffers and the presentation view. It never links a backend.

pub mod autocmd;
pub mod buffer;
pub mod editor;
pub mod errorformat;
pub mod ex;
pub mod fold;
pub mod glob;
pub mod highlights;
pub mod keymap;
pub mod modeline;
pub mod options;
pub(crate) mod options_extra;
pub mod qf;
pub mod registry;
pub mod registry_abbrev;
pub mod registry_generated;
pub mod render;
pub mod scheduler;
pub mod search;
pub mod signs;
pub mod swap;
pub mod tags;
pub mod textprop;
pub mod trace;
pub mod usercmd;
pub mod viml_host;

pub use buffer::BufferEntry;
pub use editor::{CommandKind, Editor, MessageKind, Mode, Operator};
pub use highlights::HighlightTable;
pub use keymap::{MapMode, MappingState};
pub use options::{OptionStore, OptionValue};

pub mod undofile;
pub mod viminfo;
