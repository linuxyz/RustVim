import pathlib
p = pathlib.Path("crates/rvim-platform/src/term.rs")
s = p.read_text()
s += '''

/// Whether the terminal is expected to understand ANSI escape sequences.
pub fn supports_ansi() -> bool {
    supports_ansi_for(&term_name())
}

pub fn supports_ansi_for(term: &str) -> bool {
    let term = term.trim().to_ascii_lowercase();
    !matches!(term.as_str(), "" | "unknown" | "dumb")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumb_terminal_has_no_ansi() {
        assert!(!supports_ansi_for("dumb"));
        assert!(!supports_ansi_for(""));
        assert!(supports_ansi_for("xterm-256color"));
    }
}
'''
p.write_text(s)
print("term patched")
p = pathlib.Path("crates/rvim-tui/src/lib.rs")
s = p.read_text()
old = """        self.raw = true;
        self.size = platform_size();
        self.write_stdout(b"\\x1b[?1049h\\x1b[?25l\\x1b[2J\\x1b[H\\x1b[?2004h\\x1b[?1000h\\x1b[?1002h\\x1b[?1006h\\x1b[?1004h")?;
        Ok(())"""
new = """        self.raw = true;
        self.size = platform_size();
        if term::supports_ansi() {
            self.write_stdout(b"\\x1b[?1049h\\x1b[?25l\\x1b[2J\\x1b[H\\x1b[?2004h\\x1b[?1000h\\x1b[?1002h\\x1b[?1006h\\x1b[?1004h")?;
        }
        Ok(())"""
assert old in s
s = s.replace(old, new, 1)
old2 = """        let _ = self.write_stdout(
            b"\\x1b[?1004l\\x1b[?1006l\\x1b[?1002l\\x1b[?1000l\\x1b[?2004l\\x1b[?25h\\x1b[?1049l",
        );"""
new2 = """        if term::supports_ansi() {
            let _ = self.write_stdout(
                b"\\x1b[?1004l\\x1b[?1006l\\x1b[?1002l\\x1b[?1000l\\x1b[?2004l\\x1b[?25h\\x1b[?1049l",
            );
        }"""
assert old2 in s
s = s.replace(old2, new2, 1)
old3 = """        self.size = frame.size;
        let mut out = String::new();"""
new3 = """        self.size = frame.size;
        if !term::supports_ansi() {
            let composite = frame.composite();
            let mut out = String::new();
            for row in 0..frame.size.rows {
                out.push_str(&composite.row_text(row));
                out.push('\\n');
            }
            self.write_stdout(out.as_bytes())?;
            return Ok(());
        }
        let mut out = String::new();"""
assert old3 in s
s = s.replace(old3, new3, 1)
p.write_text(s)
print("tui patched")
