use std::collections::HashMap;

use rvim_edit::screen_col;
use rvim_types::{BufPos, BufferId, WindowId};

type StyledLine = Vec<(u32, u32, Attr)>;
type StyleMap = HashMap<(BufferId, u32), StyledLine>;
use rvim_ui_api::{Attr, Cursor, CursorShape, Frame, GridId, Size, UiMode};
use rvim_view::{Chrome, RenderHost};

use crate::editor::{CommandKind, Editor, Mode};

struct EditorHost<'a> {
    buffers: &'a rvim_types::Arena<crate::buffer::BufferEntry>,
    highlights: &'a crate::highlights::HighlightTable,
    mode: UiMode,
    mode_text: String,
    command: Option<String>,
    message: Option<String>,
    message_highlight: &'a str,
    showcmd: Option<String>,
    showmode: Option<String>,
    ruler: Option<String>,
    popup: Option<(rvim_ui_api::Rect, String)>,
    completion_popup: Option<(rvim_ui_api::Rect, String)>,
    tabline: Option<String>,
    tabline_regions: Vec<(u16, u16, u64)>,
    search: Option<&'a rvim_regex::Regex>,
    styles: &'a StyleMap,
    statuslines: &'a HashMap<WindowId, String>,
}

impl RenderHost for EditorHost<'_> {
    fn buffer_line(&self, buffer: BufferId, lnum: u32) -> Option<&[u8]> {
        self.buffers
            .get(buffer)
            .and_then(|buffer| buffer.text.line(lnum))
    }

    fn buffer_line_count(&self, buffer: BufferId) -> u32 {
        self.buffers
            .get(buffer)
            .map_or(0, |buffer| buffer.text.line_count())
    }

    fn buffer_name(&self, buffer: BufferId) -> Option<String> {
        self.buffers.get(buffer).map(|buffer| {
            let name = buffer.name_string();
            if name.is_empty() {
                "[No Name]".to_string()
            } else {
                name
            }
        })
    }

    fn buffer_modified(&self, buffer: BufferId) -> bool {
        self.buffers
            .get(buffer)
            .is_some_and(crate::buffer::BufferEntry::modified)
    }

    fn buffer_readonly(&self, buffer: BufferId) -> bool {
        self.buffers
            .get(buffer)
            .is_some_and(|buffer| buffer.readonly)
    }

    fn buffer_filetype(&self, buffer: BufferId) -> String {
        self.buffers
            .get(buffer)
            .map_or_else(String::new, |buffer| buffer.filetype.clone())
    }

    fn mode(&self) -> UiMode {
        self.mode
    }

    fn mode_text(&self) -> String {
        self.mode_text.clone()
    }

    fn command_text(&self) -> Option<&str> {
        self.command.as_deref()
    }

    fn message_text(&self) -> Option<&str> {
        self.message.as_deref()
    }

    fn message_highlight(&self) -> &str {
        self.message_highlight
    }

    fn showcmd_text(&self) -> Option<String> {
        self.showcmd.clone()
    }

    fn showmode_text(&self) -> Option<String> {
        self.showmode.clone()
    }

    fn ruler_text(&self) -> Option<String> {
        self.ruler.clone()
    }

    fn message_popup(&self) -> Option<(rvim_ui_api::Rect, String)> {
        self.popup.clone()
    }

    fn completion_popup(&self) -> Option<(rvim_ui_api::Rect, String)> {
        self.completion_popup.clone()
    }

    fn tabline_text(&self) -> Option<String> {
        self.tabline.clone()
    }

    fn tabline_regions(&self) -> Vec<(u16, u16, u64)> {
        self.tabline_regions.clone()
    }

    fn highlight(&self, group: &str) -> Attr {
        self.highlights.get(group)
    }

    fn screen_col(&self, buffer: BufferId, pos: BufPos, tabstop: usize) -> u32 {
        self.buffers
            .get(buffer)
            .map_or(pos.col, |buffer| screen_col(&buffer.text, pos, tabstop))
    }

    fn line_styles(&self, buffer: BufferId, lnum: u32) -> Vec<(u32, u32, Attr)> {
        self.styles
            .get(&(buffer, lnum))
            .cloned()
            .unwrap_or_default()
    }

    fn statusline_template(&self, window: WindowId) -> Option<String> {
        self.statuslines.get(&window).cloned()
    }

    fn search_matches(&self, buffer: BufferId, lnum: u32) -> Vec<(u32, u32)> {
        let Some(regex) = self.search else {
            return Vec::new();
        };
        let Some(line) = self
            .buffers
            .get(buffer)
            .and_then(|buffer| buffer.text.line(lnum))
        else {
            return Vec::new();
        };
        rvim_regex::find_all(regex, line)
            .into_iter()
            .filter(|found| found.end > found.start)
            .map(|found| (found.start as u32, found.end as u32))
            .collect()
    }
}

impl Editor {
    pub fn render(&mut self) -> Frame {
        self.sync_folds();
        self.sync_signs();
        self.update_window_buffer_public();
        self.sync_visual_selection_public();
        let command = match self.mode {
            Mode::Command(kind) => {
                let prompt = match kind {
                    CommandKind::Ex => ':',
                    CommandKind::Search => '/',
                };
                Some(format!("{prompt}{}", self.cmdline.text))
            }
            _ => None,
        };
        let mut message = if command.is_none() {
            self.messages.last.clone()
        } else {
            None
        };
        if let Some(text) = &message {
            if text.chars().count() >= self.size.cols as usize || text.contains('\n') {
                self.hit_enter = true;
            }
        }
        if self.hit_enter {
            message = Some("Press ENTER or type command to continue".to_string());
        }
        let message_highlight = self.messages.highlight();
        let popup = if self.messages.echo_window {
            message.as_ref().map(|text| {
                let width = (text.chars().count() as u16 + 2).min(self.size.cols).max(2);
                let col = self.size.cols.saturating_sub(width) / 2;
                let row = self.size.rows.saturating_sub(3);
                (rvim_ui_api::Rect::new(row, col, 1, width), text.clone())
            })
        } else {
            None
        };
        let completion_popup = if self.options.get_bool("wildmenu") {
            self.cmdline_completion.as_ref().and_then(|completion| {
                if completion.candidates.is_empty() {
                    return None;
                }
                let listed = completion
                    .candidates
                    .iter()
                    .enumerate()
                    .map(|(index, candidate)| {
                        if index == completion.index {
                            format!("[{candidate}]")
                        } else {
                            candidate.clone()
                        }
                    })
                    .collect::<Vec<_>>();
                let configured = self.options.get_number("pumheight").max(0) as usize;
                let height = if configured == 0 {
                    listed.len().min(10)
                } else {
                    listed.len().min(configured.max(1))
                }
                .max(1);
                let lines = listed.into_iter().take(height).collect::<Vec<_>>();
                let width = lines
                    .iter()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0) as u16
                    + 2;
                let configured_width = self.options.get_number("pumwidth").max(0) as u16;
                let width = width.max(configured_width).min(self.size.cols).max(2);
                let row = self.size.rows.saturating_sub(height as u16 + 2);
                Some((
                    rvim_ui_api::Rect::new(row, 1, height as u16, width),
                    lines.join(
                        "
",
                    ),
                ))
            })
        } else {
            None
        };
        let mut statuslines: HashMap<WindowId, String> = HashMap::new();
        let templates: Vec<(WindowId, String)> = self
            .view
            .windows
            .iter()
            .filter_map(|(id, window)| {
                window
                    .options
                    .statusline
                    .clone()
                    .map(|template| (id, template))
            })
            .collect();
        for (window, template) in templates {
            if !template.contains("%{") {
                continue;
            }
            let (changedtick, line, col) = self
                .view
                .windows
                .get(&window)
                .map(|window| {
                    (
                        self.buffers
                            .get(window.buffer)
                            .map_or(0, |buffer| buffer.text.meta.changedtick),
                        window.cursor.lnum,
                        window.cursor.col,
                    )
                })
                .unwrap_or((0, 0, 0));
            let key = (
                window,
                template.clone(),
                changedtick,
                line,
                col,
                self.mode_name().to_string(),
            );
            let expanded = if let Some(cached) = self.statusline_cache.get(&key) {
                cached.clone()
            } else {
                let value = expand_statusline_expressions(self, &template, window);
                self.statusline_cache.insert(key, value.clone());
                value
            };
            statuslines.insert(window, expanded);
        }
        let mut syntax = self.syntax.clone();
        let mut styles: StyleMap = HashMap::new();
        if syntax.enabled() {
            let buffer = self.current_buffer;
            let count = self
                .buffers
                .get(buffer)
                .map_or(0, |buffer| buffer.text.line_count());
            for lnum in 1..=count {
                let Some(line) = self
                    .buffers
                    .get(buffer)
                    .and_then(|buffer| buffer.text.line(lnum))
                else {
                    continue;
                };
                let spans = syntax.highlight_line(line);
                if !spans.is_empty() {
                    let styles_line = spans
                        .into_iter()
                        .map(|span| {
                            (
                                span.start as u32,
                                span.end as u32,
                                self.highlights.get(&span.group),
                            )
                        })
                        .collect();
                    styles.insert((buffer, lnum), styles_line);
                }
            }
        }
        let search = if self.options.get_bool("hlsearch") && self.search_highlight_active {
            self.search_pattern
                .as_deref()
                .and_then(|pattern| crate::search::compile_pattern(self, pattern).ok())
        } else {
            None
        };
        let tabline = self.tabline_text();
        let tabline_regions = self.tabline_regions();
        let cmdheight = self.options.get_number("cmdheight").clamp(1, 10) as u16;
        let showcmd = self.showcmd_text();
        let showmode = self.showmode_text();
        let ruler = self.ruler_text();
        let chrome = Chrome {
            tabline_rows: if tabline.is_some() { 1 } else { 0 },
            cmdline_rows: if command.is_some()
                || showcmd.is_some()
                || showmode.is_some()
                || ruler.is_some()
            {
                cmdheight
            } else {
                0
            },
            message_rows: if message.is_some() { 1 } else { 0 },
        };
        let host = EditorHost {
            buffers: &self.buffers,
            highlights: &self.highlights,
            mode: self.ui_mode(),
            mode_text: self.mode_name().to_string(),
            command,
            message,
            message_highlight,
            showcmd,
            showmode,
            ruler,
            popup,
            completion_popup,
            tabline,
            tabline_regions,
            search: search.as_ref(),
            styles: &styles,
            statuslines: &statuslines,
        };
        let mut frame = self.view.render(&host, chrome);
        for click in self.tabline_clicks() {
            frame.clicks.push(rvim_ui_api::ClickRegion {
                rect: rvim_ui_api::Rect::new(0, click.start, 1, click.end - click.start),
                function: click.function,
                minwid: click.minwid,
                area: rvim_ui_api::ClickArea::Tabline,
                winid: 0,
                tabnr: 0,
            });
        }
        self.view.last_frame = Some(frame.clone());
        if let Mode::Command(_) = self.mode {
            let row = frame.size.rows.saturating_sub(1);
            let prompt_width = 1;
            let col = (prompt_width + self.cmdline.cursor)
                .min(frame.size.cols.saturating_sub(1) as usize) as u16;
            frame.cursor = Some(Cursor {
                grid: GridId(1),
                row,
                col,
                shape: CursorShape::Vertical { thickness: 2 },
                blink: true,
            });
        }
        self.trace.log(
            "render",
            &format!(
                "{}x{} layers={} windows={}",
                self.size.cols,
                self.size.rows,
                frame.layers.len(),
                self.view.window_ids_in_current_tab().len()
            ),
        );
        frame
    }

    pub fn blank_frame(&self, size: Size) -> Frame {
        Frame::new(0, size)
    }
}

fn expand_statusline_expressions(editor: &mut Editor, template: &str, window: WindowId) -> String {
    editor.viml.set_global(
        "statusline_winid",
        rvim_viml::Value::Number(rvim_types::Id::raw(window) as i64),
    );
    let mut out = String::new();
    let mut chars = template.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch == '%' && template[index..].starts_with("%{") {
            let start = index + 2;
            if let Some(relative_end) = template[start..].find('}') {
                let end = start + relative_end;
                let expression = &template[start..end];
                match editor.eval_viml(expression) {
                    Ok(value) => out.push_str(&value.echo_string()),
                    Err(error) => {
                        editor.messages.error(error.to_string());
                        out.push('?');
                    }
                }
                while let Some((next, _)) = chars.peek() {
                    if *next <= end {
                        chars.next();
                    } else {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(ch);
    }
    out
}

#[allow(dead_code)]
fn _unused(_: Size) {}
