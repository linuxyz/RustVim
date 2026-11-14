//! Deterministic headless backend and screen snapshots.

use std::collections::VecDeque;
use std::time::Duration;

use rvim_ui_api::{ClipboardRequest, Frame, Size, UiBackend, UiCapabilities, UiError, UiEvent};

#[derive(Clone, Debug)]
pub struct HeadlessBackend {
    size: Size,
    events: VecDeque<UiEvent>,
    frames: Vec<Frame>,
    last: Option<Frame>,
    title: String,
    clipboard: Vec<ClipboardRequest>,
    pub failures: Vec<String>,
}

impl HeadlessBackend {
    pub fn new(size: Size) -> Self {
        Self {
            size,
            events: VecDeque::new(),
            frames: Vec::new(),
            last: None,
            title: String::new(),
            clipboard: Vec::new(),
            failures: Vec::new(),
        }
    }

    pub fn push_event(&mut self, event: UiEvent) {
        self.events.push_back(event);
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn last_frame(&self) -> Option<&Frame> {
        self.last.as_ref()
    }

    pub fn take_last_frame(&mut self) -> Option<Frame> {
        self.last.take()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn clipboard_requests(&self) -> &[ClipboardRequest] {
        &self.clipboard
    }

    pub fn snapshot(&self) -> String {
        self.last
            .as_ref()
            .map(|frame| frame.composite().to_plain_text())
            .unwrap_or_default()
    }

    pub fn snapshot_with_cursor(&self) -> String {
        let Some(frame) = &self.last else {
            return String::new();
        };
        let mut rows: Vec<String> = (0..frame.size.rows)
            .map(|row| frame.composite().row_text(row))
            .collect();
        if let Some(cursor) = frame.cursor {
            if let Some(row) = rows.get_mut(cursor.row as usize) {
                let mut chars: Vec<char> = row.chars().collect();
                if (cursor.col as usize) < chars.len() {
                    chars[cursor.col as usize] = '|';
                }
                *row = chars.into_iter().collect();
            }
        }
        rows.join("\n")
    }

    /// Deterministic text + per-cell attribute-id + palette dump for golden tests.
    pub fn snapshot_with_attrs(&self) -> String {
        let Some(frame) = &self.last else {
            return String::new();
        };
        let grid = frame.composite();
        let mut out = String::new();
        for row in 0..frame.size.rows {
            out.push_str(&format!(
                "{row:02} |{}|
",
                grid.row_text(row)
            ));
            let ids = (0..frame.size.cols)
                .map(|col| grid.cell(row, col).map_or(0, |cell| cell.attrs))
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            out.push_str(&format!(
                "   {ids}
"
            ));
        }
        out.push_str(
            "
# palette
",
        );
        for (index, attr) in grid.attrs.iter().enumerate() {
            out.push_str(&format!(
                "{index}: fg={:?} bg={:?} sp={:?} flags={}
",
                attr.fg,
                attr.bg,
                attr.sp,
                attr.flags.bits()
            ));
        }
        out
    }

    pub fn assert_clean(&self) -> Result<(), UiError> {
        if self.failures.is_empty() {
            Ok(())
        } else {
            Err(UiError::new(self.failures.join("; ")))
        }
    }
}

impl UiBackend for HeadlessBackend {
    fn capabilities(&self) -> UiCapabilities {
        UiCapabilities {
            color: rvim_ui_api::backend::ColorDepth::TrueColor,
            unicode: true,
            mouse: rvim_ui_api::MouseCaps {
                click: true,
                drag: true,
                motion: true,
                wheel: true,
                sgr: true,
                urxvt: false,
            },
            bracket_paste: true,
            focus_events: true,
            kitty_keyboard: false,
            modify_other_keys: false,
            title: true,
            clipboard: rvim_ui_api::ClipboardCaps {
                unnamed: true,
                selection: true,
                plus: true,
            },
            ime: true,
            pixels: None,
            images: false,
            sync_output: true,
            in_band_resize: true,
        }
    }

    fn init(&mut self) -> Result<(), UiError> {
        Ok(())
    }

    fn present(&mut self, frame: &Frame) -> Result<(), UiError> {
        self.last = Some(frame.clone());
        self.frames.push(frame.clone());
        if let Some(request) = &frame.clipboard {
            self.clipboard.push(request.clone());
        }
        Ok(())
    }

    fn poll_event(&mut self, _timeout: Duration) -> Result<Option<UiEvent>, UiError> {
        Ok(self.events.pop_front())
    }

    fn resize(&mut self, size: Size) -> Result<(), UiError> {
        self.size = size;
        Ok(())
    }

    fn set_title(&mut self, title: &str) -> Result<(), UiError> {
        self.title = title.to_string();
        Ok(())
    }

    fn clipboard(&mut self, request: ClipboardRequest) -> Result<(), UiError> {
        self.clipboard.push(request);
        Ok(())
    }

    fn suspend(&mut self) -> Result<(), UiError> {
        Ok(())
    }

    fn resume(&mut self) -> Result<(), UiError> {
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), UiError> {
        Ok(())
    }
}

pub fn frame_text(frame: &Frame) -> String {
    frame.composite().to_plain_text()
}
