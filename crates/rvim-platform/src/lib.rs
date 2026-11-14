//! Operating-system services behind traits the editor can test.
//!
//! Unix is the first-class platform. Windows support stays behind the same
//! module boundaries; unsupported operations return a typed error rather than
//! silently doing the wrong thing.

pub mod clipboard;
pub mod env;
pub mod fs;
pub mod process;
pub mod pty;
pub mod term;
pub mod terminfo;

pub use clipboard::{clipboard_get, clipboard_set, ClipboardKind, ClipboardProvider};
pub use fs::{FileInfo, FsError};
pub use process::{iconv, run_shell, run_shell_command, CommandOutput, ProcessError};
pub use pty::{PtyError, PtyPair};
pub use term::{terminal_size, Size};
