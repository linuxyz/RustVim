use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

impl Size {
    pub const fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }
}

pub fn is_tty() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

pub fn terminal_size() -> Size {
    #[cfg(unix)]
    {
        if let Some(size) = unix_size() {
            return size;
        }
    }
    let rows = std::env::var("LINES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(24);
    let cols = std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(80);
    Size { rows, cols }
}

#[cfg(unix)]
fn unix_size() -> Option<Size> {
    use std::os::raw::{c_int, c_ulong};
    #[repr(C)]
    #[derive(Default)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }
    #[cfg(target_os = "macos")]
    const TIOCGWINSZ: c_ulong = 0x4008_7468;
    #[cfg(not(target_os = "macos"))]
    const TIOCGWINSZ: c_ulong = 0x5413;
    extern "C" {
        fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    }
    let mut winsize = Winsize::default();
    // stdout is fd 1; when it is redirected, stdin often still has a size.
    for fd in [1, 0, 2] {
        // SAFETY: TIOCGWINSZ writes a winsize struct into a valid pointer.
        let result = unsafe { ioctl(fd, TIOCGWINSZ, &mut winsize as *mut Winsize) };
        if result == 0 && winsize.ws_row > 0 && winsize.ws_col > 0 {
            return Some(Size {
                rows: winsize.ws_row,
                cols: winsize.ws_col,
            });
        }
    }
    None
}

pub fn colorterm_truecolor() -> bool {
    std::env::var("COLORTERM")
        .map(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("truecolor") || value.contains("24bit")
        })
        .unwrap_or(false)
}

pub fn term_name() -> String {
    std::env::var("TERM").unwrap_or_else(|_| "unknown".to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermCaps {
    pub ansi: bool,
    pub colors: u16,
    pub truecolor: bool,
}

pub fn capabilities() -> TermCaps {
    let mut caps = capabilities_for(&term_name());
    if caps.ansi && colorterm_truecolor() {
        caps.truecolor = true;
        caps.colors = 256;
    }
    caps
}

/// Small built-in terminfo subset for common terminals.
pub fn capabilities_for(term: &str) -> TermCaps {
    let term = term.trim().to_ascii_lowercase();
    if !supports_ansi_for(&term) {
        return TermCaps {
            ansi: false,
            colors: 0,
            truecolor: false,
        };
    }
    if let Some(colors) = crate::terminfo::lookup_colors(&term) {
        return TermCaps {
            ansi: true,
            colors,
            truecolor: false,
        };
    }
    let colors = if term.contains("256color") || term.contains("truecolor") {
        256
    } else if term.contains("color")
        || term.contains("xterm")
        || term.contains("screen")
        || term.contains("tmux")
        || term.contains("linux")
        || term.contains("vt100")
        || term.contains("ansi")
    {
        16
    } else {
        8
    };
    TermCaps {
        ansi: true,
        colors,
        truecolor: false,
    }
}

/// Whether the terminal is expected to understand ANSI escape sequences.
pub fn supports_ansi() -> bool {
    supports_ansi_for(&term_name())
}

pub fn supports_ansi_for(term: &str) -> bool {
    let term = term.trim().to_ascii_lowercase();
    !matches!(term.as_str(), "" | "unknown" | "dumb")
}

/// Suspend the current process until it is resumed (Unix SIGTSTP).
#[cfg(unix)]
pub fn suspend_process() {
    use std::os::raw::c_int;
    #[cfg(target_os = "macos")]
    const SIGTSTP: c_int = 18;
    #[cfg(not(target_os = "macos"))]
    const SIGTSTP: c_int = 20;
    extern "C" {
        fn raise(signal: c_int) -> c_int;
    }
    // SAFETY: raise is called with the SIGTSTP signal constant.
    unsafe {
        raise(SIGTSTP);
    }
}

#[cfg(not(unix))]
pub fn suspend_process() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumb_terminal_has_no_ansi() {
        assert!(!supports_ansi_for("dumb"));
        assert!(!supports_ansi_for(""));
        assert!(supports_ansi_for("xterm-256color"));
    }

    #[test]
    fn common_terminal_capabilities() {
        assert_eq!(capabilities_for("dumb").colors, 0);
        assert!(capabilities_for("xterm-256color").colors >= 16);
        assert!(capabilities_for("xterm").colors > 0);
        assert!(capabilities_for("xterm").ansi);
    }
}
