use rvim_types::unicode::{char_at, char_width};
use rvim_types::{BufPos, BufferId, TabId, WindowId};
use rvim_ui_api::{
    ActionId, ActionKind, Attr, ClickArea, ClickRegion, Cursor, CursorShape, Frame, Grid, GridId,
    HitRegion, Layer, LayerKind, MouseShape, Rect, Size, UiMode,
};

use crate::layout::{compute_layout, LayoutNode, SplitDir};
use crate::window::{Window, WindowOptions};

/// Information the presentation layer needs from the editor. It is deliberately
/// read-only: rendering never invokes an Ex command or mutates a buffer.
pub trait RenderHost {
    fn buffer_line(&self, buffer: BufferId, lnum: u32) -> Option<&[u8]>;
    fn buffer_line_count(&self, buffer: BufferId) -> u32;
    fn buffer_name(&self, buffer: BufferId) -> Option<String>;
    fn buffer_modified(&self, buffer: BufferId) -> bool;
    fn buffer_readonly(&self, _buffer: BufferId) -> bool {
        false
    }
    fn buffer_filetype(&self, _buffer: BufferId) -> String {
        String::new()
    }
    fn search_matches(&self, _buffer: BufferId, _lnum: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
    fn line_styles(&self, _buffer: BufferId, _lnum: u32) -> Vec<(u32, u32, Attr)> {
        Vec::new()
    }
    fn statusline_template(&self, _window: WindowId) -> Option<String> {
        None
    }
    fn mode(&self) -> UiMode;
    fn mode_text(&self) -> String;
    fn command_text(&self) -> Option<&str>;
    fn message_text(&self) -> Option<&str>;

    fn showcmd_text(&self) -> Option<String> {
        None
    }

    fn showmode_text(&self) -> Option<String> {
        None
    }

    fn ruler_text(&self) -> Option<String> {
        None
    }

    fn message_highlight(&self) -> &str {
        "WarningMsg"
    }
    fn message_popup(&self) -> Option<(Rect, String)> {
        None
    }
    fn completion_popup(&self) -> Option<(Rect, String)> {
        None
    }
    fn tabline_text(&self) -> Option<String>;
    fn tabline_regions(&self) -> Vec<(u16, u16, u64)> {
        Vec::new()
    }
    fn highlight(&self, group: &str) -> Attr;
    fn screen_col(&self, buffer: BufferId, pos: BufPos, tabstop: usize) -> u32;
    fn line_attr(&self, buffer: BufferId, lnum: u32, col: u32) -> Attr {
        let _ = (buffer, lnum, col);
        self.highlight("Normal")
    }
    fn cursor_shape(&self, mode: UiMode) -> CursorShape {
        match mode {
            UiMode::Insert => CursorShape::Vertical { thickness: 2 },
            UiMode::Replace => CursorShape::Underline { thickness: 1 },
            UiMode::Command => CursorShape::Vertical { thickness: 2 },
            _ => CursorShape::Block,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Chrome {
    pub tabline_rows: u16,
    pub cmdline_rows: u16,
    pub message_rows: u16,
}

#[derive(Clone, Debug)]
pub struct TabPage {
    pub id: TabId,
    pub root: LayoutNode,
    pub current: WindowId,
    pub cwd: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ViewState {
    pub windows: crate::arena_map::ArenaMap<WindowId, Window>,
    pub tabs: crate::arena_map::ArenaMap<TabId, TabPage>,
    pub tab_order: Vec<TabId>,
    pub current_tab: TabId,
    pub screen: Size,
    pub seq: u64,
    pub last_frame: Option<Frame>,
    pub global_statusline: bool,
    pub global_statusline_rows: u16,
    next_window: u64,
    next_tab: u64,
}

impl ViewState {
    pub fn new(screen: Size, buffer: BufferId) -> Self {
        let mut windows = crate::arena_map::ArenaMap::new();
        let mut tabs = crate::arena_map::ArenaMap::new();
        let window_id = WindowId::from_raw((1u64 << 32) | 1);
        let tab_id = TabId::from_raw((1u64 << 32) | 1);
        let window = Window::new(
            window_id,
            buffer,
            Size::new(screen.rows.saturating_sub(1), screen.cols),
        );
        windows.insert(window_id, window);
        tabs.insert(
            tab_id,
            TabPage {
                id: tab_id,
                root: LayoutNode::leaf(window_id),
                current: window_id,
                cwd: None,
            },
        );
        Self {
            windows,
            tabs,
            tab_order: vec![tab_id],
            current_tab: tab_id,
            screen,
            seq: 0,
            last_frame: None,
            global_statusline: false,
            global_statusline_rows: 0,
            next_window: 2,
            next_tab: 2,
        }
    }

    pub fn new_tab(&mut self, buffer: BufferId) -> TabId {
        let id = TabId::from_raw((1u64 << 32) | self.next_tab);
        self.next_tab += 1;
        let window_id = WindowId::from_raw((1u64 << 32) | self.next_window);
        self.next_window += 1;
        let size = Size::new(self.screen.rows.saturating_sub(1), self.screen.cols);
        let window = Window::new(window_id, buffer, size);
        self.windows.insert(window_id, window);
        self.tabs.insert(
            id,
            TabPage {
                id,
                root: LayoutNode::leaf(window_id),
                current: window_id,
                cwd: None,
            },
        );
        self.tab_order.push(id);
        self.current_tab = id;
        id
    }

    pub fn next_tab(&mut self) -> TabId {
        let ids: Vec<TabId> = self.tab_ids();
        if ids.is_empty() {
            return self.current_tab;
        }
        let index = ids
            .iter()
            .position(|id| *id == self.current_tab)
            .unwrap_or(0);
        let next = ids[(index + 1) % ids.len()];
        self.current_tab = next;
        next
    }

    pub fn previous_tab(&mut self) -> TabId {
        let ids: Vec<TabId> = self.tab_ids();
        if ids.is_empty() {
            return self.current_tab;
        }
        let index = ids
            .iter()
            .position(|id| *id == self.current_tab)
            .unwrap_or(0);
        let previous = ids[(index + ids.len() - 1) % ids.len()];
        self.current_tab = previous;
        previous
    }

    pub fn close_current_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        let Some(tab) = self.tabs.remove(&self.current_tab) else {
            return false;
        };
        for window in crate::layout::all_windows(&tab.root) {
            self.windows.remove(&window);
        }
        self.tab_order.retain(|item| *item != tab.id);
        self.current_tab = self.tab_ids().first().copied().unwrap_or(self.current_tab);
        true
    }

    pub fn set_tab(&mut self, tab: TabId) -> bool {
        if self.tabs.contains_key(&tab) {
            self.current_tab = tab;
            true
        } else {
            false
        }
    }

    pub fn window_rects(
        &self,
        chrome: Chrome,
        host: &dyn RenderHost,
    ) -> Vec<crate::layout::LayoutRect> {
        self.compute_rects(chrome, host)
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn tab_ids(&self) -> Vec<TabId> {
        let mut order: Vec<TabId> = self
            .tab_order
            .iter()
            .copied()
            .filter(|id| self.tabs.contains_key(id))
            .collect();
        for id in self.tabs.keys() {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        order
    }

    pub fn move_current_tab(&mut self, target: usize) {
        let ids = self.tab_ids();
        let Some(index) = ids.iter().position(|id| *id == self.current_tab) else {
            return;
        };
        let target = target.min(ids.len().saturating_sub(1));
        if index == target {
            return;
        }
        let current = self.current_tab;
        self.tab_order = ids;
        self.tab_order.remove(index);
        self.tab_order.insert(target, current);
    }

    pub fn window_ids_in_current_tab(&self) -> Vec<WindowId> {
        self.tabs
            .get(&self.current_tab)
            .map(|tab| crate::layout::all_windows(&tab.root))
            .unwrap_or_default()
    }

    pub fn equalize_windows(&mut self) {
        if let Some(tab) = self.tabs.get_mut(&self.current_tab) {
            equalize_node(&mut tab.root);
        }
    }

    pub fn resize_window(&mut self, target: WindowId, vertical: bool, delta: i32) -> bool {
        let Some(tab) = self.tabs.get_mut(&self.current_tab) else {
            return false;
        };
        adjust_split(&mut tab.root, target, vertical, delta)
    }

    pub fn only_current_window(&mut self) -> bool {
        let Some(tab) = self.tabs.get_mut(&self.current_tab) else {
            return false;
        };
        let current = tab.current;
        let keep = self.windows.contains_key(&current);
        if !keep {
            return false;
        }
        let old = crate::layout::all_windows(&tab.root);
        for window in old {
            if window != current {
                self.windows.remove(&window);
            }
        }
        tab.root = LayoutNode::leaf(current);
        true
    }

    pub fn only_current_tab(&mut self) -> bool {
        let current = self.current_tab;
        let remove: Vec<TabId> = self
            .tab_ids()
            .into_iter()
            .filter(|id| *id != current)
            .collect();
        for tab in remove {
            if let Some(page) = self.tabs.remove(&tab) {
                self.tab_order.retain(|item| *item != tab);
                for window in crate::layout::all_windows(&page.root) {
                    self.windows.remove(&window);
                }
            }
        }
        self.tabs.contains_key(&current)
    }

    pub fn current_tab(&self) -> Option<&TabPage> {
        self.tabs.get(&self.current_tab)
    }

    pub fn current_window(&self) -> Option<&Window> {
        let tab = self.current_tab()?;
        self.windows.get(&tab.current)
    }

    pub fn current_window_mut(&mut self) -> Option<&mut Window> {
        let tab = self.tabs.get(&self.current_tab)?;
        self.windows.get_mut(&tab.current)
    }

    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.get_mut(&id)
    }

    pub fn set_current_window(&mut self, id: WindowId) -> bool {
        if !self.windows.contains_key(&id) {
            return false;
        }
        if let Some(tab) = self.tabs.get_mut(&self.current_tab) {
            tab.current = id;
            true
        } else {
            false
        }
    }

    pub fn windows_in_tab(&self, tab: TabId) -> Vec<WindowId> {
        self.tabs
            .get(&tab)
            .map(|tab| crate::layout::all_windows(&tab.root))
            .unwrap_or_default()
    }

    pub fn split(&mut self, buffer: BufferId, dir: SplitDir) -> Option<WindowId> {
        self.split_ordered(buffer, dir, true)
    }

    pub fn split_ordered(
        &mut self,
        buffer: BufferId,
        dir: SplitDir,
        after: bool,
    ) -> Option<WindowId> {
        let tab = self.tabs.get(&self.current_tab)?;
        let current = tab.current;
        let window = self.windows.get(&current)?;
        let size = Size::new(window.height, window.width);
        let id = WindowId::from_raw((1u64 << 32) | self.next_window);
        self.next_window += 1;
        let new_window = Window::new(id, buffer, size);
        self.windows.insert(id, new_window);
        let tab = self.tabs.get_mut(&self.current_tab)?;
        replace_leaf(&mut tab.root, current, dir, id, after);
        tab.current = id;
        Some(id)
    }

    pub fn close_current_window(&mut self) -> bool {
        let Some(tab) = self.tabs.get(&self.current_tab) else {
            return false;
        };
        if tab.root == LayoutNode::leaf(tab.current) {
            return false;
        }
        let current = tab.current;
        let tab = self.tabs.get_mut(&self.current_tab).unwrap();
        remove_leaf(&mut tab.root, current);
        let remaining = crate::layout::all_windows(&tab.root);
        tab.current = *remaining.first().unwrap_or(&current);
        self.windows.remove(&current);
        true
    }

    /// Resize the split so `target`'s text area has `rows` rows.
    pub fn set_window_text_height(&mut self, target: WindowId, rows: u16) -> bool {
        let extra = self.global_statusline_rows;
        let area = Rect::new(
            0,
            0,
            self.screen.rows.saturating_sub(extra),
            self.screen.cols,
        );
        let status_height = if self.global_statusline {
            0
        } else {
            self.windows
                .get(&target)
                .map(|window| window.status_height())
                .unwrap_or(1)
        };
        let Some(tab) = self.tabs.get_mut(&self.current_tab) else {
            return false;
        };
        crate::layout::set_text_height(&mut tab.root, area, status_height, target, rows)
    }
    pub fn resize(&mut self, size: Size) {
        self.screen = size;
    }

    pub fn layout_rects(&self, top: u16, bottom: u16) -> Vec<crate::layout::LayoutRect> {
        let Some(tab) = self.tabs.get(&self.current_tab) else {
            return Vec::new();
        };
        let extra = self.global_statusline_rows;
        let area = Rect::new(
            top,
            0,
            self.screen.rows.saturating_sub(top + bottom + extra),
            self.screen.cols,
        );
        let current = tab.current;
        let status_height = if self.global_statusline {
            0
        } else {
            self.windows
                .get(&current)
                .map(|w| w.status_height())
                .unwrap_or(1)
        };
        crate::layout::compute_layout(&tab.root, area, status_height)
    }

    pub fn tabline_rows(&self, chrome: Chrome, host: &dyn RenderHost) -> u16 {
        if chrome.tabline_rows > 0 && host.tabline_text().is_some() {
            1
        } else {
            0
        }
    }

    pub fn chrome_rows(&self, chrome: Chrome, host: &dyn RenderHost) -> u16 {
        let mut rows = self.tabline_rows(chrome, host);
        rows += chrome.cmdline_rows;
        rows += chrome.message_rows;
        rows
    }

    pub fn compute_rects(
        &self,
        chrome: Chrome,
        host: &dyn RenderHost,
    ) -> Vec<crate::layout::LayoutRect> {
        let Some(tab) = self.tabs.get(&self.current_tab) else {
            return Vec::new();
        };
        let top = self.tabline_rows(chrome, host);
        let bottom = chrome.cmdline_rows + chrome.message_rows;
        let global_extra = u16::from(self.global_statusline);
        let area = Rect::new(
            top,
            0,
            self.screen.rows.saturating_sub(top + bottom + global_extra),
            self.screen.cols,
        );
        let current = tab.current;
        let status_height = if self.global_statusline {
            0
        } else {
            self.windows
                .get(&current)
                .map(|w| w.status_height())
                .unwrap_or(1)
        };
        compute_layout(&tab.root, area, status_height)
    }

    pub fn render(&mut self, host: &dyn RenderHost, chrome: Chrome) -> Frame {
        self.seq += 1;
        let mut frame = Frame::new(self.seq, self.screen);
        frame.mode = host.mode();
        let normal = host.highlight("Normal");
        frame.base_mut().clear(normal);

        let tabline_rows = self.tabline_rows(chrome, host);
        if tabline_rows > 0 {
            if let Some(text) = host.tabline_text() {
                let attr = host.highlight("TabLine");
                frame
                    .base_mut()
                    .fill(Rect::new(0, 0, 1, self.screen.cols), ' ', attr);
                frame.base_mut().put_str(0, 0, &text, attr);
            }
        }

        let rects = self.compute_rects(chrome, host);
        for rect in &rects {
            self.render_window(&mut frame, rect, host);
            frame.hit_regions.push(HitRegion {
                rect: rect.text,
                action: ActionId {
                    kind: ActionKind::Window,
                    id: rvim_types::Id::raw(rect.window),
                    index: 0,
                },
            });
            if rect.status.rows > 0 {
                frame.hit_regions.push(HitRegion {
                    rect: rect.status,
                    action: ActionId {
                        kind: ActionKind::Statusline,
                        id: rvim_types::Id::raw(rect.window),
                        index: 0,
                    },
                });
            }
            if let Some(separator) = rect.separator {
                frame.hit_regions.push(HitRegion {
                    rect: separator,
                    action: ActionId {
                        kind: ActionKind::WindowBorder,
                        id: rvim_types::Id::raw(rect.window),
                        index: 0,
                    },
                });
            }
        }
        if tabline_rows > 0 {
            let regions = host.tabline_regions();
            if regions.is_empty() {
                frame.hit_regions.push(HitRegion {
                    rect: Rect::new(0, 0, tabline_rows, self.screen.cols),
                    action: ActionId {
                        kind: ActionKind::Tab,
                        id: rvim_types::Id::raw(self.current_tab),
                        index: 0,
                    },
                });
            } else {
                for (start, end, tab) in regions {
                    frame.hit_regions.push(HitRegion {
                        rect: Rect::new(0, start, tabline_rows, end.saturating_sub(start)),
                        action: ActionId {
                            kind: ActionKind::Tab,
                            id: tab,
                            index: 0,
                        },
                    });
                }
            }
        }
        if self.global_statusline {
            let bottom = chrome.cmdline_rows + chrome.message_rows;
            let rows = self.global_statusline_rows.max(1);
            let start = self.screen.rows.saturating_sub(bottom + rows);
            if let Some(window) = self.current_window() {
                let layout =
                    Self::statusline_layout(window, host, self.screen.cols.saturating_sub(1));
                let attr = host.highlight("StatusLine");
                for (index, text) in layout.text.split('\n').enumerate().take(rows as usize) {
                    let row = start + index as u16;
                    frame
                        .base_mut()
                        .fill(Rect::new(row, 0, 1, self.screen.cols), ' ', attr);
                    frame.base_mut().put_str(row, 0, text, attr);
                    frame.hit_regions.push(HitRegion {
                        rect: Rect::new(row, 0, 1, self.screen.cols),
                        action: ActionId {
                            kind: ActionKind::Statusline,
                            id: rvim_types::Id::raw(window.id),
                            index: 0,
                        },
                    });
                }
                for click in &layout.clicks {
                    let width = click
                        .end
                        .saturating_sub(click.start)
                        .min(self.screen.cols.saturating_sub(click.start));
                    if width == 0 {
                        continue;
                    }
                    frame.clicks.push(ClickRegion {
                        rect: Rect::new(start, click.start, 1, width),
                        function: click.function.clone(),
                        minwid: click.minwid,
                        area: ClickArea::Statusline,
                        winid: rvim_types::Id::raw(window.id),
                        tabnr: 0,
                    });
                }
            }
        }
        self.render_bottom(&mut frame, chrome, host, tabline_rows);

        if let Some((rect, text)) = host.message_popup() {
            let rect = Rect::new(
                rect.row.min(self.screen.rows.saturating_sub(rect.rows)),
                rect.col.min(self.screen.cols.saturating_sub(rect.cols)),
                rect.rows.min(self.screen.rows),
                rect.cols.min(self.screen.cols),
            );
            if rect.rows > 0 && rect.cols > 0 {
                let mut grid = Grid::new(GridId(2), Size::new(rect.rows, rect.cols));
                let attr = host.highlight("Pmenu");
                grid.fill(Rect::new(0, 0, rect.rows, rect.cols), ' ', attr);
                for (line_index, line) in text.lines().enumerate() {
                    if line_index >= rect.rows as usize {
                        break;
                    }
                    grid.put_str(line_index as u16, 1, line, attr);
                }
                frame.layers.push(Layer {
                    id: GridId(2),
                    rect,
                    z: 10,
                    kind: LayerKind::Popup,
                    grid,
                });
                frame.hit_regions.push(HitRegion {
                    rect,
                    action: ActionId {
                        kind: ActionKind::PopupItem,
                        id: 0,
                        index: 0,
                    },
                });
                if let Some(cursor) = frame.cursor {
                    if rect.contains(cursor.row, cursor.col) {
                        frame.cursor = None;
                    }
                }
            }
        }

        if let Some((rect, text)) = host.completion_popup() {
            let rect = Rect::new(
                rect.row.min(self.screen.rows.saturating_sub(rect.rows)),
                rect.col.min(self.screen.cols.saturating_sub(rect.cols)),
                rect.rows.min(self.screen.rows),
                rect.cols.min(self.screen.cols),
            );
            if rect.rows > 0 && rect.cols > 0 {
                let mut grid = Grid::new(GridId(3), Size::new(rect.rows, rect.cols));
                let attr = host.highlight("Pmenu");
                grid.fill(Rect::new(0, 0, rect.rows, rect.cols), ' ', attr);
                for (line_index, line) in text.lines().enumerate() {
                    if line_index >= rect.rows as usize {
                        break;
                    }
                    grid.put_str(line_index as u16, 1, line, attr);
                }
                frame.layers.push(Layer {
                    id: GridId(3),
                    rect,
                    z: 11,
                    kind: LayerKind::Popup,
                    grid,
                });
                frame.hit_regions.push(HitRegion {
                    rect,
                    action: ActionId {
                        kind: ActionKind::PopupItem,
                        id: 1,
                        index: 0,
                    },
                });
                if let Some(cursor) = frame.cursor {
                    if rect.contains(cursor.row, cursor.col) {
                        frame.cursor = None;
                    }
                }
            }
        }

        if let Some(cursor) = frame.cursor {
            let shape = host.cursor_shape(host.mode());
            frame.cursor = Some(Cursor { shape, ..cursor });
        }
        frame.mouse_shape = MouseShape::Arrow;
        frame.title = Some(format!("RVim — {}", host.mode_text()));
        frame.mode = host.mode();
        frame.compute_dirty_rows(self.last_frame.as_ref());
        self.last_frame = Some(frame.clone());
        frame
    }

    fn render_window(
        &mut self,
        frame: &mut Frame,
        rect: &crate::layout::LayoutRect,
        host: &dyn RenderHost,
    ) {
        let active_window = self
            .tabs
            .get(&self.current_tab)
            .is_some_and(|tab| tab.current == rect.window);
        let Some(window) = self.windows.get_mut(&rect.window) else {
            return;
        };
        window.resize(rect.text.cols, rect.text.rows);
        let text_height = rect.text.rows;
        window.height = text_height;
        window.width = rect.text.cols;
        window.viewport.height = text_height;
        window.viewport.width = rect.text.cols;
        window.ensure_cursor_visible(host);

        if host.buffer_line_count(window.buffer) == 0 {
            return;
        }
        let line_nr = host.highlight("LineNr");
        let cursor_line_nr = host.highlight("CursorLineNr");
        let tilde = host.highlight("NonText");
        let cursorline = if window.options.cursorline {
            Some(host.highlight("CursorLine"))
        } else {
            None
        };
        let number_width = if window.options.number || window.options.relativenumber {
            window.options.numberwidth as u16 + 1
        } else {
            0
        };
        let sign_width = window.signcolumn_width();
        let fold_width = window.options.foldcolumn as u16;
        let gutter_width = fold_width + sign_width + number_width;
        let text_width = rect.text.cols.saturating_sub(gutter_width);
        let topline = window.viewport.topline.max(1);
        let leftcol = window.viewport.leftcol;
        let colorcolumn = host.highlight("ColorColumn");
        let cursorcolumn = host.highlight("CursorColumn");

        let wrap = window.options.wrap && text_width > 0;
        let bottom = rect.text.row + text_height;
        let mut screen_row = rect.text.row;
        let mut lnum = topline;
        let mut first_visible = true;
        while screen_row < bottom {
            let line_background = if lnum == window.cursor.lnum {
                cursorline
            } else {
                None
            };
            if lnum > host.buffer_line_count(window.buffer) {
                if !first_visible {
                    frame
                        .base_mut()
                        .put_str(screen_row, rect.text.col + gutter_width, "~", tilde);
                }
                screen_row += 1;
                lnum += 1;
                first_visible = false;
                continue;
            }
            let Some(line) = host.buffer_line(window.buffer, lnum) else {
                break;
            };
            let prefix = continuation_prefix(window, line);
            let prefix_width = text_width_of(&prefix, window.options.tabstop.max(1));
            let rest_width = text_width.saturating_sub(prefix_width).max(1);
            let mut chunks = if wrap {
                wrap_chunks(
                    line,
                    text_width as usize,
                    rest_width as usize,
                    window.options.tabstop.max(1),
                    window.options.linebreak,
                    &window.options.breakat,
                )
            } else {
                vec![(0, line.len())]
            };
            if lnum == topline && window.viewport.skipcol > 0 && !chunks.is_empty() {
                let skipcol = window.viewport.skipcol as usize;
                let tabstop = window.options.tabstop.max(1);
                let mut partial = None;
                for (index, (start, end)) in chunks.iter().enumerate() {
                    let first = rvim_types::unicode::display_width(&line[..*start], 0, tabstop);
                    let last = rvim_types::unicode::display_width(&line[..*end], 0, tabstop);
                    if skipcol >= first && skipcol < last {
                        let offset = byte_at_display_col(line, *start, skipcol - first, tabstop);
                        partial = Some((index, offset));
                        break;
                    }
                }
                if let Some((index, offset)) = partial {
                    let mut visible = vec![(offset, chunks[index].1)];
                    visible.extend_from_slice(&chunks[index + 1..]);
                    chunks = visible;
                } else {
                    let skip = chunks
                        .iter()
                        .position(|(start, _)| {
                            rvim_types::unicode::display_width(&line[..*start], 0, tabstop)
                                >= skipcol
                        })
                        .unwrap_or(chunks.len().saturating_sub(1));
                    chunks.drain(..skip);
                }
            }
            let searches = host.search_matches(window.buffer, lnum);
            let search_attr = host.highlight("Search");
            let styles = host.line_styles(window.buffer, lnum);
            for (chunk_index, (first_byte, last_byte)) in chunks.iter().enumerate() {
                if screen_row >= bottom {
                    break;
                }
                if let Some(attr) = line_background {
                    frame.base_mut().fill(
                        Rect::new(screen_row, rect.text.col, 1, rect.text.cols),
                        ' ',
                        attr,
                    );
                }
                if number_width > 0 {
                    let text = if chunk_index == 0 {
                        let number = if window.options.relativenumber && lnum != window.cursor.lnum
                        {
                            (lnum as i64 - window.cursor.lnum as i64).unsigned_abs() as u32
                        } else {
                            lnum
                        };
                        format_number(number, &window.options.numberformat)
                    } else {
                        String::new()
                    };
                    let padded = format!("{:>width$} ", text, width = (number_width - 1) as usize);
                    let attr = if lnum == window.cursor.lnum {
                        cursor_line_nr
                    } else {
                        line_nr
                    };
                    frame.base_mut().put_str(
                        screen_row,
                        rect.text.col + fold_width + sign_width,
                        &padded,
                        attr,
                    );
                }
                let chunk_col = if chunk_index > 0 {
                    frame.base_mut().put_str(
                        screen_row,
                        rect.text.col + gutter_width,
                        &prefix,
                        host.highlight("NonText"),
                    );
                    rect.text.col + gutter_width + prefix_width
                } else {
                    rect.text.col + gutter_width
                };
                Self::render_line_impl(
                    frame,
                    host,
                    window,
                    lnum,
                    screen_row,
                    chunk_col,
                    if chunk_index > 0 {
                        rest_width
                    } else {
                        text_width
                    },
                    if wrap { 0 } else { leftcol },
                    line,
                    line_background,
                    &searches,
                    search_attr,
                    &styles,
                    *first_byte,
                    *last_byte,
                );
                screen_row += 1;
                first_visible = false;
            }
            lnum += 1;
        }
        if let Some(cursor) =
            Self::cursor_screen_impl(window, host, topline, leftcol, gutter_width, rect)
        {
            if cursor.0 < rect.text.row + text_height {
                frame.cursor = Some(Cursor {
                    grid: GridId(1),
                    row: cursor.0,
                    col: cursor.1,
                    shape: CursorShape::Block,
                    blink: true,
                });
            }
        }

        for column in window.options.colorcolumn.split(',') {
            if let Ok(column) = column.trim().parse::<u32>() {
                if column == 0 {
                    continue;
                }
                let screen = column.saturating_sub(1);
                if screen < text_width as u32 {
                    let col = rect.text.col + gutter_width + screen as u16;
                    for row in 0..text_height {
                        frame
                            .base_mut()
                            .put_str(rect.text.row + row, col, " ", colorcolumn);
                    }
                }
            }
        }
        if window.options.cursorcolumn {
            let screen = host.screen_col(window.buffer, window.cursor, window.options.tabstop);
            if screen < text_width as u32 {
                let col = rect.text.col + number_width + screen as u16;
                for row in 0..text_height {
                    frame
                        .base_mut()
                        .put_str(rect.text.row + row, col, " ", cursorcolumn);
                }
            }
        }

        Self::render_statusline_impl(frame, window, host, rect, active_window);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_line_impl(
        frame: &mut Frame,
        host: &dyn RenderHost,
        window: &Window,
        lnum: u32,
        row: u16,
        start_col: u16,
        width: u16,
        leftcol: u32,
        line: &[u8],
        line_background: Option<Attr>,
        searches: &[(u32, u32)],
        search_attr: Attr,
        styles: &[(u32, u32, Attr)],
        first_byte: usize,
        last_byte: usize,
    ) {
        let normal = merge_attr(host.highlight("Normal"), line_background);
        let visual = merge_attr(host.highlight("Visual"), line_background);
        let tabstop = window.options.tabstop.max(1);
        let list = window.options.list;
        let tab_char = list_char(&window.options.listchars, "tab").unwrap_or('>');
        let trail_char = list_char(&window.options.listchars, "trail").unwrap_or('-');
        let last_nonblank = line
            .iter()
            .rposition(|byte| *byte != b' ' && *byte != b'\t')
            .unwrap_or(0);
        let mut screen = 0u32;
        let mut byte = first_byte.min(line.len());
        let last_byte = last_byte.min(line.len());
        let mut out_col = start_col;
        let max_col = start_col + width;
        while byte < last_byte && out_col < max_col {
            let (ch, len) = if line[byte] == b'\t' {
                ('\t', 1)
            } else if let Some((ch, len)) = char_at(line, byte) {
                (ch, len)
            } else {
                ('\u{FFFD}', 1)
            };
            let width_cells = if ch == '\t' {
                let stop = tabstop;
                stop - ((screen as usize + window.viewport.leftcol as usize) % stop)
            } else {
                char_width(ch).max(1)
            };
            if screen + width_cells as u32 <= leftcol {
                screen += width_cells as u32;
                byte += len;
                continue;
            }
            if screen >= leftcol {
                if ch == '\t' {
                    for index in 0..width_cells {
                        if out_col >= max_col {
                            break;
                        }
                        let text = if list && index == 0 {
                            tab_char.to_string()
                        } else {
                            " ".to_string()
                        };
                        frame.base_mut().put_str(row, out_col, &text, normal);
                        out_col += 1;
                    }
                } else if (ch as u32) < 0x20 {
                    if out_col + 1 < max_col {
                        let attr = selected_attr(
                            host,
                            window,
                            lnum,
                            byte as u32,
                            visual,
                            line_background,
                            searches,
                            search_attr,
                            styles,
                        );
                        frame.base_mut().put_str(row, out_col, "^", attr);
                        let display = char::from_u32((ch as u32 + 0x40) & 0x7f).unwrap_or('?');
                        frame
                            .base_mut()
                            .put_str(row, out_col + 1, &display.to_string(), attr);
                    }
                    out_col += 2;
                } else if width_cells == 2 && out_col + 1 < max_col {
                    let attr = selected_attr(
                        host,
                        window,
                        lnum,
                        byte as u32,
                        visual,
                        line_background,
                        searches,
                        search_attr,
                        styles,
                    );
                    frame
                        .base_mut()
                        .put_str(row, out_col, &ch.to_string(), attr);
                    out_col += 2;
                } else {
                    let attr = selected_attr(
                        host,
                        window,
                        lnum,
                        byte as u32,
                        visual,
                        line_background,
                        searches,
                        search_attr,
                        styles,
                    );
                    let text = if list && ch == ' ' && byte >= last_nonblank {
                        trail_char.to_string()
                    } else {
                        ch.to_string()
                    };
                    frame.base_mut().put_str(row, out_col, &text, attr);
                    out_col += 1;
                }
            }
            screen += width_cells as u32;
            byte += len;
        }
    }

    fn cursor_screen_impl(
        window: &Window,
        host: &dyn RenderHost,
        topline: u32,
        leftcol: u32,
        number_width: u16,
        rect: &crate::layout::LayoutRect,
    ) -> Option<(u16, u16)> {
        if window.cursor.lnum < topline {
            return None;
        }
        let text_height = window.height.max(1);
        if window.options.wrap && window.text_width() > 0 {
            let width = window.text_width() as u32;
            let mut row = 0u32;
            for lnum in topline..window.cursor.lnum {
                if let Some(line) = host.buffer_line(window.buffer, lnum) {
                    let prefix = continuation_prefix(window, line);
                    let prefix_width = text_width_of(&prefix, window.options.tabstop);
                    let rest_width = width as u16 - prefix_width.min(width as u16);
                    let chunks = wrap_chunks(
                        line,
                        width as usize,
                        rest_width.max(1) as usize,
                        window.options.tabstop,
                        window.options.linebreak,
                        &window.options.breakat,
                    );
                    let skipped = if lnum == topline {
                        skipped_chunks(window, line, &chunks)
                    } else {
                        0
                    };
                    row += chunks.len().saturating_sub(skipped) as u32;
                }
            }
            let line = host
                .buffer_line(window.buffer, window.cursor.lnum)
                .unwrap_or(b"");
            let prefix = continuation_prefix(window, line);
            let prefix_width = text_width_of(&prefix, window.options.tabstop);
            let rest_width = (width as u16).saturating_sub(prefix_width).max(1);
            let chunks = wrap_chunks(
                line,
                width as usize,
                rest_width as usize,
                window.options.tabstop,
                window.options.linebreak,
                &window.options.breakat,
            );
            let cursor_col = window.cursor.col as usize;
            let mut chunk_index = 0usize;
            for (index, (first, last)) in chunks.iter().enumerate() {
                if cursor_col >= *first && (cursor_col < *last || index + 1 == chunks.len()) {
                    chunk_index = index;
                    break;
                }
            }
            let skipped = if window.cursor.lnum == topline {
                skipped_chunks(window, line, &chunks)
            } else {
                0
            };
            if chunk_index < skipped {
                return None;
            }
            row += (chunk_index - skipped) as u32;
            let first = chunks.get(chunk_index).map_or(0, |chunk| chunk.0);
            let screen = host.screen_col(window.buffer, window.cursor, window.options.tabstop);
            let chunk_start_screen = rvim_types::unicode::display_width(
                &line[..first.min(line.len())],
                0,
                window.options.tabstop,
            );
            let mut col = screen.saturating_sub(chunk_start_screen as u32);
            if chunk_index == skipped && window.cursor.lnum == topline {
                let skipcol = window.viewport.skipcol;
                col = col.saturating_sub(skipcol.saturating_sub(chunk_start_screen as u32));
            }
            if chunk_index > skipped {
                col += prefix_width as u32;
            }
            if row >= text_height as u32 {
                return None;
            }
            return Some((
                rect.text.row + row as u16,
                rect.text.col + number_width + col as u16,
            ));
        }
        let row = window.cursor.lnum - topline;
        if row >= rect.text.rows as u32 {
            return None;
        }
        let screen = host.screen_col(window.buffer, window.cursor, window.options.tabstop);
        let col = screen.saturating_sub(leftcol);
        if col >= window.text_width() as u32 && window.text_width() > 0 {
            return None;
        }
        Some((
            rect.text.row + row as u16,
            rect.text.col + number_width + col as u16,
        ))
    }

    fn statusline_layout(
        window: &Window,
        host: &dyn RenderHost,
        width: u16,
    ) -> crate::statusline::StatuslineLayout {
        let text_width = width;
        let screen = host.screen_col(window.buffer, window.cursor, window.options.tabstop);
        let full_name = host
            .buffer_name(window.buffer)
            .unwrap_or_else(|| "[No Name]".to_string());
        let name = std::path::Path::new(&full_name)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| full_name.clone());
        let statusline_template = host.statusline_template(window.id);
        if let Some(template) = statusline_template
            .as_ref()
            .or(window.options.statusline.as_ref())
        {
            crate::statusline::expand_statusline_layout(
                template,
                &name,
                &full_name,
                host.buffer_modified(window.buffer),
                host.buffer_readonly(window.buffer),
                &host.buffer_filetype(window.buffer),
                window.cursor.lnum,
                screen + 1,
                host.buffer_line_count(window.buffer),
                &host.mode_text(),
                text_width,
            )
        } else {
            crate::statusline::StatuslineLayout {
                text: crate::statusline::format_default_statusline(
                    &full_name,
                    host.buffer_modified(window.buffer),
                    window.cursor.lnum,
                    screen + 1,
                    host.buffer_line_count(window.buffer),
                    &host.mode_text(),
                    text_width,
                ),
                clicks: Vec::new(),
            }
        }
    }

    fn render_statusline_impl(
        frame: &mut Frame,
        window: &Window,
        host: &dyn RenderHost,
        rect: &crate::layout::LayoutRect,
        active: bool,
    ) {
        let rows = window.status_height().min(rect.status.rows);
        if rows == 0 {
            return;
        }
        let attr = if active {
            host.highlight("StatusLine")
        } else {
            host.highlight("StatusLineNC")
        };
        let text_width = window.width.saturating_sub(1);
        let layout = Self::statusline_layout(window, host, text_width);
        for row in 0..rows {
            let r = rect.status.row + row;
            if r >= frame.size.rows {
                break;
            }
            let cols = frame.size.cols;
            frame.base_mut().fill(
                Rect::new(r, rect.status.col, 1, rect.status.cols.min(cols)),
                ' ',
                attr,
            );
            if let Some(text) = layout.text.split('\n').nth(row as usize) {
                frame.base_mut().put_str(r, rect.status.col, text, attr);
            }
            if row == 0 {
                for click in &layout.clicks {
                    let available = rect.status.cols.saturating_sub(click.start);
                    let width = click.end.saturating_sub(click.start).min(available);
                    if width == 0 {
                        continue;
                    }
                    frame.clicks.push(ClickRegion {
                        rect: Rect::new(r, rect.status.col.saturating_add(click.start), 1, width),
                        function: click.function.clone(),
                        minwid: click.minwid,
                        area: ClickArea::Statusline,
                        winid: rvim_types::Id::raw(window.id),
                        tabnr: 0,
                    });
                }
            }
        }
    }

    fn render_bottom(
        &self,
        frame: &mut Frame,
        chrome: Chrome,
        host: &dyn RenderHost,
        tabline_rows: u16,
    ) {
        let rows = self.screen.rows;
        if rows == 0 {
            return;
        }
        let mut row = rows.saturating_sub(chrome.cmdline_rows + chrome.message_rows);
        if chrome.message_rows > 0 && row < rows {
            if let Some(message) = host.message_text() {
                let attr = host.highlight(host.message_highlight());
                frame
                    .base_mut()
                    .fill(Rect::new(row, 0, 1, self.screen.cols), ' ', attr);
                let text = message.replace('\n', " ");
                frame.base_mut().put_str(row, 0, &text, attr);
            }
            row += 1;
        }
        if chrome.cmdline_rows > 0 && row < rows {
            if let Some(command) = host.command_text() {
                let attr = host.highlight("CommandLine");
                frame
                    .base_mut()
                    .fill(Rect::new(row, 0, 1, self.screen.cols), ' ', attr);
                frame.base_mut().put_str(row, 0, command, attr);
            } else {
                let showmode = host.showmode_text();
                let showcmd = host.showcmd_text();
                if showmode.is_some() || showcmd.is_some() {
                    frame.base_mut().fill(
                        Rect::new(row, 0, 1, self.screen.cols),
                        ' ',
                        host.highlight("CommandLine"),
                    );
                }
                if let Some(showmode) = showmode {
                    frame
                        .base_mut()
                        .put_str(row, 0, &showmode, host.highlight("CommandLine"));
                }
                let right = showcmd.or_else(|| host.ruler_text());
                if let Some(right) = right {
                    let width = rvim_types::unicode::display_width(right.as_bytes(), 0, 8) as u16;
                    if width > 0 && width <= self.screen.cols {
                        frame.base_mut().put_str(
                            row,
                            self.screen.cols - width,
                            &right,
                            host.highlight("CommandLine"),
                        );
                    }
                }
            }
        }
        let _ = tabline_rows;
    }

    pub fn blank_frame(&self, seq: u64) -> Frame {
        Frame::new(seq, self.screen)
    }
}

#[allow(clippy::too_many_arguments)]
fn list_char(listchars: &str, name: &str) -> Option<char> {
    for entry in listchars.split(',') {
        if let Some((key, value)) = entry.split_once(':') {
            if key == name {
                return value.chars().next();
            }
        }
    }
    None
}

fn continuation_prefix(window: &Window, line: &[u8]) -> String {
    continuation_prefix_for(line, &window.options)
}

fn continuation_prefix_for(line: &[u8], options: &WindowOptions) -> String {
    let mut prefix = options.showbreak.clone();
    if options.breakindent {
        let indent = line
            .iter()
            .copied()
            .take_while(|byte| *byte == b' ' || *byte == b'\t')
            .collect::<Vec<u8>>();
        prefix.push_str(&String::from_utf8_lossy(&indent));
    }
    prefix
}

/// Display-line chunks for a buffer line, honoring wrap/linebreak/showbreak.
pub fn display_chunks(
    line: &[u8],
    text_width: u16,
    options: &WindowOptions,
) -> Vec<(usize, usize)> {
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
}

fn format_number(number: u32, format: &str) -> String {
    let Some(body) = format.strip_prefix('%') else {
        return number.to_string();
    };
    let Some(body) = body.strip_suffix('d') else {
        return number.to_string();
    };
    if body.is_empty() {
        return number.to_string();
    }
    let (pad, width) = if let Some(width) = body.strip_prefix('0') {
        (true, width)
    } else {
        (false, body)
    };
    let Ok(width) = width.parse::<usize>() else {
        return number.to_string();
    };
    if pad {
        format!("{number:0width$}")
    } else {
        format!("{number:width$}")
    }
}

fn text_width_of(text: &str, tabstop: usize) -> u16 {
    rvim_types::unicode::display_width(text.as_bytes(), 0, tabstop.max(1)) as u16
}

fn byte_at_display_col(line: &[u8], start: usize, target: usize, tabstop: usize) -> usize {
    let mut used = 0usize;
    let mut index = start;
    while index < line.len() {
        let (ch, len) = if line[index] == b'\t' {
            ('\t', 1)
        } else if let Some((ch, len)) = char_at(line, index) {
            (ch, len)
        } else {
            ('\u{FFFD}', 1)
        };
        let cells = if ch == '\t' {
            tabstop.max(1) - (used % tabstop.max(1))
        } else {
            char_width(ch).max(1)
        };
        if used + cells > target {
            return index;
        }
        used += cells;
        index += len;
    }
    line.len()
}

fn skipped_chunks(window: &Window, line: &[u8], chunks: &[(usize, usize)]) -> usize {
    if window.viewport.skipcol == 0 || chunks.is_empty() {
        return 0;
    }
    let skipcol = window.viewport.skipcol as usize;
    chunks
        .iter()
        .position(|(_, end)| {
            rvim_types::unicode::display_width(&line[..*end], 0, window.options.tabstop.max(1))
                > skipcol
        })
        .unwrap_or(chunks.len().saturating_sub(1))
}

fn wrap_chunks(
    line: &[u8],
    first_width: usize,
    rest_width: usize,
    tabstop: usize,
    linebreak: bool,
    breakat: &str,
) -> Vec<(usize, usize)> {
    if first_width == 0 || line.is_empty() {
        return vec![(0, line.len())];
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut used = 0usize;
    let mut byte = 0usize;
    while byte < line.len() {
        let (ch, len) = if line[byte] == b'\t' {
            ('\t', 1)
        } else if let Some((ch, len)) = char_at(line, byte) {
            (ch, len)
        } else {
            ('\u{FFFD}', 1)
        };
        let cells = if ch == '\t' {
            tabstop.max(1) - (used % tabstop.max(1))
        } else {
            char_width(ch).max(1)
        };
        let width = if chunks.is_empty() {
            first_width
        } else {
            rest_width
        };
        if used > 0 && used + cells > width {
            let mut break_at = byte;
            if linebreak && !breakat.is_empty() {
                let mut candidate = None;
                let mut index = start;
                while index < byte {
                    let (ch, len) = if line[index] == b'\t' {
                        ('\t', 1)
                    } else if let Some((ch, len)) = char_at(line, index) {
                        (ch, len)
                    } else {
                        ('\u{FFFD}', 1)
                    };
                    if breakat.contains(ch) {
                        candidate = Some(index + len);
                    }
                    index += len;
                }
                if let Some(at) = candidate {
                    if at > start {
                        break_at = at;
                    }
                }
            }
            chunks.push((start, break_at));
            start = break_at;
            used = 0;
            byte = break_at;
            continue;
        }
        used += cells;
        byte += len;
    }
    if start < line.len() || chunks.is_empty() {
        chunks.push((start, line.len()));
    }
    chunks
}

#[allow(clippy::too_many_arguments)]
fn selected_attr(
    host: &dyn RenderHost,
    window: &Window,
    lnum: u32,
    byte: u32,
    visual: Attr,
    background: Option<Attr>,
    searches: &[(u32, u32)],
    search_attr: Attr,
    styles: &[(u32, u32, Attr)],
) -> Attr {
    let selected = window
        .visual
        .is_some_and(|selection| selection_contains(selection, lnum, byte));
    let searched = searches
        .iter()
        .any(|(start, end)| byte >= *start && byte < *end);
    let base = if selected {
        visual
    } else if searched {
        search_attr
    } else if let Some((_, _, attr)) = styles
        .iter()
        .find(|(start, end, _)| byte >= *start && byte < *end)
    {
        *attr
    } else {
        host.line_attr(window.buffer, lnum, byte)
    };
    merge_attr(base, background)
}

fn merge_attr(mut attr: Attr, background: Option<Attr>) -> Attr {
    if let Some(background) = background {
        if background.bg != rvim_ui_api::Color::Default {
            attr.bg = background.bg;
        }
        attr.flags |= background.flags;
    }
    attr
}

fn selection_contains(selection: crate::window::VisualSelection, lnum: u32, byte: u32) -> bool {
    let (start, end) = if selection.start <= selection.end {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    };
    if selection.linewise {
        return lnum >= start.lnum && lnum <= end.lnum;
    }
    if selection.block {
        let start_col = start.col.min(end.col);
        let end_col = start.col.max(end.col);
        return lnum >= start.lnum && lnum <= end.lnum && byte >= start_col && byte <= end_col;
    }
    if lnum < start.lnum || lnum > end.lnum {
        return false;
    }
    if start.lnum == end.lnum {
        return byte >= start.col && byte <= end.col;
    }
    if lnum == start.lnum {
        return byte >= start.col;
    }
    if lnum == end.lnum {
        return byte <= end.col;
    }
    true
}

fn equalize_node(node: &mut LayoutNode) {
    if let LayoutNode::Split {
        children, weights, ..
    } = node
    {
        *weights = vec![1; children.len()];
        for child in children {
            equalize_node(child);
        }
    }
}

fn adjust_split(node: &mut LayoutNode, target: WindowId, vertical: bool, delta: i32) -> bool {
    match node {
        LayoutNode::Leaf(_) => false,
        LayoutNode::Split {
            dir,
            children,
            weights,
        } => {
            let matches_orientation = matches!(
                (dir, vertical),
                (SplitDir::Vertical, true) | (SplitDir::Horizontal, false)
            );
            if matches_orientation {
                if let Some(index) = children
                    .iter()
                    .position(|child| *child == LayoutNode::Leaf(target))
                {
                    if children.len() > 1 {
                        let other = if index + 1 < children.len() {
                            index + 1
                        } else {
                            index - 1
                        };
                        let target_weight = weights.get(index).copied().unwrap_or(1) as i32;
                        let other_weight = weights.get(other).copied().unwrap_or(1) as i32;
                        let new_target = (target_weight + delta).max(1);
                        let new_other = (other_weight - delta).max(1);
                        weights[index] = new_target as u16;
                        weights[other] = new_other as u16;
                        return true;
                    }
                }
            }
            for child in children {
                if adjust_split(child, target, vertical, delta) {
                    return true;
                }
            }
            false
        }
    }
}

fn replace_leaf(
    node: &mut LayoutNode,
    target: WindowId,
    dir: SplitDir,
    new_window: WindowId,
    after: bool,
) {
    match node {
        LayoutNode::Leaf(id) if *id == target => {
            let old = LayoutNode::Leaf(*id);
            let new = LayoutNode::Leaf(new_window);
            let children = if after {
                vec![old, new]
            } else {
                vec![new, old]
            };
            *node = LayoutNode::Split {
                dir,
                children,
                weights: vec![1, 1],
            };
        }
        LayoutNode::Leaf(_) => {}
        LayoutNode::Split { children, .. } => {
            for child in children {
                replace_leaf(child, target, dir, new_window, after);
            }
        }
    }
}

fn remove_leaf(node: &mut LayoutNode, target: WindowId) -> bool {
    match node {
        LayoutNode::Leaf(id) => *id == target,
        LayoutNode::Split {
            children, weights, ..
        } => {
            if let Some(index) = children
                .iter()
                .position(|child| child == &LayoutNode::Leaf(target))
            {
                children.remove(index);
                if index < weights.len() {
                    weights.remove(index);
                }
                if children.len() == 1 {
                    *node = children.remove(0);
                }
                return true;
            }
            for child in children.iter_mut() {
                if remove_leaf(child, target) {
                    if children.len() == 1 {
                        *node = children.remove(0);
                    }
                    return true;
                }
            }
            false
        }
    }
}

#[cfg(test)]
mod wrap_tests {
    use super::wrap_chunks;

    #[test]
    fn linebreak_prefers_breakat_boundaries() {
        assert_eq!(
            wrap_chunks(b"hello world", 7, 7, 8, true, " "),
            vec![(0, 6), (6, 11)]
        );
        assert_eq!(
            wrap_chunks(b"hello world", 7, 7, 8, false, " "),
            vec![(0, 7), (7, 11)]
        );
    }
}
