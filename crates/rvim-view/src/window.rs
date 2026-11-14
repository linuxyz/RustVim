use rvim_types::{BufPos, BufferId, WindowId};
use rvim_ui_api::{Size, UiMode};

use crate::diff::DiffState;
use crate::fold::FoldState;
use crate::signs::SignState;
use crate::view::RenderHost;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewport {
    pub topline: u32,
    pub topfill: u16,
    pub leftcol: u32,
    pub skipcol: u32,
    pub curswant: u32,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowOptions {
    pub number: bool,
    pub relativenumber: bool,
    pub wrap: bool,
    pub linebreak: bool,
    pub breakat: String,
    pub showbreak: String,
    pub breakindent: bool,
    pub smoothscroll: bool,
    pub list: bool,
    pub listchars: String,
    pub cursorline: bool,
    pub cursorcolumn: bool,
    pub colorcolumn: String,
    pub statusline: Option<String>,
    pub laststatus: u8,
    pub showtabline: u8,
    pub tabstop: usize,
    pub shiftwidth: usize,
    pub scrolloff: usize,
    pub sidescrolloff: usize,
    pub signcolumn: String,
    pub foldcolumn: u8,
    pub numberwidth: u8,
    pub numberformat: String,
    pub winfixheight: bool,
    pub winfixwidth: bool,
    pub fillchars: String,
    pub foldmethod: String,
    pub foldmarker: String,
    pub foldignore: String,
    pub foldminlines: i64,
    pub foldnestmax: i64,
    pub foldtext: String,
    pub foldexpr: String,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            number: false,
            relativenumber: false,
            wrap: true,
            linebreak: false,
            breakat: String::new(),
            showbreak: String::new(),
            breakindent: false,
            smoothscroll: false,
            list: false,
            listchars: String::new(),
            cursorline: false,
            cursorcolumn: false,
            colorcolumn: String::new(),
            statusline: None,
            laststatus: 2,
            showtabline: 1,
            tabstop: 8,
            shiftwidth: 8,
            scrolloff: 0,
            sidescrolloff: 0,
            signcolumn: String::from("auto"),
            foldcolumn: 0,
            numberwidth: 4,
            numberformat: String::from("%d"),
            winfixheight: false,
            winfixwidth: false,
            fillchars: String::new(),
            foldmethod: String::from("manual"),
            foldmarker: String::from("{{{,}}}"),
            foldignore: String::from("#"),
            foldminlines: 1,
            foldnestmax: 20,
            foldtext: String::from("foldtext()"),
            foldexpr: String::from("0"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualSelection {
    pub start: BufPos,
    pub end: BufPos,
    pub linewise: bool,
    pub block: bool,
}

#[derive(Clone, Debug)]
pub struct Window {
    pub id: WindowId,
    pub buffer: BufferId,
    pub cursor: BufPos,
    pub viewport: Viewport,
    pub options: WindowOptions,
    pub width: u16,
    pub height: u16,
    pub statusline_rows: u16,
    pub last_cursor: BufPos,
    pub mode: UiMode,
    pub search_highlight: Option<(u32, u32)>,
    pub visual: Option<VisualSelection>,
    /// Window-local fold state, maintained by the editor.
    pub folds: FoldState,
    /// Window-local diff state, maintained by the editor.
    pub diff: DiffState,
    /// Signs displayed in this window, maintained by the editor.
    pub signs: SignState,
}

impl Window {
    pub fn new(id: WindowId, buffer: BufferId, size: Size) -> Self {
        Self {
            id,
            buffer,
            cursor: BufPos::start(),
            viewport: Viewport {
                topline: 1,
                topfill: 0,
                leftcol: 0,
                skipcol: 0,
                curswant: 0,
                width: size.cols,
                height: size.rows,
            },
            options: WindowOptions::default(),
            width: size.cols,
            height: size.rows.saturating_sub(1),
            statusline_rows: 1,
            last_cursor: BufPos::start(),
            mode: UiMode::Normal,
            search_highlight: None,
            visual: None,
            folds: FoldState::default(),
            diff: DiffState::default(),
            signs: SignState::default(),
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.viewport.width = width;
        self.viewport.height = height;
    }

    pub fn set_cursor(&mut self, pos: BufPos) {
        self.last_cursor = self.cursor;
        self.cursor = pos;
    }

    /// Adjust topline/leftcol so the cursor is visible.
    pub fn ensure_cursor_visible(&mut self, host: &dyn RenderHost) {
        let text_height = self.height.max(1) as u32;
        let text_width = self.text_width();
        let screen = rvim_edit_screen_col(host, self.buffer, self.cursor, self.options.tabstop);
        let cursor_lnum = self.displayed_cursor_lnum();
        let cursor_hidden = self.folds.is_hidden(self.cursor.lnum);
        if self.folds.is_hidden(self.viewport.topline) {
            self.viewport.topline = self.folds.owner(self.viewport.topline).max(1);
        }
        if self.options.wrap && text_width > 0 {
            if cursor_lnum < self.viewport.topline {
                self.viewport.topline = cursor_lnum;
            }
            let width = text_width as usize;
            let mut row = 0usize;
            for lnum in self.viewport.topline..cursor_lnum {
                if self.folds.is_hidden(lnum) {
                    if self.folds.owner(lnum) == lnum {
                        row += 1 + usize::from(self.diff.filler_above(lnum));
                    }
                    continue;
                }
                row += usize::from(self.diff.filler_above(lnum));
                let (above_rows, below_rows) = crate::view::virtual_rows_for_line(self, host, lnum);
                row += (above_rows + below_rows) as usize;
                let line = host.buffer_line(self.buffer, lnum).unwrap_or(b"");
                let display = rvim_types::unicode::display_width(line, 0, self.options.tabstop);
                row += display.div_ceil(width).max(1);
            }
            if !cursor_hidden {
                row += crate::view::virtual_rows_for_line(self, host, cursor_lnum).0 as usize;
                row += (screen as usize) / width;
            }
            if row >= text_height as usize {
                self.viewport.topline = cursor_lnum;
            }
            self.viewport.curswant = screen;
            return;
        }
        if self.folds.is_hidden(self.viewport.topline) {
            self.viewport.topline = self.folds.owner(self.viewport.topline).max(1);
        }
        let scrolloff = (self.options.scrolloff as u32).min(text_height.saturating_sub(1) / 2);
        if cursor_lnum < self.viewport.topline + scrolloff {
            self.viewport.topline = cursor_lnum.saturating_sub(scrolloff).max(1);
        } else if cursor_lnum + scrolloff >= self.viewport.topline + text_height {
            self.viewport.topline = cursor_lnum
                .saturating_sub(text_height.saturating_sub(scrolloff).saturating_sub(1))
                .max(1);
        }
        if self.folds.is_hidden(self.viewport.topline) {
            self.viewport.topline = self.folds.owner(self.viewport.topline).max(1);
        }
        if screen < self.viewport.leftcol {
            self.viewport.leftcol = screen;
        } else if screen >= self.viewport.leftcol + text_width as u32 {
            self.viewport.leftcol = screen.saturating_sub(text_width as u32 - 1);
        }
        self.viewport.curswant = screen;
    }

    /// Width of the sign column outside the number column.
    ///
    /// "number" shares the number column when line numbers are displayed;
    /// otherwise it behaves like "auto". "auto" only reserves cells when the
    /// buffer actually has a sign.
    pub fn signcolumn_width(&self) -> u16 {
        match self.options.signcolumn.as_str() {
            "no" => 0,
            "yes" => 2,
            "number" => {
                if self.options.number || self.options.relativenumber {
                    0
                } else if self.signs.present {
                    2
                } else {
                    0
                }
            }
            _ => {
                if self.signs.present {
                    2
                } else {
                    0
                }
            }
        }
    }

    /// True when signs are painted inside the number column.
    pub fn signs_in_number_column(&self) -> bool {
        self.options.signcolumn == "number" && (self.options.number || self.options.relativenumber)
    }

    /// Line shown for the cursor: the start line when the cursor is inside a
    /// closed fold (Vim never leaves the cursor on a hidden line).
    pub fn displayed_cursor_lnum(&self) -> u32 {
        if self.folds.is_hidden(self.cursor.lnum) {
            self.folds.owner(self.cursor.lnum).max(1)
        } else {
            self.cursor.lnum
        }
    }

    pub fn text_width(&self) -> u16 {
        let number = if self.options.number || self.options.relativenumber {
            self.options.numberwidth as u16 + 1
        } else {
            0
        };
        let signs = self.signcolumn_width();
        let folds = self.options.foldcolumn as u16;
        self.width.saturating_sub(number + signs + folds)
    }

    pub fn status_height(&self) -> u16 {
        self.statusline_rows
    }
}

fn rvim_edit_screen_col(
    host: &dyn RenderHost,
    buffer: BufferId,
    pos: BufPos,
    tabstop: usize,
) -> u32 {
    host.screen_col(buffer, pos, tabstop)
}
