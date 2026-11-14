use std::collections::HashMap;

use rvim_viml::value::VimDict;
use rvim_viml::{
    ExContext, OptionScope as VimOptionScope, Result as VimResult, Value, VarScope, VimError,
    VimHost, VimString,
};

use crate::editor::Editor;
use crate::options::OptionValue;
use rvim_types::{BufPos, WindowId};

pub struct EditorVimHost<'a> {
    pub editor: &'a mut Editor,
    pub vars: HashMap<(VarScope, String), Value>,
}

impl<'a> EditorVimHost<'a> {
    pub fn new(editor: &'a mut Editor, vars: HashMap<(VarScope, String), Value>) -> Self {
        Self { editor, vars }
    }

    fn scope_key(&self, scope: VarScope, name: &str) -> (VarScope, String) {
        let id = match scope {
            VarScope::Buffer => self.editor.current_buffer_id(),
            VarScope::Window => self.editor.current_window_id(),
            VarScope::Tab => self.editor.current_tab_id(),
            _ => 0,
        };
        (scope, format!("{id}:{name}"))
    }
}

impl EditorVimHost<'_> {
    /// Resolve an optional window id used as the second argument of the fold
    /// functions; None means the current window.
    fn window_for_winid(&self, winid: Option<u64>) -> Option<WindowId> {
        match winid {
            Some(raw) => self
                .editor
                .view
                .windows
                .keys()
                .find(|id| rvim_types::Id::raw(*id) == raw),
            None => self.editor.view.current_window().map(|window| window.id),
        }
    }
}

impl VimHost for EditorVimHost<'_> {
    fn get_var(&mut self, scope: VarScope, name: &str) -> VimResult<Value> {
        if scope == VarScope::Buffer && name == "changedtick" {
            return Ok(Value::Number(
                self.editor
                    .current_buffer()
                    .map_or(0, |buffer| buffer.text.meta.changedtick as i64),
            ));
        }
        let key = self.scope_key(scope, name);
        self.vars.get(&key).cloned().ok_or_else(|| {
            VimError::new(121, format!("Undefined variable: {}{name}", scope.prefix()))
        })
    }

    fn set_var(&mut self, scope: VarScope, name: &str, value: Value) -> VimResult<()> {
        let key = self.scope_key(scope, name);
        self.vars.insert(key, value);
        Ok(())
    }

    fn delete_var(&mut self, scope: VarScope, name: &str) -> VimResult<()> {
        let key = self.scope_key(scope, name);
        self.vars.remove(&key);
        Ok(())
    }

    fn get_scope_dict(&mut self, scope: VarScope) -> VimResult<Value> {
        let id = match scope {
            VarScope::Buffer => self.editor.current_buffer_id(),
            VarScope::Window => self.editor.current_window_id(),
            VarScope::Tab => self.editor.current_tab_id(),
            _ => 0,
        };
        let prefix = format!("{id}:");
        let mut dict = VimDict::new();
        for ((entry_scope, key), value) in &self.vars {
            if *entry_scope == scope {
                if let Some(name) = key.strip_prefix(&prefix) {
                    dict.set(VimString::from(name), value.clone());
                }
            }
        }
        if scope == VarScope::Buffer {
            let tick = self
                .editor
                .current_buffer()
                .map_or(0, |buffer| buffer.text.meta.changedtick as i64);
            dict.set(VimString::from("changedtick"), Value::Number(tick));
        }
        Ok(Value::dict(dict))
    }

    fn get_option(&mut self, scope: VimOptionScope, name: &str) -> VimResult<Value> {
        if name.starts_with("t_") {
            let value = if name == "t_Co" { 256 } else { 0 };
            return Ok(Value::Number(value));
        }
        let buffer = self.editor.current_buffer_id();
        let window = self.editor.current_window_id();
        let spec = crate::options::resolve(name)
            .ok_or_else(|| VimError::new(518, format!("Unknown option: {name}")))?;
        let window_scoped = matches!(
            spec.scope,
            crate::options::OptionScope::Window | crate::options::OptionScope::GlobalLocalWindow
        );
        let canonical = spec.name;
        let value = match scope {
            VimOptionScope::Global => self.editor.options.get(canonical).cloned(),
            VimOptionScope::Local => {
                if window_scoped {
                    self.editor.options.window_value(window, canonical).cloned()
                } else {
                    self.editor.options.buffer_value(buffer, canonical).cloned()
                }
            }
            VimOptionScope::Default => {
                if window_scoped {
                    self.editor
                        .options
                        .window_value(window, canonical)
                        .or_else(|| self.editor.options.get(canonical))
                        .cloned()
                } else {
                    self.editor
                        .options
                        .buffer_value(buffer, canonical)
                        .or_else(|| self.editor.options.get(canonical))
                        .cloned()
                }
            }
        };
        let value = value.ok_or_else(|| VimError::new(518, format!("Unknown option: {name}")))?;
        Ok(match value {
            OptionValue::Bool(value) => Value::Number(i64::from(value)),
            OptionValue::Number(value) => Value::Number(value),
            OptionValue::String(value) => Value::String(value),
        })
    }

    fn set_option(&mut self, scope: VimOptionScope, name: &str, value: Value) -> VimResult<()> {
        if name.starts_with("t_") {
            return Ok(());
        }
        let spec = crate::options::resolve(name)
            .ok_or_else(|| VimError::new(518, format!("Unknown option: {name}")))?;
        let option = match spec.kind {
            crate::options::OptionType::Bool => OptionValue::Bool(match value {
                Value::Bool(value) => value,
                Value::Number(value) => value != 0,
                other => !other.echo_string().is_empty(),
            }),
            crate::options::OptionType::Number => OptionValue::Number(match value {
                Value::Number(value) => value,
                Value::Bool(value) => i64::from(value),
                other => other.echo_string().parse().unwrap_or(0),
            }),
            crate::options::OptionType::String => OptionValue::String(match value {
                Value::String(value) => value,
                other => VimString::from(other.echo_string()),
            }),
        };
        let buffer = self.editor.current_buffer_id();
        let window = self.editor.current_window_id();
        let canonical = spec.name;
        let window_scoped = matches!(
            spec.scope,
            crate::options::OptionScope::Window | crate::options::OptionScope::GlobalLocalWindow
        );
        let result = match scope {
            VimOptionScope::Global => self.editor.options.set_global(canonical, option),
            VimOptionScope::Local => {
                if window_scoped {
                    self.editor.options.set_window(window, canonical, option)
                } else {
                    self.editor.options.set_buffer(buffer, canonical, option)
                }
            }
            VimOptionScope::Default => {
                if window_scoped {
                    self.editor.options.set_window(window, canonical, option)
                } else {
                    self.editor.options.set_buffer(buffer, canonical, option)
                }
            }
        };
        result.map_err(|message| VimError::new(474, message))?;
        self.editor.sync_window_options();
        Ok(())
    }

    fn invoke_function(&mut self, name: &str, args: &[Value]) -> VimResult<Option<Value>> {
        let string_arg = |index: usize| {
            args.get(index)
                .map(|value| value.echo_string())
                .unwrap_or_default()
        };
        match name {
            "findfile" => {
                let file = string_arg(0);
                let path = string_arg(1);
                let mut candidates = Vec::new();
                let upward = path.contains(".;");
                let search = path.replace(".;", ".");
                for entry in search.split(',') {
                    let entry = entry.trim();
                    if entry.is_empty() {
                        continue;
                    }
                    if upward {
                        let mut dir = std::env::current_dir().ok();
                        while let Some(current) = dir {
                            let candidate = current.join(&file);
                            if candidate.is_file() {
                                candidates.push(candidate);
                                break;
                            }
                            dir = current.parent().map(|path| path.to_path_buf());
                        }
                    } else {
                        let candidate = std::path::Path::new(entry).join(&file);
                        if candidate.is_file() {
                            candidates.push(candidate);
                        }
                    }
                }
                let value = candidates
                    .first()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                Ok(Some(Value::string(value)))
            }
            "getcwd" => Ok(Some(Value::string(
                std::env::current_dir()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
            ))),
            "getcmdline" => Ok(Some(Value::string(self.editor.cmdline.text.as_str()))),
            "getcmdtype" => {
                let kind = match self.editor.mode {
                    crate::Mode::Command(crate::CommandKind::Ex) => ":",
                    crate::Mode::Command(crate::CommandKind::Search) => "/",
                    _ => "",
                };
                Ok(Some(Value::string(kind)))
            }
            "getcmdpos" => Ok(Some(Value::Number(self.editor.cmdline.cursor as i64 + 1))),
            "setcmdpos" => {
                let position = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                self.editor.cmdline.cursor = position
                    .saturating_sub(1)
                    .min(self.editor.cmdline.text.len());
                Ok(Some(Value::Number(0)))
            }
            "setcmdline" => {
                let text = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let position = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                self.editor.cmdline.text = text;
                self.editor.cmdline.cursor = if position > 0 {
                    position
                        .saturating_sub(1)
                        .min(self.editor.cmdline.text.len())
                } else {
                    self.editor.cmdline.text.len()
                };
                Ok(Some(Value::Number(0)))
            }
            "getscriptinfo" => {
                let mut entries = Vec::new();
                for (sid, name) in &self.editor.sourced_scripts {
                    let name = name.clone();
                    let sid = *sid;
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("sid"), Value::Number(sid as i64));
                    dict.set(VimString::from("name"), Value::string(name.as_str()));
                    dict.set(VimString::from("version"), Value::Number(1));
                    dict.set(
                        VimString::from("autoload"),
                        Value::Number(if name.contains("autoload/") { 1 } else { 0 }),
                    );
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "gettagstack" => {
                let bufnr = self
                    .editor
                    .buffer_number_of(self.editor.current_buffer_id());
                let mut items = Vec::new();
                for (buffer, pos) in &self.editor.tag_stack {
                    let from_buf = self.editor.buffer_number_of(rvim_types::Id::raw(*buffer));
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("tagname"), Value::string(""));
                    dict.set(
                        VimString::from("from"),
                        Value::list(vec![
                            Value::Number(from_buf as i64),
                            Value::Number(pos.lnum as i64),
                            Value::Number(pos.col as i64 + 1),
                            Value::Number(0),
                        ]),
                    );
                    dict.set(VimString::from("matchnr"), Value::Number(1));
                    items.push(Value::dict(dict));
                }
                let _ = bufnr;
                let mut dict = VimDict::new();
                dict.set(
                    VimString::from("curidx"),
                    Value::Number(self.editor.tag_stack.len() as i64),
                );
                dict.set(VimString::from("items"), Value::list(items));
                Ok(Some(Value::dict(dict)))
            }
            "settagstack" => {
                let action = args
                    .get(2)
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "r".to_string());
                if action.starts_with('r') {
                    self.editor.tag_stack.clear();
                }
                if let Some(Value::Dict(dict)) = args.get(1) {
                    let dict = dict.borrow();
                    if let Some(Value::List(items)) = dict.get_str("items") {
                        for item in items.borrow().items.iter() {
                            let Value::Dict(item) = item else {
                                continue;
                            };
                            let item = item.borrow();
                            let Some(Value::List(from)) = item.get_str("from") else {
                                continue;
                            };
                            let values = from.borrow().items.clone();
                            let lnum = values
                                .get(1)
                                .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                                .unwrap_or(1)
                                .max(1);
                            let col = values
                                .get(2)
                                .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                                .unwrap_or(1)
                                .max(1)
                                - 1;
                            let buffer = values
                                .first()
                                .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                                .unwrap_or(0);
                            if let Some(id) = self.editor.buffer_by_number(buffer) {
                                self.editor.tag_stack.push((id, BufPos::new(lnum, col)));
                            }
                        }
                    }
                }
                Ok(Some(Value::Number(0)))
            }
            "tagfiles" => {
                let tags = self.editor.options.get_string("tags");
                let files: Vec<Value> = tags
                    .split([',', ' '])
                    .filter(|item| !item.is_empty())
                    .map(|item| Value::string(item.to_string()))
                    .collect();
                Ok(Some(Value::list(files)))
            }
            "taglist" => {
                let pattern = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let filename = args.get(1).map(|value| value.echo_string());
                let regex = if let Some(pattern) = pattern.strip_prefix('/') {
                    rvim_regex::compile(pattern.as_bytes(), rvim_regex::Magic::Magic, false).ok()
                } else {
                    None
                };
                let mut entries = Vec::new();
                for entry in self.editor.tag_index.all_entries() {
                    let matches = if let Some(regex) = &regex {
                        !rvim_regex::find_all(regex, entry.name.as_bytes()).is_empty()
                    } else {
                        entry.name == pattern
                    };
                    if !matches {
                        continue;
                    }
                    if let Some(filename) = &filename {
                        if !filename.is_empty() && !entry.file.to_string_lossy().ends_with(filename)
                        {
                            continue;
                        }
                    }
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("name"), Value::string(entry.name.as_str()));
                    dict.set(
                        VimString::from("filename"),
                        Value::string(entry.file.display().to_string()),
                    );
                    dict.set(
                        VimString::from("cmd"),
                        Value::string(entry.command.as_str()),
                    );
                    dict.set(VimString::from("kind"), Value::string(""));
                    dict.set(VimString::from("static"), Value::Number(0));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "getmarklist" => {
                let bufnr = self
                    .editor
                    .buffer_number_of(self.editor.current_buffer_id());
                let mut entries = Vec::new();
                if let Some(buffer) = self.editor.current_buffer() {
                    let file = buffer.name_string();
                    for (mark, pos) in &buffer.marks {
                        let mut dict = VimDict::new();
                        dict.set(VimString::from("mark"), Value::string(mark.to_string()));
                        dict.set(
                            VimString::from("pos"),
                            Value::list(vec![
                                Value::Number(bufnr as i64),
                                Value::Number(pos.lnum as i64),
                                Value::Number(pos.col as i64 + 1),
                                Value::Number(0),
                            ]),
                        );
                        dict.set(VimString::from("file"), Value::string(file.as_str()));
                        entries.push(Value::dict(dict));
                    }
                }
                Ok(Some(Value::list(entries)))
            }
            "winlayout" => {
                let page = self.editor.view.tabs.get(&self.editor.view.current_tab);
                let value = page
                    .map(|page| layout_value(&page.root))
                    .unwrap_or_else(|| Value::list(Vec::new()));
                Ok(Some(value))
            }
            "screenpos" => {
                let winid = args
                    .first()
                    .map(|value| value.echo_string().parse::<u64>().unwrap_or(0))
                    .unwrap_or(0);
                let lnum = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let col = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let window_id = rvim_types::WindowId::from_raw(winid);
                let frame = self.editor.render();
                let Some(window) = self.editor.view.windows.get(&window_id) else {
                    let mut dict = VimDict::new();
                    for name in ["row", "col", "curscol", "endcol"] {
                        dict.set(VimString::from(name), Value::Number(0));
                    }
                    return Ok(Some(Value::dict(dict)));
                };
                let raw = rvim_types::Id::raw(window_id);
                let rect = frame
                    .hit_regions
                    .iter()
                    .find(|region| {
                        region.action.kind == rvim_ui_api::frame::ActionKind::Window
                            && region.action.id == raw
                    })
                    .map(|region| region.rect);
                let Some(rect) = rect else {
                    let mut dict = VimDict::new();
                    for name in ["row", "col", "curscol", "endcol"] {
                        dict.set(VimString::from(name), Value::Number(0));
                    }
                    return Ok(Some(Value::dict(dict)));
                };
                let line = self
                    .editor
                    .buffers
                    .get(window.buffer)
                    .and_then(|buffer| buffer.text.line(lnum))
                    .unwrap_or(b"");
                let width = rvim_types::unicode::display_width(
                    &line[..(col as usize - 1).min(line.len())],
                    0,
                    window.options.tabstop.max(1),
                ) as u16;
                let row = rect
                    .row
                    .saturating_add(lnum.saturating_sub(window.viewport.topline) as u16)
                    .saturating_add(1);
                let column = rect
                    .col
                    .saturating_add(width)
                    .saturating_sub(window.viewport.leftcol as u16)
                    .saturating_add(1);
                let mut dict = VimDict::new();
                dict.set(VimString::from("row"), Value::Number(row as i64));
                dict.set(VimString::from("col"), Value::Number(column as i64));
                dict.set(VimString::from("curscol"), Value::Number(column as i64));
                dict.set(VimString::from("endcol"), Value::Number(column as i64));
                Ok(Some(Value::dict(dict)))
            }
            "screenchar" => {
                let row = args
                    .first()
                    .map(|value| value.echo_string().parse::<u16>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let col = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u16>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let frame = self.editor.render();
                let text = frame
                    .base()
                    .cell(row - 1, col - 1)
                    .map(|cell| cell.text.clone())
                    .unwrap_or_default();
                match text.chars().next() {
                    Some(ch) if ch.is_ascii() => Ok(Some(Value::Number(ch as i64))),
                    Some(ch) => Ok(Some(Value::string(ch.to_string()))),
                    None => Ok(Some(Value::Number(0))),
                }
            }
            "screenstring" => {
                let row = args
                    .first()
                    .map(|value| value.echo_string().parse::<u16>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let col = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u16>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let length = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let frame = self.editor.render();
                let text = frame.base().row_text(row - 1);
                let result: String = text
                    .chars()
                    .skip(col.saturating_sub(1) as usize)
                    .take(length)
                    .collect();
                Ok(Some(Value::string(result)))
            }
            "virtcol" => {
                let mut lnum = self.editor.cursor().lnum;
                let mut col = self.editor.cursor().col;
                if let Some(Value::List(list)) = args.first() {
                    let values = list.borrow().items.clone();
                    if let Some(value) = values.first() {
                        lnum = value.echo_string().parse::<u32>().unwrap_or(lnum);
                    }
                    if let Some(value) = values.get(1) {
                        col = value
                            .echo_string()
                            .parse::<u32>()
                            .unwrap_or(col + 1)
                            .saturating_sub(1);
                    }
                }
                let line = self.editor.line(lnum).unwrap_or(b"");
                let options = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| window.options.clone())
                    .unwrap_or_default();
                let width = rvim_types::unicode::display_width(
                    &line[..(col as usize).min(line.len())],
                    0,
                    options.tabstop.max(1),
                );
                Ok(Some(Value::Number(width as i64 + 1)))
            }
            "virtcol2col" => {
                let values = args.to_vec();
                let lnum = values
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                let target = values
                    .get(2)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                let line = self.editor.line(lnum).unwrap_or(b"").to_vec();
                let options = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| window.options.clone())
                    .unwrap_or_default();
                let mut col = line.len() as u32;
                for index in 0..=line.len() as u32 {
                    let width = rvim_types::unicode::display_width(
                        &line[..index as usize],
                        0,
                        options.tabstop.max(1),
                    ) as u32
                        + 1;
                    if width >= target {
                        col = index;
                        break;
                    }
                }
                Ok(Some(Value::Number(col as i64 + 1)))
            }
            "getcharpos" => {
                let expr = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| ".".to_string());
                let pos = if expr == "." {
                    Some(self.editor.cursor())
                } else if expr == "$" {
                    Some(BufPos::new(self.editor.line_count().max(1), 0))
                } else if expr == "^" {
                    Some(BufPos::new(1, 0))
                } else if let Some(mark) = expr.trim_start_matches('\'').chars().next() {
                    self.editor.mark_position(mark)
                } else {
                    None
                };
                let Some(pos) = pos else {
                    return Ok(Some(Value::list(vec![
                        Value::Number(0),
                        Value::Number(0),
                        Value::Number(0),
                        Value::Number(0),
                    ])));
                };
                let line = self.editor.line(pos.lnum).unwrap_or(b"");
                let char_col = character_index(line, pos.col) + 1;
                let bufnr = self
                    .editor
                    .buffer_number_of(self.editor.current_buffer_id());
                Ok(Some(Value::list(vec![
                    Value::Number(bufnr as i64),
                    Value::Number(pos.lnum as i64),
                    Value::Number(char_col as i64),
                    Value::Number(0),
                ])))
            }
            "setcharpos" => {
                let expr = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| ".".to_string());
                let Some(Value::List(list)) = args.get(1) else {
                    return Ok(Some(Value::Number(-1)));
                };
                let values: Vec<i64> = list
                    .borrow()
                    .items
                    .iter()
                    .map(|value| value.echo_string().parse::<i64>().unwrap_or(0))
                    .collect();
                let lnum = values.get(1).copied().unwrap_or(0).max(1) as u32;
                let char_col = values.get(2).copied().unwrap_or(1).max(1) as u32;
                let line = self.editor.line(lnum).unwrap_or(b"").to_vec();
                let col = byte_column(&line, char_col.saturating_sub(1));
                let pos = BufPos::new(lnum, col);
                if let Some(mark) = expr.strip_prefix('\'') {
                    let mark = mark.chars().next().unwrap_or('\'');
                    self.editor.set_mark_position(mark, pos);
                } else if expr == "." {
                    self.editor.set_cursor(pos);
                }
                Ok(Some(Value::Number(0)))
            }
            "getcursorcharpos" => {
                let cursor = self.editor.cursor();
                let line = self.editor.line(cursor.lnum).unwrap_or(b"");
                let char_col = character_index(line, cursor.col) + 1;
                let bufnr = self
                    .editor
                    .buffer_number_of(self.editor.current_buffer_id());
                Ok(Some(Value::list(vec![
                    Value::Number(bufnr as i64),
                    Value::Number(cursor.lnum as i64),
                    Value::Number(char_col as i64),
                    Value::Number(0),
                ])))
            }
            "setcursorcharpos" => {
                let Some(Value::List(list)) = args.first() else {
                    return Ok(Some(Value::Number(-1)));
                };
                let values: Vec<i64> = list
                    .borrow()
                    .items
                    .iter()
                    .map(|value| value.echo_string().parse::<i64>().unwrap_or(0))
                    .collect();
                let lnum = values.get(1).copied().unwrap_or(0).max(1) as u32;
                let char_col = values.get(2).copied().unwrap_or(1).max(1) as u32;
                let line = self.editor.line(lnum).unwrap_or(b"").to_vec();
                let col = byte_column(&line, char_col.saturating_sub(1));
                self.editor.set_cursor(BufPos::new(lnum, col));
                Ok(Some(Value::Number(0)))
            }
            "getpos" => {
                let expr = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| ".".to_string());
                let pos = if expr == "." {
                    Some(self.editor.cursor())
                } else if expr == "$" {
                    Some(BufPos::new(self.editor.line_count().max(1), 0))
                } else if expr == "^" {
                    Some(BufPos::new(1, 0))
                } else if let Some(mark) = expr.trim_start_matches('\'').chars().next() {
                    self.editor.mark_position(mark)
                } else {
                    None
                };
                let Some(pos) = pos else {
                    return Ok(Some(Value::list(vec![
                        Value::Number(0),
                        Value::Number(0),
                        Value::Number(0),
                        Value::Number(0),
                    ])));
                };
                let bufnr = self
                    .editor
                    .buffer_number_of(self.editor.current_buffer_id());
                Ok(Some(Value::list(vec![
                    Value::Number(bufnr as i64),
                    Value::Number(pos.lnum as i64),
                    Value::Number(pos.col as i64 + 1),
                    Value::Number(0),
                ])))
            }
            "getcurpos" => {
                let cursor = self.editor.cursor();
                let bufnr = self
                    .editor
                    .buffer_number_of(self.editor.current_buffer_id());
                Ok(Some(Value::list(vec![
                    Value::Number(bufnr as i64),
                    Value::Number(cursor.lnum as i64),
                    Value::Number(cursor.col as i64 + 1),
                    Value::Number(0),
                    Value::Number(0),
                ])))
            }
            "getmousepos" => {
                let (row, col) = self.editor.last_mouse.unwrap_or((0, 0));
                let frame = self.editor.render();
                let region = frame.hit_regions.iter().find(|region| {
                    region.action.kind == rvim_ui_api::frame::ActionKind::Window
                        && region.rect.row <= row
                        && row < region.rect.row + region.rect.rows
                        && region.rect.col <= col
                        && col < region.rect.col + region.rect.cols
                });
                let mut dict = VimDict::new();
                dict.set(VimString::from("screenrow"), Value::Number(row as i64 + 1));
                dict.set(VimString::from("screencol"), Value::Number(col as i64 + 1));
                dict.set(
                    VimString::from("winid"),
                    Value::Number(region.map_or(0, |region| region.action.id as i64)),
                );
                dict.set(
                    VimString::from("winrow"),
                    Value::Number(
                        region.map_or(0, |region| row.saturating_sub(region.rect.row) as i64 + 1),
                    ),
                );
                dict.set(
                    VimString::from("wincol"),
                    Value::Number(
                        region.map_or(0, |region| col.saturating_sub(region.rect.col) as i64 + 1),
                    ),
                );
                Ok(Some(Value::dict(dict)))
            }
            "setpos" => {
                let expr = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| ".".to_string());
                let Some(Value::List(list)) = args.get(1) else {
                    return Ok(Some(Value::Number(-1)));
                };
                let values: Vec<i64> = list
                    .borrow()
                    .items
                    .iter()
                    .map(|value| value.echo_string().parse::<i64>().unwrap_or(0))
                    .collect();
                let lnum = values.get(1).copied().unwrap_or(0).max(1) as u32;
                let col = values.get(2).copied().unwrap_or(1).max(1) as u32 - 1;
                let pos = BufPos::new(lnum, col);
                if let Some(mark) = expr.strip_prefix('\'') {
                    let mark = mark.chars().next().unwrap_or('\'');
                    self.editor.set_mark_position(mark, pos);
                } else if expr == "." {
                    self.editor.set_cursor(pos);
                }
                Ok(Some(Value::Number(0)))
            }
            "confirm" => {
                if let Some(message) = args.first() {
                    self.editor.messages.info(message.echo_string());
                }
                let choices = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "&Yes\n&No".to_string());
                let default = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let labeled: Vec<String> = choices
                    .lines()
                    .map(|line| line.trim_start_matches('&').to_string())
                    .collect();
                self.editor.messages.info(
                    labeled
                        .iter()
                        .enumerate()
                        .map(|(index, label)| format!("{}. {label}", index + 1))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                let mut text = String::new();
                while let Some(value) = self.editor.fed_keys.pop_front() {
                    match value {
                        Value::String(chunk) => text.push_str(&chunk.to_string_lossy()),
                        Value::Number(13) | Value::Number(10) => break,
                        _ => {}
                    }
                }
                let choice = text.trim().parse::<usize>().ok();
                let choice = match choice {
                    Some(choice) if choice >= 1 && choice <= labeled.len() => choice,
                    _ => default.min(labeled.len().max(1)),
                };
                Ok(Some(Value::Number(choice as i64)))
            }
            "inputsave" => {
                let saved = std::mem::take(&mut self.editor.fed_keys);
                self.editor.input_stack.push(saved);
                Ok(Some(Value::Number(0)))
            }
            "inputrestore" => {
                let restored = self.editor.input_stack.pop();
                match restored {
                    Some(saved) => {
                        self.editor.fed_keys = saved;
                        Ok(Some(Value::Number(1)))
                    }
                    None => Ok(Some(Value::Number(0))),
                }
            }
            "inputsecret" => {
                if let Some(prompt) = args.first() {
                    self.editor.messages.info(prompt.echo_string());
                }
                let default = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut text = String::new();
                while let Some(value) = self.editor.fed_keys.pop_front() {
                    match value {
                        Value::String(chunk) => text.push_str(&chunk.to_string_lossy()),
                        Value::Number(13) | Value::Number(10) => break,
                        _ => {}
                    }
                }
                if text.is_empty() {
                    text = default;
                }
                Ok(Some(Value::String(VimString::from(text))))
            }
            "inputlist" => {
                let Some(Value::List(list)) = args.first() else {
                    return Ok(Some(Value::Number(0)));
                };
                let items = list.borrow().items.clone();
                let mut display = Vec::new();
                for (index, item) in items.iter().enumerate() {
                    display.push(format!("{}. {}", index + 1, item.echo_string()));
                }
                self.editor.messages.info(display.join("\n"));
                let mut text = String::new();
                while let Some(value) = self.editor.fed_keys.pop_front() {
                    match value {
                        Value::String(chunk) => text.push_str(&chunk.to_string_lossy()),
                        Value::Number(13) | Value::Number(10) => break,
                        _ => {}
                    }
                }
                let selected = text.trim().parse::<usize>().unwrap_or(0);
                let selected = if selected >= 1 && selected <= items.len() {
                    selected
                } else {
                    0
                };
                Ok(Some(Value::Number(selected as i64)))
            }
            "inputdialog" => {
                if let Some(prompt) = args.first() {
                    self.editor.messages.info(prompt.echo_string());
                }
                let default = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut text = String::new();
                while let Some(value) = self.editor.fed_keys.pop_front() {
                    match value {
                        Value::String(chunk) => text.push_str(&chunk.to_string_lossy()),
                        Value::Number(13) | Value::Number(10) => break,
                        _ => {}
                    }
                }
                if text.is_empty() {
                    text = default;
                }
                Ok(Some(Value::String(VimString::from(text))))
            }
            "feedkeys" => {
                let text = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                for value in parse_fed_keys(&text) {
                    self.editor.fed_keys.push_back(value);
                }
                Ok(Some(Value::Number(0)))
            }
            "getchar" => Ok(Some(
                self.editor.fed_keys.pop_front().unwrap_or(Value::Number(0)),
            )),
            "getcharstr" => {
                let value = self
                    .editor
                    .fed_keys
                    .pop_front()
                    .unwrap_or(Value::String(VimString::from("")));
                let text = match value {
                    Value::String(text) => text.to_string_lossy().into_owned(),
                    Value::Number(code) => char::from_u32(code as u32)
                        .map(|ch| ch.to_string())
                        .unwrap_or_default(),
                    other => other.echo_string(),
                };
                Ok(Some(Value::String(VimString::from(text))))
            }
            "input" => {
                if let Some(prompt) = args.first() {
                    self.editor.messages.info(prompt.echo_string());
                }
                let mut text = String::new();
                while let Some(value) = self.editor.fed_keys.pop_front() {
                    match value {
                        Value::String(chunk) => text.push_str(&chunk.to_string_lossy()),
                        Value::Number(13) | Value::Number(10) => break,
                        _ => {}
                    }
                }
                Ok(Some(Value::String(VimString::from(text))))
            }
            "win_getid" => {
                let winnr = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let tabnr = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                if winnr > 0 {
                    let tabs = self.editor.view.tab_ids();
                    let tab = if tabnr > 0 {
                        tabs.get(tabnr - 1).copied()
                    } else {
                        Some(self.editor.view.current_tab)
                    };
                    let window = tab.and_then(|tab| {
                        self.editor.view.windows_in_tab(tab).get(winnr - 1).copied()
                    });
                    let id = window.map_or(0, |window| rvim_types::Id::raw(window) as i64);
                    return Ok(Some(Value::Number(id)));
                }
                let id = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| window.id)
                    .unwrap_or_default();
                Ok(Some(Value::Number(rvim_types::Id::raw(id) as i64)))
            }
            "win_id2win" => {
                let id = args
                    .first()
                    .map(|value| value.echo_string().parse::<u64>().unwrap_or(0))
                    .unwrap_or(0);
                let ids = self.editor.view.window_ids_in_current_tab();
                let index = ids
                    .iter()
                    .position(|window| rvim_types::Id::raw(*window) == id)
                    .map_or(0, |index| index + 1);
                Ok(Some(Value::Number(index as i64)))
            }
            "win_gotoid" => {
                let id = args
                    .first()
                    .map(|value| value.echo_string().parse::<u64>().unwrap_or(0))
                    .unwrap_or(0);
                let found = self
                    .editor
                    .view
                    .window_ids_in_current_tab()
                    .into_iter()
                    .find(|window| rvim_types::Id::raw(*window) == id);
                if let Some(window) = found {
                    self.editor.view.set_current_window(window);
                    self.editor.sync_current_buffer_to_window();
                    self.editor.sync_window_options();
                    Ok(Some(Value::Number(1)))
                } else {
                    Ok(Some(Value::Number(0)))
                }
            }
            "win_findbuf" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let Some(buffer) = self.editor.buffer_by_number(number) else {
                    return Ok(Some(Value::list(Vec::new())));
                };
                let ids = self
                    .editor
                    .view
                    .windows
                    .iter()
                    .filter(|(_, window)| window.buffer == buffer)
                    .map(|(_, window)| Value::Number(rvim_types::Id::raw(window.id) as i64))
                    .collect();
                Ok(Some(Value::list(ids)))
            }
            "getqflist" => {
                if let Some(Value::Dict(dict)) = args.first() {
                    let dict = dict.borrow();
                    if dict.get_str("context").is_some() {
                        let mut out = VimDict::new();
                        if let Some(context) = &self.editor.quickfix.context {
                            out.set(VimString::from("context"), context.clone());
                        }
                        return Ok(Some(Value::dict(out)));
                    }
                    if dict.get_str("title").is_some() {
                        let mut out = VimDict::new();
                        out.set(
                            VimString::from("title"),
                            Value::string(self.editor.quickfix.title.as_str()),
                        );
                        return Ok(Some(Value::dict(out)));
                    }
                }
                let entries = self.editor.quickfix.entries.clone();
                Ok(Some(qf_entries_value(self.editor, &entries)))
            }
            "setqflist" => {
                let Some(value) = args.first() else {
                    return Ok(Some(Value::Number(-1)));
                };
                let parsed = qf_entries_from_value(value);
                let mut action = String::new();
                let mut title = "setqflist()".to_string();
                let mut context = None;
                for (index, arg) in args.iter().enumerate().skip(1) {
                    match arg {
                        Value::Dict(dict) => {
                            let dict = dict.borrow();
                            if let Some(value) = dict.get_str("action") {
                                action = value.echo_string();
                            }
                            if let Some(value) = dict.get_str("title") {
                                title = value.echo_string();
                            }
                            if let Some(value) = dict.get_str("context") {
                                context = Some(value.clone());
                            }
                        }
                        other => {
                            let text = other.echo_string();
                            if index == 1 {
                                action = text;
                            } else if index == 2 {
                                title = text;
                            }
                        }
                    }
                }
                if let Some(context) = context {
                    self.editor.quickfix.context = Some(context);
                }
                if action.starts_with('a') {
                    let mut combined = self.editor.quickfix.entries.clone();
                    combined.extend(parsed);
                    self.editor.set_quickfix(title, combined);
                } else {
                    self.editor.set_quickfix(title, parsed);
                }
                Ok(Some(Value::Number(0)))
            }
            "getloclist" => {
                let nr = loclist_number_arg(args);
                let entries = self.editor.loclist_entries_for(nr);
                Ok(Some(qf_entries_value(self.editor, &entries)))
            }
            "setloclist" => {
                let Some((index, value)) = args
                    .iter()
                    .enumerate()
                    .find(|(_, value)| matches!(value, Value::List(_)))
                else {
                    return Ok(Some(Value::Number(-1)));
                };
                let parsed = qf_entries_from_value(value);
                let action = args
                    .get(index + 1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let title = args
                    .get(index + 2)
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "setloclist()".to_string());
                let nr = loclist_number_arg(args);
                if action.starts_with('a') {
                    let mut combined = self.editor.loclist_entries_for(nr);
                    combined.extend(parsed);
                    self.editor.set_loclist_for(nr, title, combined);
                } else {
                    self.editor.set_loclist_for(nr, title, parsed);
                }
                Ok(Some(Value::Number(0)))
            }
            "gettabinfo" => {
                let ids = self.editor.view.tab_ids();
                let mut entries = Vec::new();
                for (index, id) in ids.iter().enumerate() {
                    let Some(page) = self.editor.view.tabs.get(id) else {
                        continue;
                    };
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("tabnr"), Value::Number(index as i64 + 1));
                    let windows: Vec<Value> = rvim_view::layout::all_windows(&page.root)
                        .into_iter()
                        .map(|window| Value::Number(rvim_types::Id::raw(window) as i64))
                        .collect();
                    dict.set(VimString::from("windows"), Value::list(windows));
                    let mut variables = VimDict::new();
                    let prefix = format!("{}:", rvim_types::Id::raw(*id));
                    for ((scope, key), value) in &self.vars {
                        if *scope != VarScope::Tab {
                            continue;
                        }
                        if let Some(name) = key.strip_prefix(&prefix) {
                            variables.set(VimString::from(name), value.clone());
                        }
                    }
                    dict.set(VimString::from("variables"), Value::dict(variables));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "getbufinfo" => {
                let ids = self.editor.buffer_ids();
                let mut entries = Vec::new();
                for (index, id) in ids.iter().enumerate() {
                    let Some(buffer) = self.editor.buffers.get(*id) else {
                        continue;
                    };
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("bufnr"), Value::Number(index as i64 + 1));
                    dict.set(VimString::from("name"), Value::string(buffer.name_string()));
                    dict.set(VimString::from("loaded"), Value::Bool(buffer.loaded));
                    dict.set(VimString::from("listed"), Value::Bool(buffer.listed));
                    dict.set(VimString::from("changed"), Value::Bool(buffer.modified()));
                    dict.set(
                        VimString::from("linecount"),
                        Value::Number(buffer.text.line_count() as i64),
                    );
                    let windows: Vec<Value> = self
                        .editor
                        .view
                        .windows
                        .iter()
                        .filter(|(_, window)| window.buffer == *id)
                        .map(|(_, window)| Value::Number(rvim_types::Id::raw(window.id) as i64))
                        .collect();
                    dict.set(VimString::from("windows"), Value::list(windows));
                    let mut variables = VimDict::new();
                    let prefix = format!("{}:", rvim_types::Id::raw(*id));
                    for ((scope, key), value) in &self.vars {
                        if *scope == VarScope::Buffer {
                            if let Some(name) = key.strip_prefix(&prefix) {
                                variables.set(VimString::from(name), value.clone());
                            }
                        }
                    }
                    dict.set(VimString::from("variables"), Value::dict(variables));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "getwininfo" => {
                let ids = self.editor.view.window_ids_in_current_tab();
                let buffers = self.editor.buffer_ids();
                let tabnr = self
                    .editor
                    .view
                    .tab_ids()
                    .iter()
                    .position(|id| *id == self.editor.view.current_tab)
                    .map_or(1, |index| index + 1);
                let frame = self.editor.render();
                let mut entries = Vec::new();
                for (index, id) in ids.iter().enumerate() {
                    let Some(window) = self.editor.view.windows.get(id) else {
                        continue;
                    };
                    let bufnr = buffers
                        .iter()
                        .position(|buffer| *buffer == window.buffer)
                        .map_or(0, |index| index + 1);
                    let mut dict = VimDict::new();
                    dict.set(
                        VimString::from("winid"),
                        Value::Number(rvim_types::Id::raw(*id) as i64),
                    );
                    dict.set(VimString::from("winnr"), Value::Number(index as i64 + 1));
                    dict.set(VimString::from("bufnr"), Value::Number(bufnr as i64));
                    dict.set(VimString::from("tabnr"), Value::Number(tabnr as i64));
                    dict.set(
                        VimString::from("height"),
                        Value::Number(window.height as i64),
                    );
                    dict.set(VimString::from("width"), Value::Number(window.width as i64));
                    dict.set(
                        VimString::from("topline"),
                        Value::Number(window.viewport.topline as i64),
                    );
                    dict.set(
                        VimString::from("leftcol"),
                        Value::Number(window.viewport.leftcol as i64),
                    );
                    let raw = rvim_types::Id::raw(*id);
                    let rect = frame
                        .hit_regions
                        .iter()
                        .find(|region| {
                            region.action.kind == rvim_ui_api::frame::ActionKind::Window
                                && region.action.id == raw
                        })
                        .map(|region| region.rect);
                    dict.set(
                        VimString::from("winrow"),
                        Value::Number(rect.map_or(0, |rect| rect.row as i64 + 1)),
                    );
                    dict.set(
                        VimString::from("wincol"),
                        Value::Number(rect.map_or(0, |rect| rect.col as i64 + 1)),
                    );
                    let mut variables = VimDict::new();
                    let prefix = format!("{}:", rvim_types::Id::raw(*id));
                    for ((scope, key), value) in &self.vars {
                        if *scope == VarScope::Window {
                            if let Some(name) = key.strip_prefix(&prefix) {
                                variables.set(VimString::from(name), value.clone());
                            }
                        }
                    }
                    dict.set(VimString::from("variables"), Value::dict(variables));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(entries)))
            }
            "getjumplist" => {
                let mut entries = Vec::new();
                for (buffer, pos) in &self.editor.jumplist {
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("lnum"), Value::Number(pos.lnum as i64));
                    dict.set(VimString::from("col"), Value::Number(pos.col as i64 + 1));
                    let filename = self
                        .editor
                        .buffers
                        .get(*buffer)
                        .map(|buffer| buffer.name_string())
                        .unwrap_or_default();
                    dict.set(VimString::from("filename"), Value::string(filename));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(vec![
                    Value::list(entries),
                    Value::Number(self.editor.jump_index as i64),
                ])))
            }
            "getchangelist" => {
                let mut entries = Vec::new();
                for (buffer, pos) in &self.editor.changelist {
                    let mut dict = VimDict::new();
                    dict.set(VimString::from("lnum"), Value::Number(pos.lnum as i64));
                    dict.set(VimString::from("col"), Value::Number(pos.col as i64 + 1));
                    let filename = self
                        .editor
                        .buffers
                        .get(*buffer)
                        .map(|buffer| buffer.name_string())
                        .unwrap_or_default();
                    dict.set(VimString::from("filename"), Value::string(filename));
                    entries.push(Value::dict(dict));
                }
                Ok(Some(Value::list(vec![
                    Value::list(entries),
                    Value::Number(self.editor.change_index as i64),
                ])))
            }
            "bufnr" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "%".to_string());
                let number = resolve_buffer(self.editor, &spec)
                    .map(|id| self.editor.buffer_number_of(rvim_types::Id::raw(id)) as i64)
                    .unwrap_or(-1);
                Ok(Some(Value::Number(number)))
            }
            "bufname" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "%".to_string());
                let name = resolve_buffer(self.editor, &spec)
                    .and_then(|id| self.editor.buffers.get(id))
                    .map(|buffer| buffer.name_string())
                    .unwrap_or_default();
                Ok(Some(Value::string(name)))
            }
            "bufexists" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                Ok(Some(Value::Number(i64::from(
                    resolve_buffer(self.editor, &spec).is_some(),
                ))))
            }
            "bufloaded" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let loaded = resolve_buffer(self.editor, &spec)
                    .and_then(|id| self.editor.buffers.get(id))
                    .is_some_and(|buffer| buffer.loaded);
                Ok(Some(Value::Number(i64::from(loaded))))
            }
            "bufwinnr" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let number = resolve_buffer(self.editor, &spec)
                    .and_then(|buffer| {
                        self.editor
                            .view
                            .window_ids_in_current_tab()
                            .into_iter()
                            .position(|window| {
                                self.editor
                                    .view
                                    .windows
                                    .get(&window)
                                    .is_some_and(|window| window.buffer == buffer)
                            })
                            .map(|index| index as i64 + 1)
                    })
                    .unwrap_or(-1);
                Ok(Some(Value::Number(number)))
            }
            "bufadd" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let id = self
                    .editor
                    .create_buffer(Some(std::path::Path::new(&name)))
                    .map_err(|error| VimError::plain(error.to_string()))?;
                let id = self.editor.buffer_number_of(rvim_types::Id::raw(id));
                Ok(Some(Value::Number(id as i64)))
            }
            "getbufline" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let start = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let end = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(u32::MAX))
                    .unwrap_or(u32::MAX);
                let buffer = resolve_buffer(self.editor, &spec);
                let lines = buffer
                    .and_then(|buffer| self.editor.buffers.get(buffer))
                    .map(|buffer| {
                        (start..=end.min(buffer.text.line_count()))
                            .filter_map(|lnum| {
                                buffer.text.line(lnum).map(|line| {
                                    Value::string(String::from_utf8_lossy(line).into_owned())
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(Some(Value::list(lines)))
            }
            "getbufoneline" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let lnum = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                let buffer = resolve_buffer(self.editor, &spec);
                let line = buffer
                    .and_then(|buffer| self.editor.buffers.get(buffer))
                    .and_then(|buffer| buffer.text.line(lnum))
                    .map(|line| String::from_utf8_lossy(line).into_owned())
                    .unwrap_or_default();
                Ok(Some(Value::string(line)))
            }
            "setbufline" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let lnum = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let buffer = resolve_buffer(self.editor, &spec);
                let Some(buffer) = buffer else {
                    return Ok(Some(Value::Number(1)));
                };
                let lines: Vec<String> = match args.get(2) {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect(),
                    Some(other) => vec![other.echo_string()],
                    None => Vec::new(),
                };
                if let Some(entry) = self.editor.buffers.get_mut(buffer) {
                    for (offset, line) in lines.iter().enumerate() {
                        entry
                            .text
                            .replace_line(lnum + offset as u32, VimString::from(line.as_str()));
                    }
                    entry.set_modified(true);
                }
                Ok(Some(Value::Number(0)))
            }
            "append" => {
                let lnum = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let lines: Vec<String> = match args.get(1) {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect(),
                    Some(other) => vec![other.echo_string()],
                    None => Vec::new(),
                };
                let buffer = self.editor.current_buffer;
                if let Some(entry) = self.editor.buffers.get_mut(buffer) {
                    for (offset, line) in lines.iter().enumerate() {
                        entry
                            .text
                            .insert_line(lnum + offset as u32 + 1, VimString::from(line.as_str()));
                    }
                    entry.set_modified(true);
                }
                Ok(Some(Value::Number(0)))
            }
            "deletebufline" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let first = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1)
                    .max(1);
                let last = args
                    .get(2)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(first))
                    .unwrap_or(first)
                    .max(first);
                let buffer = resolve_buffer(self.editor, &spec);
                let Some(buffer) = buffer else {
                    return Ok(Some(Value::Number(1)));
                };
                if let Some(entry) = self.editor.buffers.get_mut(buffer) {
                    for lnum in (first..=last).rev() {
                        entry.text.delete_line(lnum);
                    }
                    entry.set_modified(true);
                }
                Ok(Some(Value::Number(0)))
            }
            "winsaveview" => {
                let cursor = self.editor.cursor();
                let (topline, leftcol, skipcol) = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| {
                        (
                            window.viewport.topline,
                            window.viewport.leftcol,
                            window.viewport.skipcol,
                        )
                    })
                    .unwrap_or((1, 0, 0));
                let mut dict = VimDict::new();
                dict.set(VimString::from("lnum"), Value::Number(cursor.lnum as i64));
                dict.set(VimString::from("col"), Value::Number(cursor.col as i64 + 1));
                dict.set(VimString::from("coladd"), Value::Number(0));
                dict.set(
                    VimString::from("curswant"),
                    Value::Number(cursor.col as i64 + 1),
                );
                dict.set(VimString::from("topline"), Value::Number(topline as i64));
                dict.set(VimString::from("topfill"), Value::Number(0));
                dict.set(VimString::from("leftcol"), Value::Number(leftcol as i64));
                dict.set(VimString::from("skipcol"), Value::Number(skipcol as i64));
                Ok(Some(Value::dict(dict)))
            }
            "winrestview" => {
                let Some(Value::Dict(dict)) = args.first() else {
                    return Ok(Some(Value::Number(0)));
                };
                let dict = dict.borrow();
                let get = |name: &str| {
                    dict.get_str(name)
                        .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                        .unwrap_or(0)
                };
                let lnum = get("lnum").max(1);
                let col = get("col").max(1).saturating_sub(1);
                let topline = get("topline").max(1);
                let leftcol = get("leftcol");
                let skipcol = get("skipcol");
                self.editor.set_cursor(BufPos::new(lnum, col));
                if let Some(window) = self.editor.view.current_window_mut() {
                    window.viewport.topline = topline;
                    window.viewport.leftcol = leftcol;
                    window.viewport.skipcol = skipcol;
                }
                Ok(Some(Value::Number(0)))
            }
            "getwinpos" => {
                let frame = self.editor.render();
                let rect = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| rvim_types::Id::raw(window.id))
                    .and_then(|raw| {
                        frame
                            .hit_regions
                            .iter()
                            .find(|region| {
                                region.action.kind == rvim_ui_api::frame::ActionKind::Window
                                    && region.action.id == raw
                            })
                            .map(|region| region.rect)
                    });
                let (col, row, width, height) = rect.map_or((0, 0, 0, 0), |rect| {
                    (rect.col + 1, rect.row + 1, rect.cols, rect.rows)
                });
                Ok(Some(Value::list(vec![
                    Value::Number(col as i64),
                    Value::Number(row as i64),
                    Value::Number(width as i64),
                    Value::Number(height as i64),
                ])))
            }
            "getwinposx" => {
                let frame = self.editor.render();
                let raw = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| rvim_types::Id::raw(window.id));
                let col = raw
                    .and_then(|raw| {
                        frame.hit_regions.iter().find(|region| {
                            region.action.kind == rvim_ui_api::frame::ActionKind::Window
                                && region.action.id == raw
                        })
                    })
                    .map_or(0, |region| region.rect.col as i64 + 1);
                Ok(Some(Value::Number(col)))
            }
            "getwinposy" => {
                let frame = self.editor.render();
                let raw = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| rvim_types::Id::raw(window.id));
                let row = raw
                    .and_then(|raw| {
                        frame.hit_regions.iter().find(|region| {
                            region.action.kind == rvim_ui_api::frame::ActionKind::Window
                                && region.action.id == raw
                        })
                    })
                    .map_or(0, |region| region.rect.row as i64 + 1);
                Ok(Some(Value::Number(row)))
            }
            "winnr" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| ".".to_string());
                let ids = self.editor.view.window_ids_in_current_tab();
                let current = self.editor.view.current_window().map(|window| window.id);
                let number = if spec == "$" {
                    ids.len() as i64
                } else {
                    current
                        .and_then(|current| ids.iter().position(|id| *id == current))
                        .map_or(0, |index| index as i64 + 1)
                };
                Ok(Some(Value::Number(number)))
            }
            "winbufnr" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let ids = self.editor.view.window_ids_in_current_tab();
                let bufnr = number
                    .checked_sub(1)
                    .and_then(|index| ids.get(index))
                    .and_then(|id| self.editor.view.windows.get(id))
                    .map(|window| {
                        self.editor
                            .buffer_number_of(rvim_types::Id::raw(window.buffer))
                    })
                    .unwrap_or(0);
                Ok(Some(Value::Number(bufnr as i64)))
            }
            "winheight" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let height = number
                    .checked_sub(1)
                    .and_then(|index| {
                        self.editor
                            .view
                            .window_ids_in_current_tab()
                            .get(index)
                            .copied()
                    })
                    .and_then(|id| self.editor.view.windows.get(&id))
                    .map_or(0, |window| window.height);
                Ok(Some(Value::Number(height as i64)))
            }
            "winwidth" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let width = number
                    .checked_sub(1)
                    .and_then(|index| {
                        self.editor
                            .view
                            .window_ids_in_current_tab()
                            .get(index)
                            .copied()
                    })
                    .and_then(|id| self.editor.view.windows.get(&id))
                    .map_or(0, |window| window.width);
                Ok(Some(Value::Number(width as i64)))
            }
            "winline" => {
                let line = self
                    .editor
                    .view
                    .current_window()
                    .map(|window| {
                        self.editor
                            .cursor()
                            .lnum
                            .saturating_sub(window.viewport.topline) as i64
                            + 1
                    })
                    .unwrap_or(0);
                Ok(Some(Value::Number(line)))
            }
            "wincol" => {
                let Some(window) = self.editor.view.current_window() else {
                    return Ok(Some(Value::Number(0)));
                };
                let cursor = self.editor.cursor();
                let line = self
                    .editor
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(cursor.lnum))
                    .unwrap_or(b"");
                let width = rvim_types::unicode::display_width(
                    &line[..(cursor.col as usize).min(line.len())],
                    0,
                    window.options.tabstop.max(1),
                );
                let column = (width as u16).saturating_sub(window.viewport.leftcol as u16) + 1;
                Ok(Some(Value::Number(column as i64)))
            }
            "gettabwinvar" => {
                let tabnr = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let winnr = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let name = args
                    .get(2)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let tabs = self.editor.view.tab_ids();
                let window = tabs.get(tabnr.saturating_sub(1)).and_then(|tab| {
                    self.editor
                        .view
                        .windows_in_tab(*tab)
                        .get(winnr.saturating_sub(1))
                        .copied()
                });
                let value = window
                    .and_then(|window| {
                        self.vars
                            .get(&(
                                VarScope::Window,
                                format!("{}:{}", rvim_types::Id::raw(window), name),
                            ))
                            .cloned()
                    })
                    .or_else(|| args.get(3).cloned())
                    .unwrap_or(Value::Number(0));
                Ok(Some(value))
            }
            "settabwinvar" => {
                let tabnr = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let winnr = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let name = args
                    .get(2)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = args.get(3).cloned().unwrap_or(Value::Number(0));
                let tabs = self.editor.view.tab_ids();
                if let Some(window) = tabs.get(tabnr.saturating_sub(1)).and_then(|tab| {
                    self.editor
                        .view
                        .windows_in_tab(*tab)
                        .get(winnr.saturating_sub(1))
                        .copied()
                }) {
                    self.vars.insert(
                        (
                            VarScope::Window,
                            format!("{}:{}", rvim_types::Id::raw(window), name),
                        ),
                        value,
                    );
                }
                Ok(Some(Value::Number(0)))
            }
            "getwinvar" => {
                let window = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let ids = self.editor.view.window_ids_in_current_tab();
                let id = ids
                    .get(window.saturating_sub(1))
                    .copied()
                    .or_else(|| self.editor.view.current_window().map(|window| window.id));
                let value = id
                    .and_then(|id| {
                        let key = (
                            VarScope::Window,
                            format!("{}:{}", rvim_types::Id::raw(id), name),
                        );
                        self.vars.get(&key).cloned()
                    })
                    .or_else(|| args.get(2).cloned())
                    .unwrap_or(Value::Number(0));
                Ok(Some(value))
            }
            "setwinvar" => {
                let window = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = args.get(2).cloned().unwrap_or(Value::Number(0));
                let ids = self.editor.view.window_ids_in_current_tab();
                if let Some(id) = ids
                    .get(window.saturating_sub(1))
                    .copied()
                    .or_else(|| self.editor.view.current_window().map(|window| window.id))
                {
                    self.vars.insert(
                        (
                            VarScope::Window,
                            format!("{}:{}", rvim_types::Id::raw(id), name),
                        ),
                        value,
                    );
                }
                Ok(Some(Value::Number(0)))
            }
            "getbufvar" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = self
                    .editor
                    .buffer_by_number(number)
                    .and_then(|id| {
                        let key = (
                            VarScope::Buffer,
                            format!("{}:{}", rvim_types::Id::raw(id), name),
                        );
                        self.vars.get(&key).cloned()
                    })
                    .or_else(|| args.get(2).cloned())
                    .unwrap_or(Value::Number(0));
                Ok(Some(value))
            }
            "setbufvar" => {
                let number = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = args.get(2).cloned().unwrap_or(Value::Number(0));
                if let Some(id) = self.editor.buffer_by_number(number) {
                    self.vars.insert(
                        (
                            VarScope::Buffer,
                            format!("{}:{}", rvim_types::Id::raw(id), name),
                        ),
                        value,
                    );
                }
                Ok(Some(Value::Number(0)))
            }
            "tabpagenr" => {
                let ids = self.editor.view.tab_ids();
                let index = ids
                    .iter()
                    .position(|id| *id == self.editor.view.current_tab)
                    .map_or(1, |index| index + 1);
                let value = if args.first().is_some_and(|value| value.echo_string() == "$") {
                    ids.len()
                } else {
                    index
                };
                Ok(Some(Value::Number(value as i64)))
            }
            "tabpagewinnr" => {
                let ids = self.editor.view.tab_ids();
                let tabnr = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let tab = if tabnr == "$" {
                    self.editor.view.current_tab
                } else {
                    tabnr
                        .parse::<usize>()
                        .ok()
                        .filter(|index| *index >= 1 && *index <= ids.len())
                        .and_then(|index| ids.get(index - 1).copied())
                        .unwrap_or(self.editor.view.current_tab)
                };
                let windows = self.editor.view.windows_in_tab(tab);
                let arg = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = if arg == "$" {
                    windows.len()
                } else {
                    self.editor
                        .view
                        .tabs
                        .get(&tab)
                        .and_then(|page| {
                            windows
                                .iter()
                                .position(|window| *window == page.current)
                                .map(|index| index + 1)
                        })
                        .unwrap_or(0)
                };
                Ok(Some(Value::Number(value as i64)))
            }
            "tabpagebuflist" => {
                let ids = self.editor.view.tab_ids();
                let value = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let tab = value
                    .parse::<usize>()
                    .ok()
                    .filter(|index| *index >= 1 && *index <= ids.len())
                    .unwrap_or_else(|| {
                        ids.iter()
                            .position(|id| *id == self.editor.view.current_tab)
                            .unwrap_or(0)
                            + 1
                    });
                let bufs = ids
                    .get(tab.saturating_sub(1))
                    .and_then(|id| self.editor.view.tabs.get(id))
                    .map(|page| {
                        rvim_view::layout::all_windows(&page.root)
                            .into_iter()
                            .filter_map(|window| {
                                self.editor
                                    .view
                                    .windows
                                    .get(&window)
                                    .map(|window| rvim_types::Id::raw(window.buffer))
                            })
                            .map(|buffer| Value::Number(buffer as i64))
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(Some(Value::list(bufs)))
            }
            "gettabvar" => {
                let tab = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let ids = self.editor.view.tab_ids();
                let value = ids
                    .get(tab.saturating_sub(1))
                    .and_then(|id| {
                        let key = (
                            VarScope::Tab,
                            format!("{}:{}", rvim_types::Id::raw(*id), name),
                        );
                        self.vars.get(&key).cloned()
                    })
                    .or_else(|| args.get(2).cloned())
                    .unwrap_or(Value::Number(0));
                Ok(Some(value))
            }
            "settabvar" => {
                let tab = args
                    .first()
                    .map(|value| value.echo_string().parse::<usize>().unwrap_or(0))
                    .unwrap_or(0);
                let name = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let value = args.get(2).cloned().unwrap_or(Value::Number(0));
                let ids = self.editor.view.tab_ids();
                if let Some(id) = ids.get(tab.saturating_sub(1)).copied() {
                    self.vars.insert(
                        (
                            VarScope::Tab,
                            format!("{}:{}", rvim_types::Id::raw(id), name),
                        ),
                        value,
                    );
                }
                Ok(Some(Value::Number(0)))
            }
            "wildmenumode" => Ok(Some(Value::Number(i64::from(
                self.editor.cmdline_completion.is_some(),
            )))),
            "histadd" => {
                let kind = string_arg(0);
                let item = string_arg(1);
                let limit = self.editor.options.get_number("history").max(0) as usize;
                if let Some(history) = self.editor.history_mut(&kind) {
                    if history.back().is_some_and(|last| *last == item) {
                        return Ok(Some(Value::Number(0)));
                    }
                    history.push_back(item);
                    while history.len() > limit {
                        history.pop_front();
                    }
                    Ok(Some(Value::Number(1)))
                } else {
                    Ok(Some(Value::Number(0)))
                }
            }
            "histdel" => {
                let kind = string_arg(0);
                let spec = args.get(1).map(|value| value.echo_string());
                let Some(history) = self.editor.history_mut(&kind) else {
                    return Ok(Some(Value::Number(0)));
                };
                match spec {
                    None => {
                        history.clear();
                        Ok(Some(Value::Number(1)))
                    }
                    Some(spec) => {
                        if let Ok(index) = spec.parse::<i64>() {
                            let len = history.len() as i64;
                            let position = if index < 0 { len + index } else { index - 1 };
                            if position >= 0 && position < len {
                                history.remove(position as usize);
                                Ok(Some(Value::Number(1)))
                            } else {
                                Ok(Some(Value::Number(0)))
                            }
                        } else {
                            let before = history.len();
                            history.retain(|item| !item.contains(&spec));
                            Ok(Some(Value::Number(i64::from(history.len() != before))))
                        }
                    }
                }
            }
            "histget" => {
                let kind = string_arg(0);
                let index = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<i64>().unwrap_or(-1))
                    .unwrap_or(-1);
                let Some(history) = self.editor.history(&kind) else {
                    return Ok(Some(Value::string("")));
                };
                let len = history.len() as i64;
                let position = if index < 0 { len + index } else { index - 1 };
                let value = if position >= 0 && position < len {
                    history.get(position as usize).cloned().unwrap_or_default()
                } else {
                    String::new()
                };
                Ok(Some(Value::string(value)))
            }
            "histnr" => {
                let kind = string_arg(0);
                let value = self
                    .editor
                    .history(&kind)
                    .map_or(-1, |history| history.len() as i64);
                Ok(Some(Value::Number(value)))
            }
            "cursor" => {
                let lnum = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                let col = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                self.editor
                    .set_cursor(rvim_types::BufPos::new(lnum.max(1), col.saturating_sub(1)));
                Ok(Some(Value::Number(0)))
            }
            "col" => {
                let spec = string_arg(0);
                let pos = self.editor.cursor();
                let value = match spec.as_str() {
                    "$" => self
                        .editor
                        .current_buffer()
                        .map_or(1, |buffer| buffer.text.line_len(pos.lnum) + 1),
                    _ => pos.col + 1,
                };
                Ok(Some(Value::Number(value as i64)))
            }
            "complete" => {
                let startcol = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(1))
                    .unwrap_or(1);
                let matches = match args.get(1) {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect(),
                    _ => Vec::new(),
                };
                self.editor.complete_matches(startcol, matches);
                Ok(Some(Value::Number(0)))
            }
            "complete_add" => {
                let matches = match args.first() {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect(),
                    Some(other) => vec![other.echo_string()],
                    None => Vec::new(),
                };
                self.editor.complete_add_matches(matches);
                Ok(Some(Value::Number(0)))
            }
            "complete_check" => Ok(Some(Value::Number(0))),
            "pumvisible" => Ok(Some(Value::Number(i64::from(
                self.editor.completion_visible(),
            )))),
            "complete_info" => {
                let items = self
                    .editor
                    .completion_items()
                    .into_iter()
                    .map(|word| {
                        let mut item = VimDict::new();
                        item.set(VimString::from("word"), Value::string(word));
                        Value::dict(item)
                    })
                    .collect();
                let mut dict = VimDict::new();
                dict.set(VimString::from("mode"), Value::string(""));
                dict.set(VimString::from("selected"), Value::Number(0));
                dict.set(VimString::from("items"), Value::list(items));
                Ok(Some(Value::dict(dict)))
            }
            "getreg" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "\"".to_string());
                let as_list = args
                    .get(1)
                    .map(|value| value.echo_string() != "0")
                    .unwrap_or(false);
                let register = name
                    .chars()
                    .next()
                    .and_then(rvim_edit::RegisterName::parse)
                    .unwrap_or(rvim_edit::RegisterName::Unnamed);
                let content = self.editor.registers.get(register);
                if as_list {
                    let lines = content
                        .map(|content| {
                            content
                                .lines
                                .iter()
                                .map(|line| {
                                    Value::string(
                                        String::from_utf8_lossy(line.as_bytes()).into_owned(),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    return Ok(Some(Value::list(lines)));
                }
                let text = content
                    .map(|content| content.text())
                    .map(|text| String::from_utf8_lossy(text.as_bytes()).into_owned())
                    .unwrap_or_default();
                Ok(Some(Value::string(text)))
            }
            "getregtype" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "\"".to_string());
                let register = name
                    .chars()
                    .next()
                    .and_then(rvim_edit::RegisterName::parse)
                    .unwrap_or(rvim_edit::RegisterName::Unnamed);
                let kind = self
                    .editor
                    .registers
                    .get(register)
                    .map(|content| match content.kind {
                        rvim_edit::RegisterKind::Characterwise => "v",
                        rvim_edit::RegisterKind::Linewise => "V",
                        rvim_edit::RegisterKind::Blockwise => "b",
                    })
                    .unwrap_or("");
                Ok(Some(Value::string(kind)))
            }
            "setreg" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_else(|| "\"".to_string());
                let register = name
                    .chars()
                    .next()
                    .and_then(rvim_edit::RegisterName::parse)
                    .unwrap_or(rvim_edit::RegisterName::Unnamed);
                let lines: Vec<VimString> = match args.get(1) {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| VimString::from(item.echo_string()))
                        .collect(),
                    Some(other) => other
                        .echo_string()
                        .split('\n')
                        .map(VimString::from)
                        .collect(),
                    None => Vec::new(),
                };
                let options = args
                    .get(2)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let kind = match options.chars().next() {
                    Some('V') => rvim_edit::RegisterKind::Linewise,
                    Some('b') => rvim_edit::RegisterKind::Blockwise,
                    _ => rvim_edit::RegisterKind::Characterwise,
                };
                self.editor
                    .registers
                    .set(register, rvim_edit::RegisterContent::new(lines, kind));
                Ok(Some(Value::Number(0)))
            }
            "getline" => {
                let first = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let first = if first == 0 {
                    self.editor.cursor().lnum
                } else {
                    first
                };
                let last = args
                    .get(1)
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(first))
                    .unwrap_or(first)
                    .max(first);
                let lines: Vec<Value> = self
                    .editor
                    .current_buffer()
                    .map(|buffer| {
                        (first..=last.min(buffer.text.line_count()))
                            .filter_map(|lnum| {
                                buffer.text.line(lnum).map(|line| {
                                    Value::string(String::from_utf8_lossy(line).into_owned())
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if args.len() > 1 {
                    Ok(Some(Value::list(lines)))
                } else {
                    Ok(Some(
                        lines
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| Value::string("")),
                    ))
                }
            }
            "setline" => {
                let lnum = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let lnum = if lnum == 0 {
                    self.editor.cursor().lnum
                } else {
                    lnum
                };
                let lines: Vec<String> = match args.get(1) {
                    Some(Value::List(list)) => list
                        .borrow()
                        .items
                        .iter()
                        .map(|item| item.echo_string())
                        .collect(),
                    Some(other) => vec![other.echo_string()],
                    None => Vec::new(),
                };
                let buffer = self.editor.current_buffer;
                if let Some(entry) = self.editor.buffers.get_mut(buffer) {
                    for (offset, line) in lines.iter().enumerate() {
                        entry
                            .text
                            .replace_line(lnum + offset as u32, VimString::from(line.as_str()));
                    }
                    entry.set_modified(true);
                }
                Ok(Some(Value::Number(0)))
            }
            "line" => {
                let spec = string_arg(0);
                let value = match spec.as_str() {
                    "$" => self.editor.line_count().max(1),
                    "." => self.editor.cursor().lnum,
                    _ => self.editor.cursor().lnum,
                };
                Ok(Some(Value::Number(value as i64)))
            }
            "indent" | "cindent" => {
                let lnum = args
                    .first()
                    .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
                    .unwrap_or(0);
                let line = self
                    .editor
                    .current_buffer()
                    .and_then(|buffer| buffer.text.line(lnum))
                    .unwrap_or(b"");
                let tabstop = self.editor.options.get_number("tabstop").max(1) as usize;
                let mut width = 0usize;
                for byte in line {
                    match byte {
                        b' ' => width += 1,
                        b'\t' => width += tabstop - (width % tabstop),
                        _ => break,
                    }
                }
                Ok(Some(Value::Number(width as i64)))
            }
            "shiftwidth" => Ok(Some(Value::Number(
                self.editor
                    .options
                    .buffer_number(self.editor.current_buffer_id(), "shiftwidth"),
            ))),
            "resolve" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let resolved = std::fs::canonicalize(&path)
                    .map(|path| path.display().to_string())
                    .unwrap_or(path);
                Ok(Some(Value::string(resolved)))
            }
            "simplify" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut parts = Vec::new();
                for part in path.split('/') {
                    match part {
                        "" | "." => {}
                        ".." => {
                            parts.pop();
                        }
                        part => parts.push(part),
                    }
                }
                let prefix = if path.starts_with('/') { "/" } else { "" };
                Ok(Some(Value::string(format!("{prefix}{}", parts.join("/")))))
            }
            "fnamemodify" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let modifiers = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut path = std::path::PathBuf::from(&name);
                let mut chars = modifiers.chars().peekable();
                while let Some(modifier) = chars.next() {
                    if modifier != ':' {
                        continue;
                    }
                    let Some(kind) = chars.next() else {
                        break;
                    };
                    match kind {
                        'p' => {
                            if path.is_relative() {
                                path = std::env::current_dir().unwrap_or_default().join(&path);
                            }
                        }
                        'h' => {
                            path = path
                                .parent()
                                .map(std::path::Path::to_path_buf)
                                .unwrap_or_default();
                        }
                        't' => {
                            path = path
                                .file_name()
                                .map(std::path::PathBuf::from)
                                .unwrap_or_default();
                        }
                        'r' => {
                            path = path.with_extension("");
                        }
                        'e' => {
                            let extension = path
                                .extension()
                                .map(|extension| extension.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            path = std::path::PathBuf::from(extension);
                        }
                        _ => {}
                    }
                }
                Ok(Some(Value::string(path.display().to_string())))
            }
            "tempname" => {
                use std::sync::atomic::{AtomicU64, Ordering};
                static COUNTER: AtomicU64 = AtomicU64::new(0);
                let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
                let unique = format!(
                    "rvim-{}-{}-{}",
                    std::process::id(),
                    counter,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.subsec_nanos())
                        .unwrap_or(0)
                );
                let path = std::env::temp_dir().join(unique);
                Ok(Some(Value::string(path.display().to_string())))
            }
            "mkdir" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let result = std::fs::create_dir(&path).map(|()| 0).unwrap_or(-1);
                Ok(Some(Value::Number(result)))
            }
            "delete" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let flags = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let result = if flags.contains('d') {
                    std::fs::remove_dir_all(&path)
                } else if flags.contains('D') {
                    std::fs::remove_dir(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                Ok(Some(Value::Number(result.map(|()| 0).unwrap_or(-1))))
            }
            "rename" => {
                let from = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let to = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let result = std::fs::rename(&from, &to).map(|()| 0).unwrap_or(-1);
                Ok(Some(Value::Number(result)))
            }
            "getpid" => Ok(Some(Value::Number(std::process::id() as i64))),
            "hostname" => {
                let name = std::env::var("HOSTNAME")
                    .or_else(|_| std::env::var("COMPUTERNAME"))
                    .or_else(|_| {
                        std::fs::read_to_string("/etc/hostname").map(|text| text.trim().to_string())
                    })
                    .unwrap_or_default();
                Ok(Some(Value::string(name)))
            }
            "prompt_setprompt" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let text = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let Some(buffer) = resolve_buffer(self.editor, &spec) else {
                    return Err(VimError::new(94, "No matching buffer"));
                };
                self.editor.set_prompt_text(buffer, text);
                Ok(Some(Value::Number(0)))
            }
            "prompt_setcallback" | "prompt_setinterrupt" => {
                let spec = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let Some(buffer) = resolve_buffer(self.editor, &spec) else {
                    return Err(VimError::new(94, "No matching buffer"));
                };
                let callee = args.get(1).cloned();
                let target = if name == "prompt_setcallback" {
                    &mut self.editor.prompt_callbacks
                } else {
                    &mut self.editor.prompt_interrupts
                };
                match callee {
                    Some(value) if value.as_funcref().is_some() => {
                        target.insert(buffer, value);
                    }
                    _ => {
                        target.remove(&buffer);
                    }
                }
                Ok(Some(Value::Number(0)))
            }
            "getenv" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                match std::env::var(&name) {
                    Ok(value) => Ok(Some(Value::string(value))),
                    Err(_) => Ok(Some(Value::Special(rvim_viml::value::SpecialValue::Null))),
                }
            }
            "setenv" => {
                let name = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                if let Some(value) = args.get(1) {
                    match value {
                        Value::Special(rvim_viml::value::SpecialValue::Null)
                        | Value::Special(rvim_viml::value::SpecialValue::None) => {
                            std::env::remove_var(&name);
                        }
                        other => std::env::set_var(&name, other.echo_string()),
                    }
                }
                Ok(Some(Value::Number(0)))
            }
            "environ" => {
                let mut dict = VimDict::new();
                for (name, value) in std::env::vars() {
                    dict.set(VimString::from(name), Value::string(value));
                }
                Ok(Some(Value::dict(dict)))
            }
            "glob" => {
                let pattern = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let as_list = args
                    .get(2)
                    .map(|value| value.echo_string() != "0")
                    .unwrap_or(false);
                let matches = glob_patterns(&pattern);
                if as_list {
                    Ok(Some(Value::list(
                        matches.into_iter().map(Value::string).collect(),
                    )))
                } else {
                    Ok(Some(Value::string(matches.join("\n"))))
                }
            }
            "globpath" => {
                let paths = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let expression = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let as_list = args
                    .get(3)
                    .map(|value| value.echo_string() != "0")
                    .unwrap_or(false);
                let mut matches = Vec::new();
                for directory in paths.split(',') {
                    let pattern = format!("{}/{}", directory.trim_end_matches('/'), expression);
                    matches.extend(glob_patterns(&pattern));
                }
                matches.sort();
                matches.dedup();
                if as_list {
                    Ok(Some(Value::list(
                        matches.into_iter().map(Value::string).collect(),
                    )))
                } else {
                    Ok(Some(Value::string(matches.join("\n"))))
                }
            }
            "filereadable" => Ok(Some(Value::Number(i64::from(
                std::path::Path::new(&string_arg(0)).is_file(),
            )))),
            "filewritable" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let writable = std::fs::OpenOptions::new().write(true).open(&path).is_ok();
                Ok(Some(Value::Number(i64::from(writable))))
            }
            "getfsize" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let size = std::fs::metadata(&path)
                    .map(|metadata| metadata.len() as i64)
                    .unwrap_or(-1);
                Ok(Some(Value::Number(size)))
            }
            "getftype" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let kind = std::fs::symlink_metadata(&path)
                    .map(|metadata| {
                        let file_type = metadata.file_type();
                        if file_type.is_symlink() {
                            "link"
                        } else if file_type.is_dir() {
                            "dir"
                        } else if file_type.is_file() {
                            "file"
                        } else {
                            ""
                        }
                    })
                    .unwrap_or("");
                Ok(Some(Value::string(kind)))
            }
            "readdir" => {
                let path = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut names: Vec<String> = std::fs::read_dir(&path)
                    .map(|entries| {
                        entries
                            .filter_map(|entry| entry.ok())
                            .map(|entry| entry.file_name().to_string_lossy().into_owned())
                            .filter(|name| name != "." && name != "..")
                            .collect()
                    })
                    .unwrap_or_default();
                names.sort();
                Ok(Some(Value::list(
                    names.into_iter().map(Value::string).collect(),
                )))
            }
            "isdirectory" => Ok(Some(Value::Number(i64::from(
                std::path::Path::new(&string_arg(0)).is_dir(),
            )))),
            "expand" => {
                let spec = string_arg(0);
                let value = match spec.as_str() {
                    "%" => self
                        .editor
                        .current_buffer()
                        .and_then(|buffer| buffer.text.meta.name.as_ref())
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                    "#" => self
                        .editor
                        .alternate_buffer
                        .and_then(|id| self.editor.buffers.get(id))
                        .and_then(|buffer| buffer.text.meta.name.as_ref())
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                    "~" => std::env::var("HOME").unwrap_or_default(),
                    "<cword>" | "<cWORD>" => self.editor.keyword_under_cursor().unwrap_or_default(),
                    "<afile>" | "<amatch>" => self
                        .editor
                        .current_buffer()
                        .and_then(|buffer| buffer.text.meta.name.as_ref())
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                    other if other.starts_with("$/") => other.to_string(),
                    other if other.starts_with('$') => {
                        let name = other.trim_start_matches('$');
                        std::env::var(name).unwrap_or_default()
                    }
                    other if other == "~/" || other.starts_with("~/") => {
                        let home = std::env::var("HOME").unwrap_or_default();
                        format!("{home}{}", &other[1..])
                    }
                    other => other.to_string(),
                };
                Ok(Some(Value::string(value)))
            }
            "undotree" => {
                let buffer = self.editor.current_buffer();
                let (nodes, current) = buffer
                    .map(|buffer| (buffer.undo.nodes(), buffer.undo.current()))
                    .unwrap_or((&[], 0));
                let mut dict = VimDict::new();
                let seq_last = nodes
                    .iter()
                    .map(|node| node.snapshot.seq)
                    .max()
                    .unwrap_or(0);
                dict.set(VimString::from("seq_last"), Value::Number(seq_last as i64));
                dict.set(VimString::from("seq_cur"), Value::Number(current as i64));
                dict.set(
                    VimString::from("time_cur"),
                    Value::Number(nodes.get(current).map_or(0, |node| node.time) as i64),
                );
                let entries = nodes
                    .iter()
                    .map(|node| {
                        let mut entry = VimDict::new();
                        entry.set(
                            VimString::from("seq"),
                            Value::Number(node.snapshot.seq as i64),
                        );
                        entry.set(VimString::from("time"), Value::Number(node.time as i64));
                        entry.set(
                            VimString::from("curhead"),
                            Value::Number(i64::from(node.id == current)),
                        );
                        entry.set(
                            VimString::from("alt"),
                            Value::Number(i64::from(node.children.len() > 1)),
                        );
                        Value::dict(entry)
                    })
                    .collect();
                dict.set(VimString::from("entries"), Value::list(entries));
                Ok(Some(Value::dict(dict)))
            }
            "getcompletion" => {
                let pattern = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let kind = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let mut candidates = completion_candidates(self, &pattern, &kind);
                candidates.sort();
                candidates.dedup();
                Ok(Some(Value::list(
                    candidates.into_iter().map(Value::string).collect(),
                )))
            }
            "getcmdwintype" => {
                let kind = self.editor.cmdwin.as_ref().map(|state| state.kind);
                let value = match kind {
                    Some(crate::editor::CommandKind::Ex) => ":",
                    Some(crate::editor::CommandKind::Search)
                        if self
                            .editor
                            .cmdwin
                            .as_ref()
                            .is_some_and(|state| state.backward) =>
                    {
                        "?"
                    }
                    Some(crate::editor::CommandKind::Search) => "/",
                    None => "",
                };
                Ok(Some(Value::string(value)))
            }
            "timer_start" => {
                let millis = args
                    .first()
                    .map(|value| value.echo_string().parse::<u64>().unwrap_or(0))
                    .unwrap_or(0);
                let callback = args
                    .get(1)
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let id = self.editor.start_timer(millis, false, callback);
                Ok(Some(Value::Number(id)))
            }
            "timer_stop" => {
                let id = args
                    .first()
                    .map(|value| value.echo_string().parse::<i64>().unwrap_or(0))
                    .unwrap_or(0);
                Ok(Some(Value::Number(i64::from(self.editor.stop_timer(id)))))
            }
            "timer_stopall" => {
                self.editor.scheduler.stop_all();
                Ok(Some(Value::Number(0)))
            }
            "prop_type_add" | "prop_type_change" | "prop_type_delete" | "prop_type_get"
            | "prop_type_list" | "prop_add" | "prop_add_list" | "prop_clear" | "prop_find"
            | "prop_list" | "prop_remove" => Ok(Some(self.editor.prop_call_viml(name, args)?)),
            "sign_define" | "sign_undefine" | "sign_getdefined" | "sign_place" | "sign_unplace"
            | "sign_getplaced" | "sign_jump" | "sign_placelist" | "sign_unplacelist" => {
                Ok(Some(self.editor.sign_call_viml(name, args)?))
            }
            _ => Ok(None),
        }
    }

    fn has_command(&mut self, name: &str) -> bool {
        crate::registry::COMMANDS
            .iter()
            .any(|command| command.name == name || command.abbrev == name)
            || self.editor.user_commands.contains_key(name)
    }

    fn get_register(&mut self, name: char) -> VimResult<Value> {
        let Some(name) = rvim_edit::RegisterName::parse(name) else {
            return Err(VimError::new(354, "Invalid register name"));
        };
        Ok(self
            .editor
            .registers
            .get(name)
            .map(|content| Value::String(content.text()))
            .unwrap_or_else(|| Value::String(VimString::new())))
    }

    fn get_register_list(&mut self, name: char, as_list: bool) -> VimResult<Value> {
        if !as_list {
            return self.get_register(name);
        }
        let Some(register) = rvim_edit::RegisterName::parse(name) else {
            return Err(VimError::new(354, "Invalid register name"));
        };
        let lines = self
            .editor
            .registers
            .get(register)
            .map(|content| {
                content
                    .lines
                    .iter()
                    .map(|line| {
                        Value::string(String::from_utf8_lossy(line.as_bytes()).into_owned())
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Value::list(lines))
    }

    fn set_register_ex(&mut self, name: char, value: Value, options: &str) -> VimResult<()> {
        let Some(register) = rvim_edit::RegisterName::parse(name) else {
            return Err(VimError::new(354, "Invalid register name"));
        };
        let lines: Vec<VimString> = match value {
            Value::List(list) => list
                .borrow()
                .items
                .iter()
                .map(|item| VimString::from(item.echo_string()))
                .collect(),
            Value::String(text) => text
                .as_bytes()
                .split(|byte| *byte == b'\n')
                .map(VimString::from)
                .collect(),
            other => vec![VimString::from(other.echo_string())],
        };
        let kind = match options.chars().next() {
            Some('V') => rvim_edit::RegisterKind::Linewise,
            Some('b') => rvim_edit::RegisterKind::Blockwise,
            _ => rvim_edit::RegisterKind::Characterwise,
        };
        self.editor
            .registers
            .set(register, rvim_edit::RegisterContent::new(lines, kind));
        Ok(())
    }

    fn set_register(&mut self, name: char, value: Value) -> VimResult<()> {
        let Some(name) = rvim_edit::RegisterName::parse(name) else {
            return Err(VimError::new(354, "Invalid register name"));
        };
        let text = match value {
            Value::String(text) => text,
            other => VimString::from(other.echo_string()),
        };
        let lines = text
            .as_bytes()
            .split(|byte| *byte == b'\n')
            .map(VimString::from)
            .collect();
        let kind = if text.as_bytes().contains(&b'\n') {
            rvim_edit::RegisterKind::Linewise
        } else {
            rvim_edit::RegisterKind::Characterwise
        };
        self.editor
            .registers
            .set(name, rvim_edit::RegisterContent::new(lines, kind));
        Ok(())
    }

    fn message(&mut self, text: &str) {
        match self.editor.message_kind {
            crate::editor::MessageKind::Echo | crate::editor::MessageKind::EchoWindow => {
                let append = self.editor.message_append;
                self.editor.messages.echo(text, append);
                if self.editor.message_kind == crate::editor::MessageKind::EchoWindow {
                    self.editor.messages.echo_window = true;
                }
            }
            crate::editor::MessageKind::EchoMsg => self.editor.messages.echomsg(text),
            crate::editor::MessageKind::EchoErr => self.editor.messages.echoerr(text),
            _ => self.editor.messages.info(text),
        }
    }

    fn error(&mut self, error: &VimError) {
        self.editor.messages.error(error.to_string());
    }

    fn highlight_id(&mut self, name: &str) -> VimResult<i64> {
        if self.editor.highlights.contains(name) {
            // Stable pseudo id: the group's sorted position + 1.
            let mut groups: Vec<&str> = self
                .editor
                .highlights
                .iter()
                .map(|(group, _)| group)
                .collect();
            groups.sort_unstable();
            let id = groups
                .iter()
                .position(|group| *group == name)
                .map(|position| position as i64 + 1)
                .unwrap_or(0);
            return Ok(id);
        }
        Ok(0)
    }

    fn indent(&mut self, lnum: i64) -> VimResult<i64> {
        let Some(buffer) = self.editor.current_buffer() else {
            return Ok(-1);
        };
        if lnum < 1 || lnum as u32 > buffer.text.line_count() {
            return Ok(-1);
        }
        let line = buffer.text.line(lnum as u32).unwrap_or(b"");
        let tabstop = self
            .editor
            .options
            .buffer_number(self.editor.current_buffer_id(), "tabstop")
            .max(1) as usize;
        let indent = line
            .iter()
            .take_while(|byte| **byte == b' ' || **byte == b'\t')
            .count();
        Ok(rvim_types::unicode::display_width(&line[..indent], 0, tabstop) as i64)
    }

    fn fold_closed(&mut self, lnum: i64, winid: Option<u64>) -> VimResult<i64> {
        let Some(window) = self.window_for_winid(winid) else {
            return Ok(-1);
        };
        if lnum <= 0 {
            return Ok(-1);
        }
        match self.editor.folded_at(window, lnum as u32) {
            Some((start, _end)) => Ok(i64::from(start)),
            None => Ok(-1),
        }
    }

    fn fold_closed_end(&mut self, lnum: i64, winid: Option<u64>) -> VimResult<i64> {
        let Some(window) = self.window_for_winid(winid) else {
            return Ok(-1);
        };
        if lnum <= 0 {
            return Ok(-1);
        }
        match self.editor.folded_at(window, lnum as u32) {
            Some((_start, end)) => Ok(i64::from(end)),
            None => Ok(-1),
        }
    }

    fn fold_level(&mut self, lnum: i64, winid: Option<u64>) -> VimResult<i64> {
        let Some(window) = self.window_for_winid(winid) else {
            return Ok(0);
        };
        if lnum <= 0 {
            return Ok(0);
        }
        Ok(i64::from(self.editor.fold_level_at(window, lnum as u32)))
    }

    fn fold_text(&mut self) -> VimResult<String> {
        Ok(self.editor.current_foldtext_var())
    }

    fn fold_text_result(&mut self, lnum: i64, winid: Option<u64>) -> VimResult<String> {
        let Some(window) = self.window_for_winid(winid) else {
            return Ok(String::new());
        };
        if lnum <= 0 {
            return Ok(String::new());
        }
        Ok(self.editor.fold_text_result(window, lnum as u32))
    }

    fn system_output(&mut self, command: &str) -> VimResult<Vec<u8>> {
        let output = self
            .editor
            .run_shell(command, None)
            .map_err(|error| VimError::plain(error.to_string()))?;
        Ok(output.stdout)
    }

    fn runtime_files(&mut self, pattern: &str, all: bool) -> VimResult<Vec<(String, Vec<u8>)>> {
        let paths = if all {
            self.editor.runtime.find_all(pattern)
        } else {
            self.editor.runtime.find(pattern).into_iter().collect()
        };
        let mut files = Vec::new();
        for path in paths {
            let bytes = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            files.push((path.display().to_string(), bytes));
        }
        Ok(files)
    }

    fn autoload_script(&mut self, prefix: &str) -> VimResult<Option<(String, Vec<u8>)>> {
        let pattern = format!("autoload/{prefix}.vim");
        let Some(path) = self.editor.runtime.find(&pattern) else {
            return Ok(None);
        };
        let bytes = rvim_platform::fs::read_file(&path)
            .map_err(|error| VimError::new(13, error.to_string()))?;
        Ok(Some((path.display().to_string(), bytes)))
    }

    fn user_command_expansion(
        &mut self,
        line: &str,
    ) -> VimResult<Option<(String, Option<rvim_types::ScriptId>)>> {
        crate::ex::expand_user_command_line(self.editor, line)
            .map_err(|error| VimError::new(error.code.unwrap_or(0), error.message))
    }

    fn execute_ex(&mut self, command: &str, context: &ExContext) -> VimResult<Value> {
        let context_script = if context.script_id.is_nil() {
            None
        } else {
            Some(context.script_id)
        };
        let previous = self.editor.viml_script_context;
        self.editor.viml_script_context = context_script;
        let result = self.editor.execute_ex_line(command);
        self.editor.viml_script_context = previous;
        result.map_err(|error| VimError::new(error.code.unwrap_or(0), error.message))?;
        Ok(Value::Number(0))
    }

    fn function_exists(&mut self, name: &str) -> bool {
        rvim_viml::builtins::is_builtin(name)
    }

    fn get_line(&mut self, lnum: i64) -> VimResult<VimString> {
        let line = self
            .editor
            .line(lnum.max(1) as u32)
            .map(VimString::from)
            .unwrap_or_default();
        Ok(line)
    }

    fn set_line(&mut self, lnum: i64, text: &VimString) -> VimResult<()> {
        if let Some(buffer) = self.editor.current_buffer_mut() {
            buffer.text.replace_line(lnum.max(1) as u32, text.clone());
        }
        Ok(())
    }

    fn line_count(&mut self) -> VimResult<i64> {
        Ok(self.editor.line_count() as i64)
    }

    fn current_line(&mut self) -> VimResult<i64> {
        Ok(self.editor.cursor().lnum as i64)
    }

    fn read_file(&mut self, path: &str) -> VimResult<Vec<u8>> {
        rvim_platform::fs::read_file(std::path::Path::new(path))
            .map_err(|error| VimError::new(13, error.to_string()))
    }
}

fn shell_commands(pattern: &str) -> Vec<String> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for directory in std::env::split_paths(&path) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(pattern) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn runtime_files(host: &EditorVimHost<'_>, pattern: &str) -> Vec<String> {
    let mut names = Vec::new();
    for root in host.editor.runtime.expanded_runtimepath() {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(pattern) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn completion_candidates(host: &mut EditorVimHost<'_>, pattern: &str, kind: &str) -> Vec<String> {
    let matches = |value: &str| value.starts_with(pattern);
    match kind {
        "command" => crate::registry_generated::ALL_COMMANDS
            .iter()
            .map(|spec| spec.name)
            .filter(|name| matches(name))
            .map(str::to_string)
            .collect(),
        "function" => crate::registry_generated::ALL_FUNCTIONS
            .iter()
            .map(|spec| spec.name)
            .filter(|name| matches(name))
            .map(str::to_string)
            .collect(),
        "event" => crate::registry_generated::ALL_EVENTS
            .iter()
            .map(|spec| spec.name)
            .filter(|name| matches(name))
            .map(str::to_string)
            .collect(),
        "option" => {
            let mut out = Vec::new();
            for spec in crate::options::specs() {
                if matches(spec.name) {
                    out.push(spec.name.to_string());
                }
                if !spec.short.is_empty() && matches(spec.short) {
                    out.push(spec.short.to_string());
                }
            }
            out
        }
        "tag" => host
            .editor
            .tag_index
            .names()
            .filter(|name| matches(name))
            .map(str::to_string)
            .collect(),
        "buffer" => host
            .editor
            .buffers
            .values()
            .map(|buffer| buffer.name_string())
            .filter(|name| !name.is_empty() && matches(name))
            .collect(),
        "highlight" => host
            .editor
            .highlights
            .iter()
            .map(|(name, _)| name)
            .filter(|name| matches(name))
            .map(str::to_string)
            .collect(),
        "environment" => std::env::vars()
            .map(|(name, _)| name)
            .filter(|name| matches(name))
            .collect(),
        "mapping" => host
            .editor
            .mappings
            .entries()
            .into_iter()
            .map(|(_, keys)| keys)
            .filter(|keys| matches(keys))
            .collect(),
        "syntax" => host
            .editor
            .highlights
            .iter()
            .map(|(name, _)| name)
            .filter(|name| matches(name))
            .map(str::to_string)
            .collect(),
        "filetype" => list_runtime_dir(host, "syntax", pattern),
        "scriptnames" => host
            .editor
            .viml
            .script_ids()
            .into_iter()
            .filter_map(|id| host.editor.viml.script_name(id))
            .filter(|name| matches(name))
            .collect(),
        "shellcmd" => shell_commands(pattern),
        "runtime" => runtime_files(host, pattern),
        "compiler" => list_runtime_dir(host, "compiler", pattern),
        "color" | "colors" => list_runtime_dir(host, "colors", pattern),
        "variable" => {
            let mut out = Vec::new();
            if let Ok(globals) = host.editor.viml.globals().try_borrow() {
                for entry in &globals.entries {
                    let name = entry.key.to_string_lossy();
                    if matches(&name) {
                        out.push(format!("g:{name}"));
                    }
                }
            }
            out
        }
        "file" | "dir" => {
            let Ok(entries) = std::fs::read_dir(std::env::current_dir().unwrap_or_default()) else {
                return Vec::new();
            };
            entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    kind != "dir" || entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                })
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| matches(name))
                .collect()
        }
        _ => Vec::new(),
    }
}

fn list_runtime_dir(host: &EditorVimHost<'_>, directory: &str, pattern: &str) -> Vec<String> {
    let Some(root) = host.editor.runtime.vimruntime() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(root.join(directory)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".vim"))
        .map(|name| name.trim_end_matches(".vim").to_string())
        .filter(|name| name.starts_with(pattern))
        .collect()
}

fn parse_fed_keys(text: &str) -> Vec<Value> {
    let mut keys = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '<' {
            keys.push(Value::String(VimString::from(ch.to_string())));
            continue;
        }
        let mut notation = String::new();
        while let Some(&next) = chars.peek() {
            chars.next();
            if next == '>' {
                break;
            }
            notation.push(next);
        }
        let value = match notation.as_str() {
            "CR" | "Enter" => Value::Number(13),
            "NL" => Value::Number(10),
            "Esc" => Value::Number(27),
            "Tab" => Value::Number(9),
            "BS" => Value::Number(8),
            "Space" => Value::Number(32),
            name if name.starts_with("C-") => {
                let key = name[2..].chars().next().unwrap_or('@');
                Value::Number((key.to_ascii_lowercase() as i64 - 'a' as i64 + 1).max(0))
            }
            other => Value::String(VimString::from(format!("<{other}>"))),
        };
        keys.push(value);
    }
    keys
}

fn layout_value(node: &rvim_view::layout::LayoutNode) -> Value {
    use rvim_view::layout::LayoutNode;
    match node {
        LayoutNode::Leaf(id) => Value::list(vec![
            Value::string("leaf"),
            Value::Number(rvim_types::Id::raw(*id) as i64),
        ]),
        LayoutNode::Split { dir, children, .. } => {
            let label = match dir {
                rvim_view::layout::SplitDir::Horizontal => "row",
                rvim_view::layout::SplitDir::Vertical => "col",
            };
            let mut items = vec![Value::string(label)];
            items.extend(children.iter().map(layout_value));
            Value::list(items)
        }
    }
}

fn glob_patterns(pattern: &str) -> Vec<String> {
    let pattern = if let Some(rest) = pattern.strip_prefix("~/") {
        std::env::var("HOME")
            .map(|home| format!("{home}/{rest}"))
            .unwrap_or_else(|_| pattern.to_string())
    } else {
        pattern.to_string()
    };
    if !pattern.contains(['*', '?', '[']) {
        return if std::path::Path::new(&pattern).exists() {
            vec![pattern]
        } else {
            Vec::new()
        };
    }
    let mut root = String::new();
    for (index, ch) in pattern.char_indices() {
        if matches!(ch, '*' | '?' | '[') {
            root = pattern[..index].to_string();
            break;
        }
    }
    if let Some(position) = root.rfind('/') {
        root.truncate(position);
    } else {
        root = ".".to_string();
    }
    if root.is_empty() {
        root = "/".to_string();
    }
    let mut matches = Vec::new();
    walk_glob(std::path::Path::new(&root), &pattern, &mut matches, 0);
    matches.sort();
    matches.dedup();
    matches
}

fn walk_glob(path: &std::path::Path, pattern: &str, out: &mut Vec<String>, depth: usize) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let child = entry.path();
        let text = child.display().to_string();
        if wild_match(pattern.as_bytes(), text.as_bytes()) {
            out.push(text.clone());
        }
        if child.is_dir() {
            walk_glob(&child, pattern, out, depth + 1);
        }
    }
}

fn wild_match(pattern: &[u8], text: &[u8]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    match pattern[0] {
        b'*' => {
            wild_match(&pattern[1..], text) || (!text.is_empty() && wild_match(pattern, &text[1..]))
        }
        b'?' => !text.is_empty() && wild_match(&pattern[1..], &text[1..]),
        b'[' => {
            let Some(end) = pattern.iter().position(|byte| *byte == b']') else {
                return false;
            };
            if text.is_empty() {
                return false;
            }
            let mut matched = false;
            let mut index = 1;
            let mut negate = false;
            if pattern.get(1) == Some(&b'^') {
                negate = true;
                index = 2;
            }
            while index < end {
                if index + 2 < end && pattern[index + 1] == b'-' {
                    if text[0] >= pattern[index] && text[0] <= pattern[index + 2] {
                        matched = true;
                    }
                    index += 3;
                } else {
                    if text[0] == pattern[index] {
                        matched = true;
                    }
                    index += 1;
                }
            }
            (matched != negate) && wild_match(&pattern[end + 1..], &text[1..])
        }
        byte => !text.is_empty() && text[0] == byte && wild_match(&pattern[1..], &text[1..]),
    }
}

fn resolve_buffer(editor: &crate::editor::Editor, spec: &str) -> Option<rvim_types::BufferId> {
    if spec.is_empty() || spec == "%" {
        return Some(editor.current_buffer);
    }
    if spec == "$" {
        return editor.buffer_ids().last().copied();
    }
    if let Ok(number) = spec.parse::<u32>() {
        return editor.buffer_by_number(number);
    }
    editor.buffer_ids().into_iter().find(|id| {
        editor.buffers.get(*id).is_some_and(|buffer| {
            let name = buffer.name_string();
            name.as_str() == spec || name.ends_with(spec)
        })
    })
}

fn character_index(line: &[u8], byte_col: u32) -> u32 {
    let end = (byte_col as usize).min(line.len());
    let mut index = 0u32;
    let mut position = 0usize;
    while position < end {
        let mut next = position + 1;
        while next < end && (line[next] & 0xC0) == 0x80 {
            next += 1;
        }
        position = next;
        index += 1;
    }
    index
}

fn byte_column(line: &[u8], char_col: u32) -> u32 {
    let mut index = 0u32;
    let mut position = 0usize;
    while position < line.len() {
        if index >= char_col {
            break;
        }
        position += 1;
        while position < line.len() && (line[position] & 0xC0) == 0x80 {
            position += 1;
        }
        index += 1;
    }
    position.min(line.len()) as u32
}

/// Converts quickfix/location list entries to Vim dictionaries.
fn qf_entries_value(editor: &Editor, entries: &[crate::qf::QfEntry]) -> Value {
    let mut out = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let bufnr = editor
            .buffers
            .iter()
            .position(|(_, buffer)| buffer.name_string() == entry.file.display().to_string())
            .map_or(0, |index| index + 1);
        let mut dict = VimDict::new();
        dict.set(VimString::from("bufnr"), Value::Number(bufnr as i64));
        dict.set(
            VimString::from("filename"),
            Value::string(entry.file.display().to_string()),
        );
        dict.set(VimString::from("lnum"), Value::Number(entry.lnum as i64));
        dict.set(VimString::from("col"), Value::Number(entry.col as i64));
        dict.set(VimString::from("text"), Value::string(entry.text.as_str()));
        dict.set(VimString::from("valid"), Value::Number(1));
        dict.set(
            VimString::from("nr"),
            Value::Number(entry.nr.unwrap_or(index as i64 + 1)),
        );
        let kind = entry.kind.map_or(0, |kind| kind as i64);
        dict.set(VimString::from("type"), Value::Number(kind));
        out.push(Value::dict(dict));
    }
    Value::list(out)
}

/// Parses a list of Vim dictionaries into quickfix/location list entries.
fn qf_entries_from_value(value: &Value) -> Vec<crate::qf::QfEntry> {
    let Value::List(list) = value else {
        return Vec::new();
    };
    let items = list.borrow().items.clone();
    let mut parsed = Vec::new();
    for item in items {
        let Value::Dict(dict) = item else {
            continue;
        };
        let dict = dict.borrow();
        let filename = dict
            .get_str("filename")
            .map(|value| value.echo_string())
            .unwrap_or_default();
        if filename.is_empty() {
            continue;
        }
        let lnum = dict
            .get_str("lnum")
            .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
            .unwrap_or(0);
        let col = dict
            .get_str("col")
            .map(|value| value.echo_string().parse::<u32>().unwrap_or(0))
            .unwrap_or(0);
        let text = dict
            .get_str("text")
            .map(|value| value.echo_string())
            .unwrap_or_default();
        let kind = dict
            .get_str("type")
            .and_then(|value| value.echo_string().chars().next());
        let nr = dict
            .get_str("nr")
            .and_then(|value| value.echo_string().parse::<i64>().ok());
        parsed.push(crate::qf::QfEntry {
            file: std::path::PathBuf::from(filename),
            lnum: lnum.max(1),
            col: col.max(1),
            text,
            kind,
            nr,
        });
    }
    parsed
}

/// Resolves the optional window argument of getloclist()/setloclist().
fn loclist_number_arg(args: &[Value]) -> i64 {
    let Some(value) = args.first() else {
        return 0;
    };
    if let Value::Dict(dict) = value {
        let dict = dict.borrow();
        for key in ["nr", "winid"] {
            if let Some(value) = dict.get_str(key) {
                if let Ok(number) = value.echo_string().parse::<i64>() {
                    return number;
                }
            }
        }
        return 0;
    }
    value.echo_string().parse::<i64>().unwrap_or(0)
}
