//! Text properties: type definitions, placements, queries and display data.
//!
//! Properties are anchored to the stable LineId of the lines they start and
//! end on, so line insertions/deletions/undo keep them with their text.  Column
//! positions are clamped when the line changes; insertions/deletions *inside*
//! a line do not shift columns (documented approximation).

use std::collections::HashMap;

use rvim_text::LineId;
use rvim_types::{BufferId, VimError, VimString};
use rvim_ui_api::Attr;
use rvim_view::VirtualText;
use rvim_viml::value::VimDict;
use rvim_viml::Value;

use crate::editor::Editor;

/// A text property type (prop_type_add()).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropType {
    pub name: String,
    pub highlight: String,
    pub combine: bool,
    pub priority: i64,
    pub override_visual: bool,
    pub start_incl: bool,
    pub end_incl: bool,
    /// 0 for a global type, buffer number for a buffer-local type.
    pub bufnr: i64,
}

impl Default for PropType {
    fn default() -> Self {
        Self {
            name: String::new(),
            highlight: String::new(),
            combine: true,
            priority: 0,
            override_visual: false,
            start_incl: false,
            end_incl: false,
            bufnr: 0,
        }
    }
}

/// One placed text property.
#[derive(Clone, Debug)]
pub struct PlacedProp {
    pub id: i64,
    pub type_name: String,
    pub type_bufnr: i64,
    pub start_line: LineId,
    pub start_lnum: u32,
    /// Zero-based byte column, inclusive.
    pub start_col: u32,
    pub end_line: LineId,
    pub end_lnum: u32,
    /// Zero-based byte column, exclusive.
    pub end_col: u32,
    /// Virtual text displayed before start_col (0 means "after the line" when
    /// text_align is "after").
    pub text: String,
    pub text_align: String,
    pub text_padding_left: i64,
    pub text_wrap: String,
}

/// A highlight span produced by a text property.
#[derive(Clone, Debug)]
pub(crate) struct PropSpan {
    pub start: u32,
    pub end: u32,
    pub attr: Attr,
    pub priority: i64,
    pub combine: bool,
}

impl Editor {
    /// Resolve a VimL buffer number (0 = current).
    pub(crate) fn prop_buffer_id(&self, bufnr: i64) -> Option<BufferId> {
        if bufnr <= 0 {
            return Some(self.current_buffer);
        }
        self.buffers
            .iter()
            .find(|(raw, _)| *raw as i64 == bufnr)
            .map(|(_, entry)| entry.id)
    }

    fn prop_type_for(&self, buffer: BufferId, name: &str) -> Option<PropType> {
        let entry = self.buffers.get(buffer)?;
        if let Some(local) = entry.prop_types.get(name) {
            return Some(local.clone());
        }
        self.prop_types.get(name).cloned()
    }

    /// Re-anchor property line numbers after buffer changes and clamp columns.
    fn reconcile_props(&mut self, buffer: BufferId) {
        let Some(entry) = self.buffers.get(buffer) else {
            return;
        };
        let tick = entry.text.meta.changedtick;
        if entry.props_tick == tick {
            return;
        }
        let mut line_numbers: HashMap<LineId, (u32, u32)> = HashMap::new();
        for (index, line) in entry.text.lines.iter().enumerate() {
            line_numbers.insert(line.id, (index as u32 + 1, line.bytes.len() as u32));
        }
        let Some(entry) = self.buffers.get_mut(buffer) else {
            return;
        };
        entry.text_props.retain_mut(|prop| {
            let Some((start_lnum, start_len)) = line_numbers.get(&prop.start_line).copied() else {
                return false;
            };
            let Some((end_lnum, end_len)) = line_numbers.get(&prop.end_line).copied() else {
                return false;
            };
            prop.start_lnum = start_lnum;
            prop.end_lnum = end_lnum.max(start_lnum);
            prop.start_col = prop.start_col.min(start_len);
            prop.end_col = prop.end_col.min(end_len);
            if prop.end_lnum == prop.start_lnum && prop.end_col < prop.start_col {
                prop.end_col = prop.start_col;
            }
            true
        });
        entry
            .text_props
            .sort_by_key(|prop| (prop.start_lnum, prop.start_col, -prop.id.abs()));
        entry.props_tick = tick;
    }

    /// Reconcile the text properties of the current buffer.
    pub(crate) fn sync_text_props_current(&mut self) {
        let buffer = self.current_buffer;
        self.reconcile_props(buffer);
    }

    /// Reconcile the text properties of \p buffer.
    pub(crate) fn sync_text_props_for(&mut self, buffer: BufferId) {
        self.reconcile_props(buffer);
    }

    // ----- display helpers -------------------------------------------------

    /// Highlight spans of text properties on \p lnum, ordered by priority
    /// (lowest first so the renderer can overlay the highest last).
    pub(crate) fn prop_highlight_spans(&self, buffer: BufferId, lnum: u32) -> Vec<PropSpan> {
        let Some(entry) = self.buffers.get(buffer) else {
            return Vec::new();
        };
        let line_len = entry.text.line_len(lnum);
        let mut spans = Vec::new();
        for prop in &entry.text_props {
            if prop.text_align_is_above_or_below() {
                continue;
            }
            let Some(def) = self.prop_type_for(buffer, &prop.type_name) else {
                continue;
            };
            if def.highlight.is_empty() {
                continue;
            }
            let Some((start, end)) = prop_line_range(prop, lnum, line_len) else {
                continue;
            };
            if start == end && !prop.text.is_empty() {
                continue;
            }
            let mut end = end;
            if start == end && start < line_len {
                end = (start + 1).min(line_len);
            }
            spans.push(PropSpan {
                start,
                end,
                attr: self.highlights.get(&def.highlight),
                priority: def.priority,
                combine: def.combine,
            });
        }
        spans.sort_by_key(|span| span.priority);
        spans
    }

    /// Virtual text attached to \p lnum, ordered by column.
    pub(crate) fn prop_virtual_texts(&self, buffer: BufferId, lnum: u32) -> Vec<VirtualText> {
        let Some(entry) = self.buffers.get(buffer) else {
            return Vec::new();
        };
        let line_len = entry.text.line_len(lnum);
        let mut texts = Vec::new();
        for prop in &entry.text_props {
            if prop.text.is_empty() || prop.start_lnum != lnum {
                continue;
            }
            let col = if prop.start_col == 0 {
                if !prop.text_align.is_empty() && prop.text_align != "after" {
                    // "above"/"below"/"right" virtual text is not rendered yet.
                    continue;
                }
                line_len
            } else {
                prop.start_col.min(line_len)
            };
            let attr = self
                .prop_type_for(buffer, &prop.type_name)
                .map(|def| {
                    if def.highlight.is_empty() {
                        Attr::DEFAULT
                    } else {
                        self.highlights.get(&def.highlight)
                    }
                })
                .unwrap_or_default();
            texts.push(VirtualText {
                col,
                text: prop.text.clone(),
                attr,
            });
        }
        texts.sort_by_key(|text| text.col);
        texts
    }

    // ----- prop_type_* -----------------------------------------------------

    fn apply_type_dict(
        &mut self,
        buffer: BufferId,
        name: &str,
        dict: &VimDict,
    ) -> Result<(), VimError> {
        let existing = self.prop_type_for(buffer, name);
        let mut def = existing.unwrap_or_else(|| PropType {
            name: name.to_string(),
            ..PropType::default()
        });
        if let Some(value) = dict.get_str("highlight") {
            def.highlight = value_string(value).unwrap_or_default();
        }
        if let Some(value) = dict.get_str("priority") {
            def.priority = value_number(value).unwrap_or(0);
        }
        if let Some(value) = dict.get_str("combine") {
            def.combine = value_number(value).unwrap_or(1) != 0;
        }
        if let Some(value) = dict.get_str("override") {
            def.override_visual = value_number(value).unwrap_or(0) != 0;
        }
        if let Some(value) = dict.get_str("start_incl") {
            def.start_incl = value_number(value).unwrap_or(0) != 0;
        }
        if let Some(value) = dict.get_str("end_incl") {
            def.end_incl = value_number(value).unwrap_or(0) != 0;
        }
        let bufnr = dict.get_str("bufnr").and_then(value_number).unwrap_or(0);
        def.bufnr = bufnr;
        let target = if bufnr > 0 {
            self.prop_buffer_id(bufnr)
                .ok_or_else(|| VimError::new(158, "Invalid buffer"))?
        } else {
            buffer
        };
        if !def.highlight.is_empty() && !self.highlights.contains(&def.highlight) {
            return Err(VimError::new(
                970,
                format!("Unknown highlight group name: '{}'", def.highlight),
            ));
        }
        if bufnr > 0 {
            if let Some(entry) = self.buffers.get_mut(target) {
                entry.prop_types.insert(name.to_string(), def);
            }
        } else {
            self.prop_types.insert(name.to_string(), def);
        }
        Ok(())
    }

    fn prop_type_add_impl(&mut self, name: &str, dict: &VimDict) -> Result<(), VimError> {
        if name.is_empty() {
            return Err(VimError::new(965, "Missing property type name"));
        }
        let buffer = self.current_buffer;
        let bufnr = dict.get_str("bufnr").and_then(value_number).unwrap_or(0);
        let target = if bufnr > 0 {
            self.prop_buffer_id(bufnr)
                .ok_or_else(|| VimError::new(158, "Invalid buffer"))?
        } else {
            buffer
        };
        let exists = if bufnr > 0 {
            self.buffers
                .get(target)
                .is_some_and(|entry| entry.prop_types.contains_key(name))
        } else {
            self.prop_types.contains_key(name)
        };
        if exists {
            return Err(VimError::new(
                969,
                format!("Property type {name} already defined"),
            ));
        }
        self.apply_type_dict(buffer, name, dict)
    }

    fn prop_type_change_impl(&mut self, name: &str, dict: &VimDict) -> Result<(), VimError> {
        if name.is_empty() {
            return Err(VimError::new(965, "Missing property type name"));
        }
        let buffer = self.current_buffer;
        if self.prop_type_for(buffer, name).is_none() {
            return Err(VimError::new(
                971,
                format!("Property type {name} does not exist"),
            ));
        }
        self.apply_type_dict(buffer, name, dict)
    }

    fn prop_type_delete_impl(
        &mut self,
        name: &str,
        dict: Option<&VimDict>,
    ) -> Result<(), VimError> {
        let bufnr = dict
            .and_then(|dict| dict.get_str("bufnr"))
            .and_then(value_number)
            .unwrap_or(0);
        if bufnr > 0 {
            let Some(target) = self.prop_buffer_id(bufnr) else {
                return Err(VimError::new(158, "Invalid buffer"));
            };
            if let Some(entry) = self.buffers.get_mut(target) {
                entry.prop_types.remove(name);
            }
        } else {
            self.prop_types.remove(name);
        }
        Ok(())
    }

    fn prop_type_get_impl(&mut self, name: &str, dict: Option<&VimDict>) -> Value {
        let bufnr = dict
            .and_then(|dict| dict.get_str("bufnr"))
            .and_then(value_number)
            .unwrap_or(0);
        let buffer = if bufnr > 0 {
            self.prop_buffer_id(bufnr)
        } else {
            Some(self.current_buffer)
        };
        let Some(def) = buffer.and_then(|buffer| self.prop_type_for(buffer, name)) else {
            return Value::dict(VimDict::new());
        };
        let mut out = VimDict::new();
        out.set(VimString::from("priority"), Value::Number(def.priority));
        out.set(
            VimString::from("combine"),
            Value::Number(i64::from(def.combine)),
        );
        out.set(
            VimString::from("override"),
            Value::Number(i64::from(def.override_visual)),
        );
        out.set(
            VimString::from("start_incl"),
            Value::Number(i64::from(def.start_incl)),
        );
        out.set(
            VimString::from("end_incl"),
            Value::Number(i64::from(def.end_incl)),
        );
        out.set(VimString::from("bufnr"), Value::Number(def.bufnr));
        if !def.highlight.is_empty() {
            out.set(
                VimString::from("highlight"),
                Value::string(def.highlight.as_str()),
            );
        }
        Value::dict(out)
    }

    fn prop_type_list_impl(&mut self, dict: Option<&VimDict>) -> Value {
        let bufnr = dict
            .and_then(|dict| dict.get_str("bufnr"))
            .and_then(value_number)
            .unwrap_or(0);
        let mut names: Vec<String> = Vec::new();
        if bufnr > 0 {
            if let Some(buffer) = self.prop_buffer_id(bufnr) {
                if let Some(entry) = self.buffers.get(buffer) {
                    names.extend(entry.prop_types.keys().cloned());
                }
            }
        } else {
            names.extend(self.prop_types.keys().cloned());
        }
        names.sort();
        Value::list(names.into_iter().map(Value::string).collect())
    }

    // ----- prop_add / remove / list / find ---------------------------------

    fn prop_add_dictionary(
        &mut self,
        bufnr: i64,
        lnum: i64,
        col: i64,
        props: &VimDict,
    ) -> Result<i64, VimError> {
        let buffer = if bufnr > 0 {
            self.prop_buffer_id(bufnr)
                .ok_or_else(|| VimError::new(158, "Invalid buffer"))?
        } else {
            self.current_buffer
        };
        self.sync_text_props_for(buffer);
        let Some(type_name) = props
            .get_str("type")
            .and_then(value_string)
            .filter(|name| !name.is_empty())
        else {
            return Err(VimError::new(965, "Missing property type name"));
        };
        let Some(entry) = self.buffers.get(buffer) else {
            return Err(VimError::new(158, "Invalid buffer"));
        };
        if lnum < 1 || lnum as u32 > entry.text.line_count() {
            return Err(VimError::new(966, format!("Invalid line number: {lnum}")));
        }
        let line_len = entry.text.line_len(lnum as u32);
        let Some(type_def) = self.prop_type_for(buffer, &type_name) else {
            return Err(VimError::new(
                971,
                format!("Property type {type_name} does not exist"),
            ));
        };
        let text = props.get_str("text").and_then(value_string);
        let mut id = props.get_str("id").and_then(value_number).unwrap_or(0);
        if text.is_some() {
            if id < 0 {
                return Err(VimError::new(
                    1293,
                    "Cannot use a negative id for a text property",
                ));
            }
            for forbidden in ["length", "end_col", "end_lnum"] {
                if props.get_str(forbidden).is_some() {
                    return Err(VimError::new(
                        1305,
                        "Cannot use \"length\", \"end_col\" and \"end_lnum\" with \"text\"",
                    ));
                }
            }
        } else if id < 0 {
            return Err(VimError::new(
                1293,
                "Cannot use a negative id for a text property",
            ));
        }
        if text.is_none() && col < 1 {
            return Err(VimError::new(964, format!("Invalid column number: {col}")));
        }
        if text.is_some() && col < 0 {
            return Err(VimError::new(964, format!("Invalid column number: {col}")));
        }
        let start_col = (col - 1).max(0) as u32;
        let start_col = if text.is_some() && col == 0 {
            0
        } else {
            start_col.min(line_len)
        };
        let (end_lnum, end_col) = if text.is_some() {
            (lnum as u32, start_col)
        } else if let Some(length) = props.get_str("length").and_then(value_number) {
            if length < 0 {
                return Err(VimError::new(964, "Invalid column number: 0"));
            }
            (lnum as u32, (start_col as i64 + length) as u32)
        } else if let Some(end_lnum) = props.get_str("end_lnum").and_then(value_number) {
            let end_col = props
                .get_str("end_col")
                .and_then(value_number)
                .unwrap_or(1)
                .max(1) as u32
                - 1;
            (end_lnum.max(lnum) as u32, end_col)
        } else if let Some(end_col) = props.get_str("end_col").and_then(value_number) {
            (lnum as u32, (end_col.max(1) as u32) - 1)
        } else {
            (lnum as u32, start_col)
        };
        if end_lnum < lnum as u32 || end_lnum > entry.text.line_count() {
            return Err(VimError::new(
                966,
                format!("Invalid line number: {end_lnum}"),
            ));
        }
        let end_len = entry.text.line_len(end_lnum);
        let end_col = end_col.min(end_len);
        let start_line = entry.text.lines.line(lnum as u32).map(|line| line.id);
        let end_line = entry.text.lines.line(end_lnum).map(|line| line.id);
        let (Some(start_line), Some(end_line)) = (start_line, end_line) else {
            return Err(VimError::new(966, "Invalid line number"));
        };
        let entry = self.buffers.get_mut(buffer).expect("buffer checked");
        if text.is_some() {
            id = entry.prop_next_id;
            entry.prop_next_id -= 1;
        }
        entry.text_props.push(PlacedProp {
            id,
            type_name,
            type_bufnr: type_def.bufnr,
            start_line,
            start_lnum: lnum as u32,
            start_col,
            end_line,
            end_lnum,
            end_col,
            text: text.unwrap_or_default(),
            text_align: props
                .get_str("text_align")
                .and_then(value_string)
                .unwrap_or_default(),
            text_padding_left: props
                .get_str("text_padding_left")
                .and_then(value_number)
                .unwrap_or(0),
            text_wrap: props
                .get_str("text_wrap")
                .and_then(value_string)
                .unwrap_or_default(),
        });
        entry.props_tick = 0;
        Ok(id)
    }

    fn prop_remove_impl(
        &mut self,
        filter: &VimDict,
        line_range: Option<(u32, Option<u32>)>,
    ) -> Result<i64, VimError> {
        let bufnr = filter.get_str("bufnr").and_then(value_number).unwrap_or(0);
        let buffer = if bufnr > 0 {
            self.prop_buffer_id(bufnr)
                .ok_or_else(|| VimError::new(158, "Invalid buffer"))?
        } else {
            self.current_buffer
        };
        self.sync_text_props_for(buffer);
        let id = filter.get_str("id").and_then(value_number);
        let single_type = filter.get_str("type").and_then(value_string);
        let types = filter
            .get_str("types")
            .and_then(|value| match value {
                Value::List(list) => Some(
                    list.borrow()
                        .items
                        .iter()
                        .filter_map(value_string)
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        if single_type.is_some() && !types.is_empty() {
            return Err(VimError::new(
                1295,
                "Cannot specify both 'type' and 'types'",
            ));
        }
        if id.is_none() && single_type.is_none() && types.is_empty() {
            return Err(VimError::new(968, "Need at least one of 'id' or 'type'"));
        }
        let both = filter.get_str("both").and_then(value_number).unwrap_or(0) != 0;
        let all = filter.get_str("all").and_then(value_number).unwrap_or(0) != 0;
        let matches = |prop: &PlacedProp| -> bool {
            let id_match = id.is_none_or(|wanted| prop.id == wanted);
            let type_match = if let Some(single) = &single_type {
                &prop.type_name == single
            } else if !types.is_empty() {
                types.contains(&prop.type_name)
            } else {
                false
            };
            let line_match = line_range.is_none_or(|(start, end)| {
                let end = end.unwrap_or(start);
                prop.start_lnum <= end && prop.end_lnum >= start
            });
            let type_present = single_type.is_some() || !types.is_empty();
            let found = if both {
                id_match && type_match
            } else if id.is_some() && type_present {
                id_match || type_match
            } else if id.is_some() {
                id_match
            } else {
                type_match
            };
            found && line_match
        };
        let Some(entry) = self.buffers.get_mut(buffer) else {
            return Ok(0);
        };
        let mut removed = 0i64;
        let mut index = 0;
        while index < entry.text_props.len() {
            if matches(&entry.text_props[index]) {
                entry.text_props.remove(index);
                removed += 1;
                if !all {
                    break;
                }
            } else {
                index += 1;
            }
        }
        entry.props_tick = 0;
        Ok(removed)
    }

    fn prop_clear_impl(
        &mut self,
        start: u32,
        end: Option<u32>,
        bufnr: i64,
    ) -> Result<(), VimError> {
        let buffer = if bufnr > 0 {
            self.prop_buffer_id(bufnr)
                .ok_or_else(|| VimError::new(158, "Invalid buffer"))?
        } else {
            self.current_buffer
        };
        self.sync_text_props_for(buffer);
        let end = end.unwrap_or(start);
        if let Some(entry) = self.buffers.get_mut(buffer) {
            entry
                .text_props
                .retain(|prop| !(prop.start_lnum <= end && prop.end_lnum >= start));
            entry.props_tick = 0;
        }
        Ok(())
    }

    fn prop_list_impl(&mut self, lnum: u32, filter: Option<&VimDict>) -> Value {
        let bufnr = filter
            .and_then(|dict| dict.get_str("bufnr"))
            .and_then(value_number)
            .unwrap_or(0);
        let Some(buffer) = (if bufnr > 0 {
            self.prop_buffer_id(bufnr)
        } else {
            Some(self.current_buffer)
        }) else {
            return Value::list(Vec::new());
        };
        self.sync_text_props_for(buffer);
        let Some(entry) = self.buffers.get(buffer) else {
            return Value::list(Vec::new());
        };
        let line_len = entry.text.line_len(lnum);
        let mut end_lnum = lnum;
        if let Some(end) = filter
            .and_then(|dict| dict.get_str("end_lnum"))
            .and_then(value_number)
        {
            let total = entry.text.line_count() as i64;
            end_lnum = if end < 0 {
                (total + end + 1).max(lnum as i64) as u32
            } else {
                end.max(lnum as i64) as u32
            };
        }
        let type_filter = filter
            .and_then(|dict| dict.get_str("types"))
            .and_then(|value| match value {
                Value::List(list) => Some(
                    list.borrow()
                        .items
                        .iter()
                        .filter_map(value_string)
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let id_filter = filter
            .and_then(|dict| dict.get_str("ids"))
            .and_then(|value| match value {
                Value::List(list) => Some(
                    list.borrow()
                        .items
                        .iter()
                        .filter_map(value_number)
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let mut items = Vec::new();
        for prop in &entry.text_props {
            if prop.start_lnum > end_lnum || prop.end_lnum < lnum {
                continue;
            }
            if !type_filter.is_empty() && !type_filter.contains(&prop.type_name) {
                continue;
            }
            if !id_filter.is_empty() && !id_filter.contains(&prop.id) {
                continue;
            }
            items.push(self.prop_dict(buffer, prop, Some(lnum), line_len));
        }
        items.sort_by_key(|value| {
            let dict = match value {
                Value::Dict(dict) => dict.clone(),
                _ => return (0, 0, 0),
            };
            let dict = dict.borrow();
            let col = dict.get_str("col").and_then(value_number).unwrap_or(0);
            let priority = dict.get_str("priority").and_then(value_number).unwrap_or(0);
            let id = dict.get_str("id").and_then(value_number).unwrap_or(0);
            (col, -priority, id)
        });
        Value::list(items)
    }

    fn prop_dict(
        &self,
        buffer: BufferId,
        prop: &PlacedProp,
        requested_lnum: Option<u32>,
        line_len: u32,
    ) -> Value {
        let mut dict = VimDict::new();
        dict.set(VimString::from("id"), Value::Number(prop.id));
        if let Some(requested) = requested_lnum {
            if prop.start_lnum != prop.end_lnum {
                dict.set(
                    VimString::from("lnum"),
                    Value::Number(i64::from(prop.start_lnum)),
                );
            }
            let col = if prop.start_lnum < requested {
                1
            } else {
                prop.start_col + 1
            };
            dict.set(VimString::from("col"), Value::Number(i64::from(col)));
            let length = if prop.start_lnum == prop.end_lnum {
                prop.end_col.saturating_sub(prop.start_col) as i64
            } else if prop.end_lnum > requested {
                (line_len - prop.start_col) as i64 + 1
            } else {
                (prop.end_col.saturating_sub(prop.start_col)) as i64 + 1
            };
            dict.set(VimString::from("length"), Value::Number(length));
            dict.set(
                VimString::from("start"),
                Value::Number(i64::from(prop.start_lnum == requested)),
            );
            dict.set(
                VimString::from("end"),
                Value::Number(i64::from(prop.end_lnum == requested)),
            );
        }
        if self.prop_type_for(buffer, &prop.type_name).is_some() {
            dict.set(
                VimString::from("type"),
                Value::string(prop.type_name.as_str()),
            );
            dict.set(
                VimString::from("type_bufnr"),
                Value::Number(prop.type_bufnr),
            );
        }
        if !prop.text.is_empty() {
            dict.set(VimString::from("text"), Value::string(prop.text.as_str()));
            if !prop.text_align.is_empty() {
                dict.set(
                    VimString::from("text_align"),
                    Value::string(prop.text_align.as_str()),
                );
            }
            if prop.text_padding_left != 0 {
                dict.set(
                    VimString::from("text_padding_left"),
                    Value::Number(prop.text_padding_left),
                );
            }
            if !prop.text_wrap.is_empty() {
                dict.set(
                    VimString::from("text_wrap"),
                    Value::string(prop.text_wrap.as_str()),
                );
            }
        }
        Value::dict(dict)
    }

    fn prop_find_impl(&mut self, filter: &VimDict, direction: &str) -> Value {
        let bufnr = filter.get_str("bufnr").and_then(value_number).unwrap_or(0);
        let Some(buffer) = (if bufnr > 0 {
            self.prop_buffer_id(bufnr)
        } else {
            Some(self.current_buffer)
        }) else {
            return Value::dict(VimDict::new());
        };
        self.sync_text_props_for(buffer);
        let id = filter.get_str("id").and_then(value_number);
        let type_name = filter.get_str("type").and_then(value_string);
        let both = filter.get_str("both").and_then(value_number).unwrap_or(0) != 0;
        if id.is_none() && type_name.is_none() {
            return Value::dict(VimDict::new());
        }
        let cursor = self.cursor();
        let start_lnum = filter
            .get_str("lnum")
            .and_then(value_number)
            .map(|lnum| lnum.max(1) as u32)
            .unwrap_or(cursor.lnum);
        let start_col = filter
            .get_str("col")
            .and_then(value_number)
            .map(|col| col.max(1) as u32 - 1)
            .unwrap_or(if filter.get_str("lnum").is_some() {
                0
            } else {
                cursor.col
            });
        let skipstart = filter
            .get_str("skipstart")
            .and_then(value_number)
            .unwrap_or(0)
            != 0;
        let backward = direction.starts_with('b');
        let Some(entry) = self.buffers.get(buffer) else {
            return Value::dict(VimDict::new());
        };
        let mut candidates: Vec<(&PlacedProp, u32, u32)> = entry
            .text_props
            .iter()
            .filter(|prop| {
                let id_match = id.is_none_or(|wanted| prop.id == wanted);
                let type_match = type_name
                    .as_deref()
                    .is_none_or(|wanted| prop.type_name == wanted);
                if both {
                    id_match && type_match
                } else if id.is_some() {
                    id_match
                } else {
                    type_match
                }
            })
            .map(|prop| (prop, prop.start_lnum, prop.start_col))
            .collect();
        candidates.sort_by_key(|(_, lnum, col)| (*lnum, *col));
        let found = if backward {
            candidates.into_iter().rev().find(|(_, lnum, col)| {
                (*lnum, *col) < (start_lnum, start_col)
                    || (!skipstart && (*lnum, *col) == (start_lnum, start_col))
            })
        } else {
            candidates.into_iter().find(|(_, lnum, col)| {
                (*lnum, *col) > (start_lnum, start_col)
                    || (!skipstart && (*lnum, *col) == (start_lnum, start_col))
            })
        };
        let Some((prop, lnum, _)) = found else {
            return Value::dict(VimDict::new());
        };
        let line_len = entry.text.line_len(lnum);
        let value = self.prop_dict(buffer, prop, Some(lnum), line_len);
        if let Value::Dict(dict) = &value {
            dict.borrow_mut()
                .set(VimString::from("lnum"), Value::Number(i64::from(lnum)));
        }
        value
    }

    /// Entry point for the prop_* VimL functions.
    pub(crate) fn prop_call_viml(&mut self, name: &str, args: &[Value]) -> Result<Value, VimError> {
        match name {
            "prop_type_add" | "prop_type_change" => {
                let type_name = args.first().and_then(value_string).unwrap_or_default();
                let Some(Value::Dict(dict)) = args.get(1) else {
                    return Err(VimError::new(474, "Invalid argument"));
                };
                let dict = dict.borrow();
                if name == "prop_type_add" {
                    self.prop_type_add_impl(&type_name, &dict)?;
                } else {
                    self.prop_type_change_impl(&type_name, &dict)?;
                }
                Ok(Value::null())
            }
            "prop_type_delete" => {
                let type_name = args.first().and_then(value_string).unwrap_or_default();
                let dict = match args.get(1) {
                    Some(Value::Dict(dict)) => Some(dict.borrow()),
                    _ => None,
                };
                self.prop_type_delete_impl(&type_name, dict.as_deref())?;
                Ok(Value::null())
            }
            "prop_type_get" => {
                let type_name = args.first().and_then(value_string).unwrap_or_default();
                let dict = match args.get(1) {
                    Some(Value::Dict(dict)) => Some(dict.borrow()),
                    _ => None,
                };
                Ok(self.prop_type_get_impl(&type_name, dict.as_deref()))
            }
            "prop_type_list" => {
                let dict = match args.first() {
                    Some(Value::Dict(dict)) => Some(dict.borrow()),
                    _ => None,
                };
                Ok(self.prop_type_list_impl(dict.as_deref()))
            }
            "prop_add" => {
                let lnum = args.first().and_then(value_number).unwrap_or(0);
                let col = args.get(1).and_then(value_number).unwrap_or(0);
                let Some(Value::Dict(props)) = args.get(2) else {
                    return Err(VimError::new(474, "Invalid argument"));
                };
                let props = props.borrow();
                let bufnr = props.get_str("bufnr").and_then(value_number).unwrap_or(0);
                Ok(Value::Number(
                    self.prop_add_dictionary(bufnr, lnum, col, &props)?,
                ))
            }
            "prop_add_list" => {
                let Some(Value::Dict(props)) = args.first() else {
                    return Err(VimError::new(474, "Invalid argument"));
                };
                let props = props.borrow();
                let bufnr = props.get_str("bufnr").and_then(value_number).unwrap_or(0);
                let Some(Value::List(items)) = args.get(1) else {
                    return Ok(Value::null());
                };
                let items = items.borrow();
                for item in &items.items {
                    let Value::List(item) = item else {
                        continue;
                    };
                    let item = item.borrow();
                    let values: Vec<i64> = item.items.iter().filter_map(value_number).collect();
                    if values.len() < 4 {
                        continue;
                    }
                    let (start_lnum, start_col, end_lnum, end_col) =
                        (values[0], values[1], values[2], values[3]);
                    let mut item_props = VimDict::new();
                    if let Some(type_name) = props.get_str("type") {
                        item_props.set(VimString::from("type"), type_name.clone());
                    }
                    let mut id = props.get_str("id").cloned();
                    if values.len() > 4 {
                        id = Some(Value::Number(values[4]));
                    }
                    if let Some(id) = id {
                        item_props.set(VimString::from("id"), id);
                    }
                    item_props.set(VimString::from("end_lnum"), Value::Number(end_lnum));
                    item_props.set(VimString::from("end_col"), Value::Number(end_col));
                    let _ = self.prop_add_dictionary(bufnr, start_lnum, start_col, &item_props);
                }
                Ok(Value::null())
            }
            "prop_clear" => {
                let start = args.first().and_then(value_number).unwrap_or(0);
                let (end, dict_index) = match args.get(1) {
                    Some(Value::Dict(dict)) => (None, Some(dict)),
                    Some(value) => (value_number(value).map(|end| end as u32), None),
                    None => (None, None),
                };
                let bufnr = dict_index
                    .and_then(|dict| dict.borrow().get_str("bufnr").and_then(value_number))
                    .unwrap_or(0);
                self.prop_clear_impl(start.max(1) as u32, end, bufnr)?;
                Ok(Value::null())
            }
            "prop_remove" => {
                let Some(Value::Dict(filter)) = args.first() else {
                    return Err(VimError::new(474, "Invalid argument"));
                };
                let filter = filter.borrow();
                let start = args
                    .get(1)
                    .and_then(value_number)
                    .map(|lnum| lnum.max(1) as u32);
                let end = args
                    .get(2)
                    .and_then(value_number)
                    .map(|lnum| lnum.max(1) as u32);
                let range = start.map(|start| (start, end));
                Ok(Value::Number(self.prop_remove_impl(&filter, range)?))
            }
            "prop_list" => {
                let lnum = args.first().and_then(value_number).unwrap_or(0);
                if lnum < 1 {
                    return Ok(Value::list(Vec::new()));
                }
                let dict = match args.get(1) {
                    Some(Value::Dict(dict)) => Some(dict.borrow()),
                    _ => None,
                };
                Ok(self.prop_list_impl(lnum as u32, dict.as_deref()))
            }
            "prop_find" => {
                let Some(Value::Dict(filter)) = args.first() else {
                    return Err(VimError::new(474, "Invalid argument"));
                };
                let filter = filter.borrow();
                let direction = args
                    .get(1)
                    .and_then(value_string)
                    .unwrap_or_else(|| "f".to_string());
                Ok(self.prop_find_impl(&filter, &direction))
            }
            _ => Err(VimError::new(117, format!("Unknown function: {name}"))),
        }
    }
}

impl PlacedProp {
    fn text_align_is_above_or_below(&self) -> bool {
        self.text_align == "above" || self.text_align == "below"
    }
}

/// Byte range [start, end) covered by \p prop on line \p lnum.
fn prop_line_range(prop: &PlacedProp, lnum: u32, line_len: u32) -> Option<(u32, u32)> {
    if lnum < prop.start_lnum || lnum > prop.end_lnum {
        return None;
    }
    let start = if lnum == prop.start_lnum {
        prop.start_col.min(line_len)
    } else {
        0
    };
    let end = if lnum == prop.end_lnum {
        prop.end_col.min(line_len)
    } else {
        line_len
    };
    Some((start, end.max(start)))
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
