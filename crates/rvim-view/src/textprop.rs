//! Text properties: display-side types for virtual text overlays.

use rvim_ui_api::Attr;

/// Where a piece of virtual text is displayed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VirtualPos {
    /// Before the byte column given by col.
    #[default]
    Inline,
    /// At the end of the line (text property column 0, "after").
    After,
    /// In one or more rows just above the line.
    Above,
    /// In one or more rows just below the line.
    Below,
    /// Right aligned in the window on the line.
    Right,
}

/// Virtual text attached to a buffer line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VirtualText {
    /// Zero-based byte column the text is displayed before (inline) or the
    /// end-of-line column (after).
    pub col: u32,
    /// Text to display (not part of the buffer).
    pub text: String,
    /// Highlight used for the virtual text.
    pub attr: Attr,
    /// Placement relative to the line.
    pub pos: VirtualPos,
    /// Spaces inserted before the virtual text.
    pub padding_left: u16,
    /// Wrap the text to the window width instead of truncating it.
    pub wrap: bool,
    /// Text property priority (higher is displayed closer to the line for
    /// above/below text).
    pub priority: i64,
}

impl VirtualText {
    pub fn inline_after_text(&self) -> bool {
        matches!(self.pos, VirtualPos::Inline | VirtualPos::After)
    }
}

/// Compute the rows to display for a block of virtual text.
///
/// The result is one entry per screen row; each entry is the text without the
/// left padding (the renderer adds it). With wrap disabled the text is
/// truncated to width cells.
pub fn virtual_text_rows(text: &str, width: u16, wrap: bool) -> Vec<String> {
    let width = width.max(1) as usize;
    let clean: String = text
        .chars()
        .map(|ch| {
            if ch == '\t' || (ch as u32) < 0x20 {
                ' '
            } else {
                ch
            }
        })
        .collect();
    if clean.is_empty() {
        return vec![String::new()];
    }
    if !wrap {
        let mut out = String::new();
        let mut used = 0usize;
        for ch in clean.chars() {
            let cells = rvim_types::unicode::char_width(ch).max(1) as usize;
            if used + cells > width {
                break;
            }
            used += cells;
            out.push(ch);
        }
        return vec![out];
    }
    let mut rows = Vec::new();
    let mut current = String::new();
    let mut used = 0usize;
    for ch in clean.chars() {
        let cells = rvim_types::unicode::char_width(ch).max(1) as usize;
        if used + cells > width && !current.is_empty() {
            rows.push(current);
            current = String::new();
            used = 0;
        }
        current.push(ch);
        used += cells;
    }
    if !current.is_empty() || rows.is_empty() {
        rows.push(current);
    }
    rows
}
