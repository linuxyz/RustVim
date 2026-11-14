//! Text properties: display-side types for virtual text overlays.

use rvim_ui_api::Attr;

/// Virtual text inserted before a byte column of a buffer line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VirtualText {
    /// Zero-based byte column the text is displayed before.
    pub col: u32,
    /// Text to display (not part of the buffer).
    pub text: String,
    /// Highlight used for the virtual text.
    pub attr: Attr,
}
