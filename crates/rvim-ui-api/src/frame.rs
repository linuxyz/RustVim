use crate::attr::{Attr, AttrFlags};
use crate::geometry::{Rect, Size};
use std::collections::HashMap;

pub const UI_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GridId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// One grapheme cluster. Empty for a continuation cell or a blank.
    pub text: String,
    /// 1 or 2 for a leading cell; 0 for the continuation of a wide cell.
    pub width: u8,
    pub attrs: u32,
}

impl Cell {
    pub fn blank() -> Self {
        Self {
            text: String::new(),
            width: 1,
            attrs: 0,
        }
    }

    pub fn wide_cont() -> Self {
        Self {
            text: String::new(),
            width: 0,
            attrs: 0,
        }
    }

    pub fn is_blank(&self) -> bool {
        self.width != 0 && self.text.is_empty()
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::blank()
    }
}

#[derive(Clone, Debug)]
pub struct Grid {
    pub id: GridId,
    pub size: Size,
    pub cells: Vec<Cell>,
    pub attrs: Vec<Attr>,
    attr_index: HashMap<Attr, u32>,
}

impl Grid {
    pub fn new(id: GridId, size: Size) -> Self {
        Self {
            id,
            size,
            cells: vec![Cell::default(); size.area()],
            attrs: vec![Attr::DEFAULT],
            attr_index: HashMap::from([(Attr::DEFAULT, 0)]),
        }
    }

    pub fn idx(&self, row: u16, col: u16) -> usize {
        row as usize * self.size.cols as usize + col as usize
    }

    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        if row >= self.size.rows || col >= self.size.cols {
            None
        } else {
            self.cells.get(self.idx(row, col))
        }
    }

    pub fn attr(&self, cell: &Cell) -> Attr {
        self.attrs
            .get(cell.attrs as usize)
            .copied()
            .unwrap_or_default()
    }

    pub fn intern(&mut self, attr: Attr) -> u32 {
        if let Some(id) = self.attr_index.get(&attr) {
            return *id;
        }
        let id = self.attrs.len() as u32;
        self.attrs.push(attr);
        self.attr_index.insert(attr, id);
        id
    }

    pub fn put_cell(&mut self, row: u16, col: u16, cell: &Cell, attr: Attr) {
        if row >= self.size.rows || col >= self.size.cols {
            return;
        }
        let index = self.idx(row, col);
        let attrs = self.intern(attr);
        self.cells[index] = Cell {
            text: cell.text.clone(),
            width: cell.width,
            attrs,
        };
    }

    pub fn blit_from(&mut self, source: &Grid, origin: Rect) {
        for row in 0..source.size.rows {
            for col in 0..source.size.cols {
                let dest_row = origin.row + row;
                let dest_col = origin.col + col;
                if dest_row >= self.size.rows || dest_col >= self.size.cols {
                    continue;
                }
                let Some(cell) = source.cell(row, col) else {
                    continue;
                };
                self.put_cell(dest_row, dest_col, cell, source.attr(cell));
            }
        }
    }

    pub fn resize(&mut self, size: Size) {
        if size == self.size {
            return;
        }
        let mut cells = vec![Cell::default(); size.area()];
        let rows = size.rows.min(self.size.rows);
        let cols = size.cols.min(self.size.cols);
        for row in 0..rows {
            for col in 0..cols {
                cells[row as usize * size.cols as usize + col as usize] =
                    self.cells[self.idx(row, col)].clone();
            }
        }
        self.size = size;
        self.cells = cells;
    }

    pub fn clear(&mut self, attr: Attr) {
        let id = self.intern(attr);
        for cell in &mut self.cells {
            *cell = Cell {
                text: String::new(),
                width: 1,
                attrs: id,
            };
        }
    }

    pub fn fill(&mut self, rect: Rect, ch: char, attr: Attr) {
        let id = self.intern(attr);
        let width = crate::frame::char_cells(ch);
        for row in rect.row..rect.bottom().min(self.size.rows) {
            for col in rect.col..rect.right().min(self.size.cols) {
                let index = self.idx(row, col);
                self.cells[index] = Cell {
                    text: ch.to_string(),
                    width: width as u8,
                    attrs: id,
                };
            }
        }
    }

    /// Write a string of characters into a row, one cell per display column.
    /// Returns the number of cells consumed. Wide clusters reserve a
    /// continuation cell.
    pub fn put_str(&mut self, row: u16, col: u16, text: &str, attr: Attr) -> u16 {
        if row >= self.size.rows || col >= self.size.cols {
            return 0;
        }
        let id = self.intern(attr);
        let mut x = col;
        let mut last_leading: Option<usize> = None;
        for ch in text.chars() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            let width = char_cells(ch);
            if width == 0 {
                // Combining mark: attach to the previous cell when possible.
                if let Some(index) = last_leading {
                    self.cells[index].text.push(ch);
                }
                continue;
            }
            if x + width as u16 > self.size.cols {
                break;
            }
            let index = self.idx(row, x);
            self.cells[index] = Cell {
                text: ch.to_string(),
                width: width as u8,
                attrs: id,
            };
            last_leading = Some(index);
            if width == 2 && x + 1 < self.size.cols {
                let cont = self.idx(row, x + 1);
                self.cells[cont] = Cell {
                    text: String::new(),
                    width: 0,
                    attrs: id,
                };
            }
            x += width as u16;
        }
        x - col
    }

    pub fn put_bytes_lossy(&mut self, row: u16, col: u16, bytes: &[u8], attr: Attr) -> u16 {
        let text = String::from_utf8_lossy(bytes);
        self.put_str(row, col, &text, attr)
    }

    pub fn row_text(&self, row: u16) -> String {
        if row >= self.size.rows {
            return String::new();
        }
        let mut out = String::new();
        for col in 0..self.size.cols {
            let cell = &self.cells[self.idx(row, col)];
            if cell.width == 0 {
                continue;
            }
            if cell.text.is_empty() {
                out.push(' ');
            } else {
                out.push_str(&cell.text);
            }
        }
        out
    }

    pub fn to_plain_text(&self) -> String {
        (0..self.size.rows)
            .map(|r| self.row_text(r))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn char_cells(ch: char) -> usize {
    let cp = ch as u32;
    if (0x1100..=0x115f).contains(&cp)
        || (0x2e80..=0xa4cf).contains(&cp)
        || (0xac00..=0xd7a3).contains(&cp)
        || (0xf900..=0xfaff).contains(&cp)
        || (0xfe10..=0xfe19).contains(&cp)
        || (0xfe30..=0xfe6f).contains(&cp)
        || (0xff00..=0xff60).contains(&cp)
        || (0xffe0..=0xffe6).contains(&cp)
        || (0x1f300..=0x1f64f).contains(&cp)
        || (0x1f900..=0x1f9ff).contains(&cp)
        || (0x20000..=0x2fffd).contains(&cp)
        || (0x30000..=0x3fffd).contains(&cp)
    {
        2
    } else if cp < 0x20 || (0x7f..0xa0).contains(&cp) {
        0
    } else {
        1
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Base,
    Popup,
    Overlay,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub id: GridId,
    pub rect: Rect,
    pub z: i16,
    pub kind: LayerKind,
    pub grid: Grid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Vertical { thickness: u8 },
    Horizontal { thickness: u8 },
    Underline { thickness: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub grid: GridId,
    pub row: u16,
    pub col: u16,
    pub shape: CursorShape,
    pub blink: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MouseShape {
    #[default]
    Default,
    Arrow,
    IBeam,
    Crosshair,
    Hand,
    ResizeHorizontal,
    ResizeVertical,
    ResizeUpLeftDownRight,
    ResizeUpRightDownLeft,
    NotAllowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UiMode {
    #[default]
    Normal,
    Insert,
    Replace,
    Visual,
    Select,
    Command,
    More,
    Terminal,
}

/// Clickable statusline/tabline region declared with `%[FuncName]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClickRegion {
    pub rect: Rect,
    pub function: String,
    pub minwid: u32,
    pub area: ClickArea,
    pub winid: u64,
    pub tabnr: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickArea {
    Statusline,
    Tabline,
    Tabpanel,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub seq: u64,
    pub size: Size,
    pub layers: Vec<Layer>,
    pub cursor: Option<Cursor>,
    pub mouse_shape: MouseShape,
    pub title: Option<String>,
    pub bell: bool,
    pub mode: UiMode,
    pub clipboard: Option<crate::clipboard::ClipboardRequest>,
    pub hit_regions: Vec<crate::frame::HitRegion>,
    /// Interactive `%[FuncName]` regions; checked before `hit_regions`.
    pub clicks: Vec<ClickRegion>,
    pub dirty_rows: Vec<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKind {
    Window,
    WindowBorder,
    Statusline,
    Tab,
    Sign,
    Fold,
    PopupItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActionId {
    pub kind: ActionKind,
    pub id: u64,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitRegion {
    pub rect: Rect,
    pub action: ActionId,
}

impl Frame {
    pub fn new(seq: u64, size: Size) -> Self {
        let base = Grid::new(GridId(1), size);
        Self {
            seq,
            size,
            layers: vec![Layer {
                id: GridId(1),
                rect: Rect::from_size(size),
                z: 0,
                kind: LayerKind::Base,
                grid: base,
            }],
            cursor: None,
            mouse_shape: MouseShape::Default,
            title: None,
            bell: false,
            mode: UiMode::Normal,
            clipboard: None,
            hit_regions: Vec::new(),
            clicks: Vec::new(),
            dirty_rows: Vec::new(),
        }
    }

    pub fn composite(&self) -> Grid {
        let mut layers: Vec<&Layer> = self.layers.iter().collect();
        layers.sort_by_key(|layer| layer.z);
        let base = layers
            .iter()
            .find(|layer| layer.kind == LayerKind::Base)
            .map(|layer| &layer.grid)
            .unwrap_or(&self.layers[0].grid);
        let mut grid = base.clone();
        for layer in layers
            .into_iter()
            .filter(|layer| layer.kind != LayerKind::Base)
        {
            grid.blit_from(&layer.grid, layer.rect);
        }
        grid
    }

    pub fn compute_dirty_rows(&mut self, previous: Option<&Frame>) {
        let current = self.composite();
        let Some(previous) = previous else {
            self.dirty_rows = (0..self.size.rows).collect();
            return;
        };
        let old = previous.composite();
        let mut rows = Vec::new();
        for row in 0..self.size.rows {
            if row >= old.size.rows
                || (0..self.size.cols).any(|col| {
                    current.cell(row, col) != old.cell(row, col)
                        || current.attr(current.cell(row, col).unwrap_or(&Cell::default()))
                            != old.attr(old.cell(row, col).unwrap_or(&Cell::default()))
                })
            {
                rows.push(row);
            }
        }
        self.dirty_rows = rows;
    }

    pub fn base(&self) -> &Grid {
        &self.layers[0].grid
    }

    pub fn base_mut(&mut self) -> &mut Grid {
        &mut self.layers[0].grid
    }

    pub fn cursor_row_col(&self) -> Option<(u16, u16)> {
        self.cursor.map(|c| (c.row, c.col))
    }
}

#[allow(dead_code)]
fn _flags_are_used(_: AttrFlags) {}
