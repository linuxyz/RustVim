import pathlib
p = pathlib.Path("crates/rvim-view/src/view.rs")
s = p.read_text()
s = s.replace("use crate::window::Window;", "use crate::window::{Window, WindowOptions};", 1)
old = """fn continuation_prefix(window: &Window, line: &[u8]) -> String {
    let mut prefix = window.options.showbreak.clone();
    if window.options.breakindent {
        let indent = line
            .iter()
            .copied()
            .take_while(|byte| *byte == b' ' || *byte == b'\\t')
            .collect::<Vec<u8>>();
        prefix.push_str(&String::from_utf8_lossy(&indent));
    }
    prefix
}"""
new = """fn continuation_prefix(window: &Window, line: &[u8]) -> String {
    continuation_prefix_for(line, &window.options)
}

fn continuation_prefix_for(line: &[u8], options: &WindowOptions) -> String {
    let mut prefix = options.showbreak.clone();
    if options.breakindent {
        let indent = line
            .iter()
            .copied()
            .take_while(|byte| *byte == b' ' || *byte == b'\\t')
            .collect::<Vec<u8>>();
        prefix.push_str(&String::from_utf8_lossy(&indent));
    }
    prefix
}

/// Display-line chunks for a buffer line, honoring wrap/linebreak/showbreak.
pub fn display_chunks(line: &[u8], text_width: u16, options: &WindowOptions) -> Vec<(usize, usize)> {
    let prefix = continuation_prefix_for(line, options);
    let prefix_width = text_width_of(&prefix, options.tabstop.max(1));
    let rest_width = text_width.saturating_sub(prefix_width).max(1);
    wrap_chunks(
        line,
        text_width as usize,
        rest_width as usize,
        options.tabstop.max(1),
        options.linebreak,
        &options.breakat,
    )
}

/// Width of the continuation prefix (showbreak + breakindent).
pub fn display_prefix_width(line: &[u8], text_width: u16, options: &WindowOptions) -> u16 {
    let prefix = continuation_prefix_for(line, options);
    let width = text_width_of(&prefix, options.tabstop.max(1));
    width.min(text_width)
}"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("view patched")
p = pathlib.Path("crates/rvim-view/src/lib.rs")
s = p.read_text()
s = s.replace("pub use statusline::{expand_statusline, format_default_statusline, StatuslineOptions};", "pub use statusline::{expand_statusline, format_default_statusline, StatuslineOptions};\npub use view::{display_chunks, display_prefix_width};", 1)
p.write_text(s)
print("lib patched")
