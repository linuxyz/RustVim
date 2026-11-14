use rvim_types::{BufPos, BufferId, WindowId};
use rvim_ui_api::{Size, UiMode};

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
        if self.options.wrap && text_width > 0 {
            if self.cursor.lnum < self.viewport.topline {
                self.viewport.topline = self.cursor.lnum;
            }
            let width = text_width as usize;
            let mut row = 0usize;
            for lnum in self.viewport.topline..self.cursor.lnum {
                let line = host.buffer_line(self.buffer, lnum).unwrap_or(b"");
                let display = rvim_types::unicode::display_width(line, 0, self.options.tabstop);
                row += display.div_ceil(width).max(1);
            }
            row += (screen as usize) / width;
            if row >= text_height as usize {
                self.viewport.topline = self.cursor.lnum;
            }
            self.viewport.curswant = screen;
            return;
        }
        let scrolloff = (self.options.scrolloff as u32).min(text_height.saturating_sub(1) / 2);
        if self.cursor.lnum < self.viewport.topline + scrolloff {
            self.viewport.topline = self.cursor.lnum.saturating_sub(scrolloff).max(1);
        } else if self.cursor.lnum + scrolloff >= self.viewport.topline + text_height {
            self.viewport.topline = self
                .cursor
                .lnum
                .saturating_sub(text_height.saturating_sub(scrolloff).saturating_sub(1))
                .max(1);
        }
        if screen < self.viewport.leftcol {
            self.viewport.leftcol = screen;
        } else if screen >= self.viewport.leftcol + text_width as u32 {
            self.viewport.leftcol = screen.saturating_sub(text_width as u32 - 1);
        }
        self.viewport.curswant = screen;
    }

    /// Width reserved by the sign column.  RVim has no sign API yet, so
    /// "auto" reserves nothing and the other modes reserve their Vim widths.
    pub fn signcolumn_width(&self) -> u16 {
        match self.options.signcolumn.as_str() {
            "yes" => 2,
            "number" => self.options.numberwidth as u16 + 1,
            _ => 0,
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
