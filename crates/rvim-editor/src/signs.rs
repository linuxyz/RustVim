//! Signs: type definitions, placements, display snapshots, the :sign command
//! and the sign_* VimL functions.
//!
//! Placements are anchored to the stable LineId of the line they were put on,
//! so they survive insertions, deletions and undo; the cached line number is
//! only a convenience and is reconciled lazily when the buffer changed.

use std::collections::HashMap;

use rvim_text::LineId;
use rvim_types::{BufPos, BufferId, VimError, VimString, WindowId};
use rvim_view::{SignCell, SignState};
use rvim_viml::value::VimDict;
use rvim_viml::Value;

use crate::editor::Editor;

/// A sign type defined with :sign define or sign_define().
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignDef {
    pub name: String,
    pub text: String,
    pub texthl: String,
    pub linehl: String,
    pub numhl: String,
    pub culhl: String,
    pub icon: String,
}

/// One placed sign.
#[derive(Clone, Debug)]
pub struct PlacedSign {
    pub id: i64,
    pub group: String,
    pub name: String,
    pub priority: i64,
    pub lnum: u32,
    pub line: LineId,
}

const DEFAULT_PRIORITY: i64 = 10;

impl Editor {
    /// Resolve a VimL buffer number (0 = current) to a buffer id.
    pub(crate) fn sign_buffer_id(&self, bufnr: i64) -> Option<BufferId> {
        if bufnr <= 0 {
            return Some(self.current_buffer);
        }
        self.buffers
            .iter()
            .find(|(raw, _)| *raw as i64 == bufnr)
            .map(|(_, entry)| entry.id)
    }

    /// Re-anchor sign line numbers after a buffer change; signs whose line was
    /// deleted are removed (like Vim's sign_mark_adjust).
    fn reconcile_signs(&mut self, buffer: BufferId) {
        let Some(entry) = self.buffers.get(buffer) else {
            return;
        };
        let tick = entry.text.meta.changedtick;
        if entry.signs_tick == tick {
            return;
        }
        let mut line_numbers: HashMap<LineId, u32> = HashMap::new();
        for (index, line) in entry.text.lines.iter().enumerate() {
            line_numbers.insert(line.id, index as u32 + 1);
        }
        let Some(entry) = self.buffers.get_mut(buffer) else {
            return;
        };
        entry.signs.retain_mut(|sign| {
            if let Some(lnum) = line_numbers.get(&sign.line).copied() {
                sign.lnum = lnum;
                true
            } else {
                false
            }
        });
        entry.signs.sort_by_key(|sign| (sign.lnum, -sign.priority));
        entry.signs_tick = tick;
    }

    /// Record that the sign configuration changed.
    fn touch_signs(&mut self) {
        self.sign_version = self.sign_version.wrapping_add(1);
    }

    /// Refresh the per-window sign snapshots used by the renderer.
    pub(crate) fn sync_signs(&mut self) {
        let version = self.sign_version;
        let ids: Vec<WindowId> = self.view.windows.keys().collect();
        for id in ids {
            let Some(window) = self.view.windows.get(&id) else {
                continue;
            };
            let buffer = window.buffer;
            let Some(entry) = self.buffers.get(buffer) else {
                continue;
            };
            let tick = entry.text.meta.changedtick;
            let raw = rvim_types::Id::raw(buffer);
            let current = &window.signs;
            if current.buffer == raw && current.changedtick == tick && current.version == version {
                continue;
            }
            self.reconcile_signs(buffer);
            let state = self.sign_state_for(buffer, tick);
            if let Some(window) = self.view.window_mut(id) {
                window.signs = state;
            }
        }
    }

    fn sign_state_for(&self, buffer: BufferId, tick: u64) -> SignState {
        let mut state = SignState {
            buffer: rvim_types::Id::raw(buffer),
            changedtick: tick,
            version: self.sign_version,
            ..SignState::default()
        };
        let Some(entry) = self.buffers.get(buffer) else {
            return state;
        };
        state.present = !entry.signs.is_empty();
        for sign in &entry.signs {
            if state.cells.contains_key(&sign.lnum) {
                // The list is sorted by line and priority, so the first sign
                // on a line is the one Vim displays.
                continue;
            }
            let Some(def) = self.sign_defs.get(&sign.name) else {
                continue;
            };
            state.cells.insert(
                sign.lnum,
                SignCell {
                    text: def.text.clone(),
                    text_hl: def.texthl.clone(),
                    line_hl: def.linehl.clone(),
                    num_hl: def.numhl.clone(),
                },
            );
        }
        state
    }

    /// Line number of the first placed sign \p id in \p group.
    fn find_sign(&self, buffer: BufferId, id: i64, group: &str) -> Option<PlacedSign> {
        let entry = self.buffers.get(buffer)?;
        entry
            .signs
            .iter()
            .find(|sign| sign.id == id && group_matches(&sign.group, group))
            .cloned()
    }

    fn define_sign(&mut self, name: &str, fields: &[(&str, &str)]) -> Result<(), VimError> {
        if name.is_empty() {
            return Err(VimError::new(156, "Missing sign name"));
        }
        let mut def = self
            .sign_defs
            .get(name)
            .cloned()
            .unwrap_or_else(|| SignDef {
                name: name.to_string(),
                ..SignDef::default()
            });
        for (field, value) in fields {
            match *field {
                "text" => {
                    let width = rvim_types::unicode::display_width(value.as_bytes(), 0, 8) as usize;
                    if width > 2 {
                        return Err(VimError::new(239, format!("Invalid sign text: {value}")));
                    }
                    def.text = (*value).to_string();
                }
                "texthl" => def.texthl = (*value).to_string(),
                "linehl" => def.linehl = (*value).to_string(),
                "numhl" => def.numhl = (*value).to_string(),
                "culhl" => def.culhl = (*value).to_string(),
                "icon" => def.icon = (*value).to_string(),
                other => {
                    return Err(VimError::new(
                        474,
                        format!("Invalid argument: {other}={value}"),
                    ))
                }
            }
        }
        self.sign_defs.insert(name.to_string(), def);
        self.touch_signs();
        Ok(())
    }

    fn undefine_sign(&mut self, name: &str) -> Result<(), VimError> {
        if self.sign_defs.remove(name).is_none() {
            return Err(VimError::new(155, format!("Unknown sign: {name}")));
        }
        self.touch_signs();
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn place_sign(
        &mut self,
        buffer: BufferId,
        id: i64,
        group: &str,
        name: &str,
        lnum: u32,
        priority: i64,
    ) -> Result<i64, VimError> {
        if !self.sign_defs.contains_key(name) {
            return Err(VimError::new(155, format!("Unknown sign: {name}")));
        }
        self.reconcile_signs(buffer);
        let Some(entry) = self.buffers.get(buffer) else {
            return Err(VimError::new(158, "Invalid buffer"));
        };
        if lnum < 1 || lnum > entry.text.line_count() {
            return Err(VimError::new(474, format!("Invalid argument: line={lnum}")));
        }
        let line = entry.text.lines.line(lnum).map(|line| line.id);
        let Some(line) = line else {
            return Err(VimError::new(474, "Invalid argument: line"));
        };
        let mut id = id;
        let entry = self.buffers.get_mut(buffer).expect("buffer checked");
        if id <= 0 {
            id = entry.sign_next_id;
            entry.sign_next_id += 1;
        } else {
            // Re-placing an existing id in the same group moves the sign.
            entry
                .signs
                .retain(|sign| !(sign.id == id && sign.group == group));
        }
        entry.signs.push(PlacedSign {
            id,
            group: group.to_string(),
            name: name.to_string(),
            priority,
            lnum,
            line,
        });
        entry.signs_tick = 0;
        self.sign_version = self.sign_version.wrapping_add(1);
        Ok(id)
    }

    fn unplace_signs(
        &mut self,
        buffer: BufferId,
        id: i64,
        group: Option<&str>,
        lnum: Option<u32>,
    ) -> Result<(), VimError> {
        self.reconcile_signs(buffer);
        let Some(entry) = self.buffers.get_mut(buffer) else {
            return Err(VimError::new(158, "Invalid buffer"));
        };
        let group = group.unwrap_or("");
        entry.signs.retain(|sign| {
            let matches_group = match group {
                "*" => true,
                other => sign.group == other,
            };
            let matches_id = id == 0 || sign.id == id;
            let matches_line = lnum.is_none_or(|lnum| sign.lnum == lnum);
            !(matches_group && matches_id && matches_line)
        });
        entry.signs_tick = 0;
        self.sign_version = self.sign_version.wrapping_add(1);
        Ok(())
    }

    fn sign_jump_to(&mut self, buffer: BufferId, id: i64, group: &str) -> Result<u32, VimError> {
        self.reconcile_signs(buffer);
        let Some(sign) = self.find_sign(buffer, id, group) else {
            return Err(VimError::new(157, format!("Invalid sign ID: {id}")));
        };
        if self.current_buffer != buffer {
            self.switch_to_buffer(buffer)?;
        }
        self.set_cursor(BufPos::new(sign.lnum, 0));
        self.goto_motion(rvim_edit::Motion::FirstNonBlank);
        Ok(sign.lnum)
    }

    /// Execute the :sign command line (without the "sign" keyword).
    pub(crate) fn execute_sign(&mut self, args: &str) -> Result<(), VimError> {
        let tokens: Vec<&str> = args.split_whitespace().collect();
        let Some(subcommand) = tokens.first().copied() else {
            return Err(VimError::new(160, "Unknown sign command: "));
        };
        let rest = &tokens[1..];
        match subcommand {
            "define" => {
                let Some(name) = rest.first().copied() else {
                    return Err(VimError::new(156, "Missing sign name"));
                };
                let mut fields = Vec::new();
                for token in &rest[1..] {
                    let Some((field, value)) = token.split_once('=') else {
                        return Err(VimError::new(474, format!("Invalid argument: {token}")));
                    };
                    fields.push((field, value));
                }
                self.define_sign(name, &fields)?;
                self.sync_signs();
                Ok(())
            }
            "undefine" => {
                let Some(name) = rest.first().copied() else {
                    return Err(VimError::new(156, "Missing sign name"));
                };
                self.undefine_sign(name)?;
                self.sync_signs();
                Ok(())
            }
            "place" => {
                let Some(first) = rest.first().copied() else {
                    return Err(VimError::new(159, "Missing sign number"));
                };
                let id = parse_sign_id(first)?;
                let mut group = "";
                let mut name = "";
                let mut lnum = None;
                let mut priority = DEFAULT_PRIORITY;
                let mut file = None;
                for token in &rest[1..] {
                    let Some((field, value)) = token.split_once('=') else {
                        return Err(VimError::new(474, format!("Invalid argument: {token}")));
                    };
                    match field {
                        "group" => group = value,
                        "name" => name = value,
                        "line" => {
                            lnum = Some(value.parse::<u32>().map_err(|_| {
                                VimError::new(474, format!("Invalid argument: line={value}"))
                            })?)
                        }
                        "priority" => {
                            priority = value.parse::<i64>().map_err(|_| {
                                VimError::new(474, format!("Invalid argument: priority={value}"))
                            })?
                        }
                        "file" => file = Some(value),
                        _ => return Err(VimError::new(474, format!("Invalid argument: {token}"))),
                    }
                }
                let buffer = self.sign_command_buffer(file)?;
                let Some(lnum) = lnum else {
                    return Err(VimError::new(474, "Invalid argument: line"));
                };
                if name.is_empty() {
                    return Err(VimError::new(474, "Invalid argument: name"));
                }
                self.place_sign(buffer, id, group, name, lnum, priority)?;
                self.sync_signs();
                Ok(())
            }
            "unplace" => {
                let Some(first) = rest.first().copied() else {
                    return Err(VimError::new(159, "Missing sign number"));
                };
                let id = if first == "*" {
                    0
                } else {
                    parse_sign_id(first)?
                };
                let mut group = None;
                let mut lnum = None;
                let mut file = None;
                for token in &rest[1..] {
                    let Some((field, value)) = token.split_once('=') else {
                        return Err(VimError::new(474, format!("Invalid argument: {token}")));
                    };
                    match field {
                        "group" => group = Some(value),
                        "line" => {
                            lnum = Some(value.parse::<u32>().map_err(|_| {
                                VimError::new(474, format!("Invalid argument: line={value}"))
                            })?)
                        }
                        "file" => file = Some(value),
                        _ => return Err(VimError::new(474, format!("Invalid argument: {token}"))),
                    }
                }
                let buffer = self.sign_command_buffer(file)?;
                self.unplace_signs(buffer, id, group, lnum)?;
                self.sync_signs();
                Ok(())
            }
            "jump" => {
                let Some(first) = rest.first().copied() else {
                    return Err(VimError::new(159, "Missing sign number"));
                };
                let id = parse_sign_id(first)?;
                let mut group = "";
                let mut file = None;
                for token in &rest[1..] {
                    let Some((field, value)) = token.split_once('=') else {
                        return Err(VimError::new(474, format!("Invalid argument: {token}")));
                    };
                    match field {
                        "group" => group = value,
                        "file" => file = Some(value),
                        _ => return Err(VimError::new(474, format!("Invalid argument: {token}"))),
                    }
                }
                let buffer = self.sign_command_buffer(file)?;
                self.sign_jump_to(buffer, id, group)?;
                Ok(())
            }
            "list" => {
                let requested = rest.first().copied();
                let mut lines = Vec::new();
                let mut names: Vec<&String> = self.sign_defs.keys().collect();
                names.sort();
                for name in names {
                    if requested.is_some_and(|wanted| wanted != name) {
                        continue;
                    }
                    let def = &self.sign_defs[name];
                    let mut line = format!("sign {} ", def.name);
                    if !def.text.is_empty() {
                        line.push_str(&format!("text={} ", def.text));
                    }
                    for (key, value) in [
                        ("texthl", &def.texthl),
                        ("linehl", &def.linehl),
                        ("numhl", &def.numhl),
                        ("culhl", &def.culhl),
                        ("icon", &def.icon),
                    ] {
                        if !value.is_empty() {
                            line.push_str(&format!("{key}={value} "));
                        }
                    }
                    lines.push(line.trim_end().to_string());
                }
                if lines.is_empty() {
                    if let Some(name) = requested {
                        return Err(VimError::new(155, format!("Unknown sign: {name}")));
                    }
                    self.messages.info("No signs defined");
                } else {
                    self.messages.info(lines.join("\n"));
                }
                Ok(())
            }
            _ => Err(VimError::new(
                160,
                format!("Unknown sign command: {subcommand}"),
            )),
        }
    }

    /// Buffer named by the optional file= argument of :sign.
    fn sign_command_buffer(&mut self, file: Option<&str>) -> Result<BufferId, VimError> {
        let Some(file) = file else {
            return Ok(self.current_buffer);
        };
        self.buffers
            .iter()
            .find(|(_, entry)| entry.short_name() == file || entry.name_string() == file)
            .map(|(_, entry)| entry.id)
            .ok_or_else(|| VimError::new(158, format!("Invalid buffer: {file}")))
    }

    /// Entry point for the sign_* VimL functions.
    pub(crate) fn sign_call_viml(&mut self, name: &str, args: &[Value]) -> Result<Value, VimError> {
        match name {
            "sign_define" => {
                let sign_name = args
                    .first()
                    .and_then(value_string)
                    .ok_or_else(|| VimError::new(474, "Invalid argument"))?;
                let mut fields: Vec<(String, String)> = Vec::new();
                match args.get(1) {
                    Some(Value::Dict(dict)) => {
                        let dict = dict.borrow();
                        for key in ["text", "texthl", "linehl", "numhl", "culhl", "icon"] {
                            if let Some(value) = dict.get(&VimString::from(key)) {
                                if let Some(text) = value_string(value) {
                                    fields.push((key.to_string(), text));
                                }
                            }
                        }
                    }
                    Some(value) => {
                        if let Some(text) = value_string(value) {
                            fields.push(("text".to_string(), text));
                        }
                    }
                    None => {}
                }
                let refs: Vec<(&str, &str)> = fields
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str()))
                    .collect();
                Ok(self.sign_define_viml(&sign_name, &refs))
            }
            "sign_undefine" => {
                let sign_name = args.first().and_then(value_string).unwrap_or_default();
                Ok(self.sign_undefine_viml(&sign_name))
            }
            "sign_getdefined" => {
                let sign_name = args.first().and_then(value_string);
                Ok(self.sign_getdefined_viml(sign_name.as_deref()))
            }
            "sign_place" => {
                let id = args.first().and_then(value_number).unwrap_or(0);
                let group = args.get(1).and_then(value_string).unwrap_or_default();
                let sign_name = args.get(2).and_then(value_string).unwrap_or_default();
                let bufnr = args.get(3).and_then(value_number).unwrap_or(0);
                let (lnum, priority) = match args.get(4) {
                    Some(Value::Dict(dict)) => {
                        let dict = dict.borrow();
                        let lnum = dict.get_str("lnum").and_then(value_number).unwrap_or(0);
                        let priority = dict
                            .get_str("priority")
                            .and_then(value_number)
                            .unwrap_or(DEFAULT_PRIORITY);
                        (lnum, priority)
                    }
                    _ => (0, DEFAULT_PRIORITY),
                };
                Ok(self.sign_place_viml(id, &group, &sign_name, bufnr, lnum, priority))
            }
            "sign_unplace" => {
                let group = args.first().and_then(value_string).unwrap_or_default();
                let (bufnr, id) = match args.get(1) {
                    Some(Value::Dict(dict)) => {
                        let dict = dict.borrow();
                        (
                            dict.get_str("bufnr").and_then(value_number).unwrap_or(0),
                            dict.get_str("id").and_then(value_number).unwrap_or(0),
                        )
                    }
                    _ => (0, 0),
                };
                Ok(self.sign_unplace_viml(&group, bufnr, id, None))
            }
            "sign_getplaced" => {
                let bufnr = args.first().and_then(value_number).unwrap_or(0);
                let (group, id, lnum) = match args.get(1) {
                    Some(Value::Dict(dict)) => {
                        let dict = dict.borrow();
                        (
                            dict.get_str("group").and_then(value_string),
                            dict.get_str("id").and_then(value_number),
                            dict.get_str("lnum")
                                .and_then(value_number)
                                .map(|lnum| lnum as u32),
                        )
                    }
                    _ => (None, None, None),
                };
                Ok(self.sign_getplaced_viml(bufnr, group.as_deref(), id, lnum))
            }
            "sign_jump" => {
                let id = args.first().and_then(value_number).unwrap_or(0);
                let group = args.get(1).and_then(value_string).unwrap_or_default();
                let bufnr = args.get(2).and_then(value_number).unwrap_or(0);
                Ok(self.sign_jump_viml(id, &group, bufnr))
            }
            "sign_placelist" => {
                let mut results = Vec::new();
                if let Some(Value::List(list)) = args.first() {
                    let list = list.borrow();
                    for item in &list.items {
                        let Value::Dict(dict) = item else {
                            results.push(Value::Number(-1));
                            continue;
                        };
                        let dict = dict.borrow();
                        let id = dict.get_str("id").and_then(value_number).unwrap_or(0);
                        let group = dict
                            .get_str("group")
                            .and_then(value_string)
                            .unwrap_or_default();
                        let sign_name = dict
                            .get_str("name")
                            .and_then(value_string)
                            .unwrap_or_default();
                        let bufnr = dict
                            .get_str("buffer")
                            .or_else(|| dict.get_str("bufnr"))
                            .and_then(value_number)
                            .unwrap_or(0);
                        let lnum = dict.get_str("lnum").and_then(value_number).unwrap_or(0);
                        let priority = dict
                            .get_str("priority")
                            .and_then(value_number)
                            .unwrap_or(DEFAULT_PRIORITY);
                        results.push(
                            self.sign_place_viml(id, &group, &sign_name, bufnr, lnum, priority),
                        );
                    }
                }
                Ok(Value::list(results))
            }
            "sign_unplacelist" => {
                let mut results = Vec::new();
                if let Some(Value::List(list)) = args.first() {
                    let list = list.borrow();
                    for item in &list.items {
                        let Value::Dict(dict) = item else {
                            results.push(Value::Number(-1));
                            continue;
                        };
                        let dict = dict.borrow();
                        let group = dict
                            .get_str("group")
                            .and_then(value_string)
                            .unwrap_or_default();
                        let bufnr = dict
                            .get_str("buffer")
                            .or_else(|| dict.get_str("bufnr"))
                            .and_then(value_number)
                            .unwrap_or(0);
                        let id = dict.get_str("id").and_then(value_number).unwrap_or(0);
                        results.push(self.sign_unplace_viml(&group, bufnr, id, None));
                    }
                }
                Ok(Value::list(results))
            }
            _ => Err(VimError::new(117, format!("Unknown function: {name}"))),
        }
    }

    /// sign_define()
    pub(crate) fn sign_define_viml(&mut self, name: &str, fields: &[(&str, &str)]) -> Value {
        match self.define_sign(name, fields) {
            Ok(()) => {
                self.sync_signs();
                Value::Number(0)
            }
            Err(_) => Value::Number(-1),
        }
    }

    /// sign_undefine()
    pub(crate) fn sign_undefine_viml(&mut self, name: &str) -> Value {
        match self.undefine_sign(name) {
            Ok(()) => {
                self.sync_signs();
                Value::Number(0)
            }
            Err(_) => Value::Number(-1),
        }
    }

    /// sign_getdefined([name])
    pub(crate) fn sign_getdefined_viml(&self, name: Option<&str>) -> Value {
        let mut names: Vec<&String> = self.sign_defs.keys().collect();
        names.sort();
        let mut list = Vec::new();
        for def_name in names {
            if name.is_some_and(|wanted| wanted != def_name) {
                continue;
            }
            let def = &self.sign_defs[def_name];
            let mut dict = VimDict::new();
            dict.set(VimString::from("name"), Value::string(def.name.as_str()));
            if !def.text.is_empty() {
                dict.set(VimString::from("text"), Value::string(def.text.as_str()));
            }
            for (key, value) in [
                ("texthl", &def.texthl),
                ("linehl", &def.linehl),
                ("numhl", &def.numhl),
                ("culhl", &def.culhl),
                ("icon", &def.icon),
            ] {
                if !value.is_empty() {
                    dict.set(VimString::from(key), Value::string(value.as_str()));
                }
            }
            list.push(Value::dict(dict));
        }
        Value::list(list)
    }

    /// sign_place()
    pub(crate) fn sign_place_viml(
        &mut self,
        id: i64,
        group: &str,
        name: &str,
        bufnr: i64,
        lnum: i64,
        priority: i64,
    ) -> Value {
        let Some(buffer) = self.sign_buffer_id(bufnr) else {
            return Value::Number(-1);
        };
        if lnum < 1 {
            return Value::Number(-1);
        }
        match self.place_sign(buffer, id, group, name, lnum as u32, priority) {
            Ok(id) => {
                self.sync_signs();
                Value::Number(id)
            }
            Err(_) => Value::Number(-1),
        }
    }

    /// sign_unplace()
    pub(crate) fn sign_unplace_viml(
        &mut self,
        group: &str,
        bufnr: i64,
        id: i64,
        lnum: Option<u32>,
    ) -> Value {
        let Some(buffer) = self.sign_buffer_id(bufnr) else {
            return Value::Number(-1);
        };
        match self.unplace_signs(buffer, id, Some(group), lnum) {
            Ok(()) => {
                self.sync_signs();
                Value::Number(0)
            }
            Err(_) => Value::Number(-1),
        }
    }

    /// sign_getplaced()
    pub(crate) fn sign_getplaced_viml(
        &mut self,
        bufnr: i64,
        group: Option<&str>,
        id: Option<i64>,
        lnum: Option<u32>,
    ) -> Value {
        let Some(buffer) = self.sign_buffer_id(bufnr) else {
            return Value::Number(-1);
        };
        // Line numbers follow edits, so refresh the anchors for the query.
        self.reconcile_signs(buffer);
        let mut outer = VimDict::new();
        outer.set(
            VimString::from("bufnr"),
            Value::Number(rvim_types::Id::raw(buffer) as i64),
        );
        let mut signs = Vec::new();
        if let Some(entry) = self.buffers.get(buffer) {
            for sign in &entry.signs {
                if group.is_some_and(|wanted| !group_matches(&sign.group, wanted)) {
                    continue;
                }
                if id.is_some_and(|wanted| sign.id != wanted) {
                    continue;
                }
                if lnum.is_some_and(|wanted| sign.lnum != wanted) {
                    continue;
                }
                let mut dict = VimDict::new();
                dict.set(VimString::from("id"), Value::Number(sign.id));
                dict.set(VimString::from("lnum"), Value::Number(i64::from(sign.lnum)));
                dict.set(VimString::from("name"), Value::string(sign.name.as_str()));
                dict.set(VimString::from("group"), Value::string(sign.group.as_str()));
                dict.set(VimString::from("priority"), Value::Number(sign.priority));
                signs.push(Value::dict(dict));
            }
        }
        outer.set(VimString::from("signs"), Value::list(signs));
        Value::list(vec![Value::dict(outer)])
    }

    /// sign_jump()
    pub(crate) fn sign_jump_viml(&mut self, id: i64, group: &str, bufnr: i64) -> Value {
        let Some(buffer) = self.sign_buffer_id(bufnr) else {
            return Value::Number(-1);
        };
        match self.sign_jump_to(buffer, id, group) {
            Ok(lnum) => Value::Number(i64::from(lnum)),
            Err(_) => Value::Number(-1),
        }
    }
}

fn group_matches(sign_group: &str, wanted: &str) -> bool {
    wanted == "*" || sign_group == wanted
}

fn parse_sign_id(text: &str) -> Result<i64, VimError> {
    let id = text
        .parse::<i64>()
        .map_err(|_| VimError::new(157, format!("Invalid sign ID: {text}")))?;
    if id < 0 {
        return Err(VimError::new(157, format!("Invalid sign ID: {text}")));
    }
    Ok(id)
}

fn value_number(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => Some(*number),
        Value::Bool(flag) => Some(i64::from(*flag)),
        Value::String(text) => text.to_string_lossy().trim().parse().ok(),
        _ => None,
    }
}

fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_string_lossy().into_owned()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some((if *flag { "1" } else { "0" }).to_string()),
        _ => None,
    }
}
