//! Folding: fold level computation, fold commands and fold text.
//!
//! Fold state is window-local and lives in rvim_view::Window::folds; this
//! module computes it from the buffer and the window options and implements the
//! fold operations (:fold, zo, zc, zR, ...) plus the VimL fold functions. The
//! display side only reads the computed state.

use std::collections::{BTreeMap, BTreeSet};

use rvim_types::{BufPos, BufferId, VimError, VimString, WindowId};
use rvim_view::FoldState;

use crate::editor::Editor;

/// Window-local options that drive folding.
#[derive(Clone, Debug, Default)]
pub(crate) struct FoldOptions {
    pub method: String,
    pub level: i64,
    pub enable: bool,
    pub min_lines: i64,
    pub nest_max: i64,
    pub ignore: String,
    pub marker: String,
    pub expr: String,
    pub text: String,
}

impl FoldOptions {
    /// Fingerprint used to decide whether a window's folds are still valid.
    fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.method,
            self.level,
            i64::from(self.enable),
            self.min_lines,
            self.nest_max,
            self.marker,
            self.ignore,
            self.expr,
            self.text
        )
    }

    fn method_kind(&self) -> FoldMethod {
        match self.method.as_str() {
            "indent" => FoldMethod::Indent,
            "marker" => FoldMethod::Marker,
            "expr" => FoldMethod::Expr,
            "syntax" => FoldMethod::Syntax,
            "diff" => FoldMethod::Diff,
            _ => FoldMethod::Manual,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FoldMethod {
    Manual,
    Indent,
    Marker,
    Expr,
    Syntax,
    Diff,
}

impl Editor {
    /// Options that affect folding in window \p window.
    pub(crate) fn fold_options(&self, window: WindowId) -> FoldOptions {
        let raw = rvim_types::Id::raw(window);
        FoldOptions {
            method: self.options.window_string(raw, "foldmethod"),
            level: self.options.window_number(raw, "foldlevel"),
            enable: self.options.window_bool(raw, "foldenable"),
            min_lines: self.options.window_number(raw, "foldminlines").max(0),
            nest_max: self.options.window_number(raw, "foldnestmax").max(0),
            ignore: self.options.window_string(raw, "foldignore"),
            marker: self.options.window_string(raw, "foldmarker"),
            expr: self.options.window_string(raw, "foldexpr"),
            text: self.options.window_string(raw, "foldtext"),
        }
    }

    /// Recompute the fold state of every window whose buffer or options
    /// changed. Cheap when nothing changed (one integer comparison per window).
    pub(crate) fn sync_folds(&mut self) {
        if self.folds_syncing {
            return;
        }
        self.folds_syncing = true;
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
            let options = self.fold_options(id);
            let state = &window.folds;
            let key = options.key();
            let fresh = !state.dirty
                && state.buffer == rvim_types::Id::raw(buffer)
                && state.changedtick == tick
                && state.options_key == key;
            if fresh {
                continue;
            }
            let manual = state.manual.clone();
            let opened = state.opened.clone();
            let forced = state.forced_closed.clone();
            let new_state = self.compute_folds(buffer, id, &options, manual, opened, forced, tick);
            if let Some(window) = self.view.window_mut(id) {
                window.folds = new_state;
            }
        }
        self.folds_syncing = false;
        // Keep the cursor and topline on displayed lines.
        let ids: Vec<WindowId> = self.view.windows.keys().collect();
        for id in ids {
            let Some(window) = self.view.window_mut(id) else {
                continue;
            };
            let cursor_lnum = window.cursor.lnum;
            if window.folds.is_hidden(cursor_lnum) {
                let owner = window.folds.owner(cursor_lnum).max(1);
                window.cursor.lnum = owner;
                window.cursor.col = 0;
            }
            let top = window.viewport.topline;
            if window.folds.is_hidden(top) && window.folds.owner(top) != top {
                window.viewport.topline = window.folds.owner(top).max(1);
            }
            if window.viewport.topline > window.cursor.lnum {
                window.viewport.topline = window.cursor.lnum;
            }
        }
    }

    /// Force a recompute of the current window's folds (after a fold command).
    pub(crate) fn refresh_folds(&mut self) {
        if let Some(window) = self.view.current_window_mut() {
            window.folds.dirty = true;
        }
        self.sync_folds();
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_folds(
        &mut self,
        buffer: BufferId,
        _window: WindowId,
        options: &FoldOptions,
        manual: Vec<(u32, u32)>,
        opened: BTreeSet<u32>,
        forced: BTreeSet<u32>,
        tick: u64,
    ) -> FoldState {
        let lines: Vec<VimString> = self
            .buffers
            .get(buffer)
            .map(|entry| entry.text.lines_vec())
            .unwrap_or_default();
        let count = lines.len() as u32;
        let mut state = FoldState {
            buffer: rvim_types::Id::raw(buffer),
            changedtick: tick,
            method: options.method.clone(),
            foldlevel: options.level,
            foldenable: options.enable,
            manual: manual.clone(),
            options_key: options.key(),
            ..FoldState::default()
        };
        let raw_levels = self.fold_levels(buffer, options, &lines, &manual);
        let levels = fill_unknown_levels(raw_levels);
        state.levels = levels
            .iter()
            .map(|level| (*level).clamp(0, i64::from(u16::MAX)) as u16)
            .collect();
        let buffer_raw = rvim_types::Id::raw(buffer);

        // Every fold: a fold of level L starts where the level rises to L and
        // ends at the last line whose level is still at least L.
        let mut folds: Vec<(u32, u32, u16)> = Vec::new();
        for index in 0..levels.len() {
            let current = levels[index];
            let previous = if index == 0 { 0 } else { levels[index - 1] };
            if current <= previous {
                continue;
            }
            for level in (previous + 1)..=current {
                let mut end = index;
                let mut scan = index + 1;
                while scan < levels.len() && levels[scan] >= level {
                    end = scan;
                    scan += 1;
                }
                folds.push(((index + 1) as u32, (end + 1) as u32, level as u16));
            }
        }
        state.folds = folds.clone();
        for (start, end, _) in &folds {
            state
                .starts
                .entry(*start)
                .and_modify(|value| *value = (*value).max(*end))
                .or_insert(*end);
        }

        // Enclosing fold start and the lowest level starting on each line, used
        // by the fold column renderer.
        let mut enclosing = vec![0u32; levels.len()];
        let mut low_level = vec![0u16; levels.len()];
        let mut active: Vec<(u32, u32, u16)> = Vec::new();
        let mut starting: BTreeMap<u32, Vec<(u32, u32, u16)>> = BTreeMap::new();
        for fold in &folds {
            starting.entry(fold.0).or_default().push(*fold);
        }
        for lnum in 1..=count {
            if let Some(list) = starting.get(&lnum) {
                let mut list = list.clone();
                list.sort_by_key(|(_, _, level)| *level);
                low_level[(lnum - 1) as usize] =
                    list.iter().map(|(_, _, level)| *level).min().unwrap_or(0);
                for fold in list {
                    active.push(fold);
                }
            }
            enclosing[(lnum - 1) as usize] = active.last().map_or(0, |fold| fold.0);
            while active.last().is_some_and(|fold| fold.1 <= lnum) {
                active.pop();
            }
        }
        state.enclosing_start = enclosing;
        state.low_level = low_level;

        // Closed folds: derived from 'foldlevel' and 'foldminlines', adjusted by
        // explicit zo/zc overrides.
        let mut opened = opened;
        let mut forced = forced;
        opened.retain(|start| state.starts.contains_key(start));
        forced.retain(|start| state.starts.contains_key(start));
        if options.enable {
            for (start, end, level) in &folds {
                let length = i64::from(*end) - i64::from(*start) + 1;
                let mut closed = i64::from(*level) > options.level && length > options.min_lines;
                if opened.contains(start) {
                    closed = false;
                }
                if forced.contains(start) {
                    closed = true;
                }
                if closed {
                    state
                        .closed
                        .entry(*start)
                        .and_modify(|value| *value = (*value).max(*end))
                        .or_insert(*end);
                }
            }
        }
        state.opened = opened;
        state.forced_closed = forced;

        // Outermost closed fold covering each line.
        let mut hidden_owner = vec![0u32; levels.len()];
        let mut hidden_end = vec![0u32; levels.len()];
        if options.enable {
            for (start, end) in state.closed.clone() {
                for lnum in start..=end {
                    let index = (lnum - 1) as usize;
                    if index >= hidden_owner.len() || hidden_owner[index] != 0 {
                        continue;
                    }
                    hidden_owner[index] = start;
                    hidden_end[index] = end;
                }
            }
        }
        state.hidden_owner = hidden_owner;
        state.hidden_end = hidden_end;

        // Fold text of every displayed closed fold.
        let commentstring = self.options.buffer_string(buffer_raw, "commentstring");
        for (start, end) in state.closed.clone() {
            if state.owner(start) != start {
                continue;
            }
            let text = self.fold_text_for(
                options,
                &lines,
                start,
                end,
                state.level(start),
                &commentstring,
            );
            state.text.insert(start, text);
        }
        state
    }

    fn fold_levels(
        &mut self,
        buffer: BufferId,
        options: &FoldOptions,
        lines: &[VimString],
        manual: &[(u32, u32)],
    ) -> Vec<i64> {
        let count = lines.len() as u32;
        let mut levels = match options.method_kind() {
            FoldMethod::Manual => manual_levels(manual, count),
            FoldMethod::Indent => self.indent_levels(buffer, options, lines),
            FoldMethod::Marker => marker_levels(options, lines),
            FoldMethod::Expr => self.expr_levels(options, lines),
            FoldMethod::Syntax | FoldMethod::Diff => vec![0; lines.len()],
        };
        if levels.len() < lines.len() {
            levels.resize(lines.len(), 0);
        }
        levels
    }

    fn indent_levels(
        &mut self,
        buffer: BufferId,
        options: &FoldOptions,
        lines: &[VimString],
    ) -> Vec<i64> {
        let buffer_raw = rvim_types::Id::raw(buffer);
        let shiftwidth = self.options.buffer_number(buffer_raw, "shiftwidth").max(1);
        let tabstop = self.options.buffer_number(buffer_raw, "tabstop").max(1) as usize;
        let last = lines.len();
        let mut levels = Vec::with_capacity(last);
        for (index, line) in lines.iter().enumerate() {
            let lnum = index + 1;
            let bytes = line.as_bytes();
            let first = bytes
                .iter()
                .position(|byte| *byte != b' ' && *byte != b'\t');
            let level = match first {
                None => {
                    if lnum == 1 || lnum == last {
                        0
                    } else {
                        -1
                    }
                }
                Some(first) => {
                    let ch = bytes[first];
                    if options.ignore.as_bytes().contains(&ch) {
                        if lnum == 1 || lnum == last {
                            0
                        } else {
                            -1
                        }
                    } else {
                        let indent =
                            rvim_types::unicode::display_width(&bytes[..first], 0, tabstop);
                        (indent / shiftwidth as usize) as i64
                    }
                }
            };
            let clamped = if level > options.nest_max {
                options.nest_max.max(0)
            } else {
                level
            };
            levels.push(clamped);
        }
        levels
    }

    fn expr_levels(&mut self, options: &FoldOptions, lines: &[VimString]) -> Vec<i64> {
        let last = lines.len() as i64;
        let expression = options.expr.trim();
        if expression.is_empty() {
            return vec![0; lines.len()];
        }
        let mut levels: Vec<i64> = Vec::with_capacity(lines.len());
        let mut lvl: i64 = 0;
        let mut lvl_next: i64 = 0;
        let previous_lnum = self.viml.get_vvar("lnum");
        for lnum in 1..=last {
            self.viml.set_vvar("lnum", rvim_viml::Value::Number(lnum));
            let result = self.eval_viml(expression);
            let (n, kind) = match result {
                Ok(rvim_viml::Value::Number(n)) => (n, '\0'),
                Ok(rvim_viml::Value::String(text)) => {
                    let text = text.to_string_lossy();
                    let mut chars = text.chars();
                    match chars.next() {
                        Some(first) if !first.is_ascii_digit() && first != '-' => {
                            (chars.as_str().trim().parse::<i64>().unwrap_or(0), first)
                        }
                        Some(_) | None => (text.trim().parse::<i64>().unwrap_or(0), '\0'),
                    }
                }
                Ok(_) => (0, '\0'),
                Err(_) => (0, '\0'),
            };
            if lnum <= 1 {
                lvl = 0;
            }
            match kind {
                'a' => {
                    if lvl >= 0 {
                        lvl += n;
                        lvl_next = lvl;
                    }
                }
                's' => {
                    if lvl >= 0 {
                        lvl_next = if n > lvl { 0 } else { lvl - n };
                    }
                }
                '>' => {
                    lvl = n;
                    lvl_next = n;
                }
                '<' => {
                    lvl_next = lvl.min(n - 1);
                }
                '=' => {
                    lvl_next = lvl;
                }
                _ => {
                    if n < 0 {
                        lvl_next = lvl;
                    } else {
                        lvl_next = n;
                        lvl = n;
                    }
                }
            }
            if lvl < 0 {
                if lnum <= 1 {
                    lvl = 0;
                    lvl_next = 0;
                }
                if lnum == last {
                    lvl_next = 0;
                }
            }
            levels.push(lvl);
            lvl = lvl_next;
        }
        match previous_lnum {
            Some(value) => self.viml.set_vvar("lnum", value),
            None => self.viml.set_vvar("lnum", rvim_viml::Value::Number(0)),
        }
        levels
    }

    #[allow(clippy::too_many_arguments)]
    fn fold_text_for(
        &mut self,
        options: &FoldOptions,
        lines: &[VimString],
        start: u32,
        end: u32,
        level: u16,
        commentstring: &str,
    ) -> String {
        let text_option = options.text.trim();
        if text_option.is_empty() {
            return default_folded_text(start, end);
        }
        if text_option == "foldtext()" {
            return default_foldtext(lines, start, end, level, &options.marker, commentstring);
        }
        // A custom expression: expose the v:fold* variables like Vim does.
        let variables = [
            ("foldstart", rvim_viml::Value::Number(i64::from(start))),
            ("foldend", rvim_viml::Value::Number(i64::from(end))),
            (
                "folddashes",
                rvim_viml::Value::string("-".repeat(usize::from(level))),
            ),
            ("foldlevel", rvim_viml::Value::Number(i64::from(level))),
        ];
        let mut saved = Vec::new();
        for (name, value) in &variables {
            saved.push((name.to_string(), self.viml.get_vvar(name)));
            self.viml.set_vvar(name, value.clone());
        }
        let result = self.eval_viml(text_option);
        for (name, previous) in saved {
            match previous {
                Some(value) => self.viml.set_vvar(&name, value),
                None => self.viml.set_vvar(&name, rvim_viml::Value::Number(0)),
            }
        }
        match result {
            Ok(value) => value.echo_string(),
            Err(_) => default_foldtext(lines, start, end, level, &options.marker, commentstring),
        }
    }

    /// Closed fold containing \p lnum in window \p window, as (start, end).
    pub(crate) fn folded_at(&mut self, window: WindowId, lnum: u32) -> Option<(u32, u32)> {
        self.sync_folds();
        let state = &self.view.windows.get(&window)?.folds;
        state.closed_containing(lnum)
    }

    /// Fold level of \p lnum in window \p window.
    pub(crate) fn fold_level_at(&mut self, window: WindowId, lnum: u32) -> u16 {
        self.sync_folds();
        self.view
            .windows
            .get(&window)
            .map_or(0, |window| window.folds.level(lnum))
    }

    /// Text used by foldtextresult() for line \p lnum.
    pub(crate) fn fold_text_result(&mut self, window: WindowId, lnum: u32) -> String {
        let Some((_start, end)) = self.folded_at(window, lnum) else {
            return String::new();
        };
        let Some(entry) = self
            .view
            .windows
            .get(&window)
            .map(|window| window.buffer)
            .and_then(|buffer| self.buffers.get(buffer))
        else {
            return String::new();
        };
        let lines = entry.text.lines_vec();
        let options = self.fold_options(window);
        let level = self.view.windows.get(&window).map_or(0, |window| {
            window.folds.level(window.folds.owner(lnum)).max(1)
        });
        let buffer = self
            .view
            .windows
            .get(&window)
            .map(|window| window.buffer)
            .unwrap_or(self.current_buffer);
        let commentstring = self
            .options
            .buffer_string(rvim_types::Id::raw(buffer), "commentstring");
        if options.text.trim().is_empty() {
            return default_folded_text(lnum, end);
        }
        if options.text.trim() == "foldtext()" {
            return default_foldtext(&lines, lnum, end, level, &options.marker, &commentstring);
        }
        self.fold_text_for(&options, &lines, lnum, end, level, &commentstring)
    }

    /// Implementation of foldtext(): uses the v:fold* variables.
    pub(crate) fn current_foldtext_var(&mut self) -> String {
        let start = match self.viml.get_vvar("foldstart") {
            Some(rvim_viml::Value::Number(number)) => number,
            _ => 0,
        };
        let end = match self.viml.get_vvar("foldend") {
            Some(rvim_viml::Value::Number(number)) => number,
            _ => 0,
        };
        let dashes = match self.viml.get_vvar("folddashes") {
            Some(rvim_viml::Value::String(text)) => text.to_string_lossy().into_owned(),
            _ => String::new(),
        };
        if start <= 0 || end < start {
            return String::new();
        }
        let level = dashes.chars().count() as u16;
        let Some(buffer) = self.view.current_window().map(|window| window.buffer) else {
            return String::new();
        };
        let Some(entry) = self.buffers.get(buffer) else {
            return String::new();
        };
        let lines = entry.text.lines_vec();
        if end as usize > lines.len() {
            return String::new();
        }
        let options = self.fold_options(
            self.view
                .current_window()
                .map_or(WindowId::from_raw(0), |window| window.id),
        );
        let commentstring = self
            .options
            .buffer_string(rvim_types::Id::raw(buffer), "commentstring");
        default_foldtext(
            &lines,
            start as u32,
            end as u32,
            level,
            &options.marker,
            &commentstring,
        )
    }

    /// Normalize a cursor position that is inside a closed fold to the fold
    /// start line (Vim never leaves the cursor on a hidden line).
    pub(crate) fn fold_normalized(&mut self, pos: BufPos) -> BufPos {
        self.sync_folds();
        let Some(window) = self.view.current_window() else {
            return pos;
        };
        if window.folds.is_hidden(pos.lnum) {
            let owner = window.folds.owner(pos.lnum).max(1);
            let len = self
                .current_buffer()
                .map_or(0, |buffer| buffer.text.line_len(owner));
            return BufPos::new(owner, pos.col.min(len));
        }
        pos
    }

    /// Fold-aware vertical motion: stepping into a closed fold moves to its
    /// start moving up and past its end moving down.
    pub(crate) fn fold_adjust_vertical(&mut self, current: BufPos, target: BufPos) -> BufPos {
        self.sync_folds();
        let Some(window) = self.view.current_window() else {
            return target;
        };
        if !window.folds.is_hidden(target.lnum) || window.folds.owner(target.lnum) == target.lnum {
            return target;
        }
        let end = window
            .folds
            .hidden_end
            .get((target.lnum - 1) as usize)
            .copied()
            .unwrap_or(target.lnum)
            .min(self.line_count().max(1));
        if target.lnum > current.lnum {
            BufPos::new((end + 1).min(self.line_count().max(1)), target.col)
        } else {
            BufPos::new(window.folds.owner(target.lnum).max(1), target.col)
        }
    }

    /// zo / :foldopen: open folds on/containing \p lnum.
    pub(crate) fn fold_open(&mut self, lnum: u32, recursive: bool) {
        self.sync_folds();
        let Some(window) = self.view.current_window() else {
            return;
        };
        let mut closed: Vec<(u32, u32)> = window
            .folds
            .closed
            .iter()
            .filter(|(start, end)| **start <= lnum && lnum <= **end)
            .map(|(start, end)| (*start, *end))
            .collect();
        if closed.is_empty() {
            return;
        }
        closed.sort_unstable();
        let mut state = window.folds.clone();
        if recursive {
            for (start, _end) in closed {
                state.closed.remove(&start);
                state.opened.insert(start);
                state.forced_closed.remove(&start);
            }
        } else {
            let (start, _end) = *closed.last().unwrap();
            state.closed.remove(&start);
            state.opened.insert(start);
            state.forced_closed.remove(&start);
        }
        self.apply_fold_state(state);
        self.refresh_folds();
    }

    /// zc / :foldclose: close the fold containing \p lnum.
    pub(crate) fn fold_close(&mut self, lnum: u32, recursive: bool) {
        self.sync_folds();
        let Some(window) = self.view.current_window() else {
            return;
        };
        let mut ranges: Vec<(u32, u32)> = window
            .folds
            .starts
            .iter()
            .filter(|(start, end)| **start <= lnum && lnum <= **end)
            .map(|(start, end)| (*start, *end))
            .collect();
        if ranges.is_empty() {
            return;
        }
        ranges.sort_unstable();
        let mut state = window.folds.clone();
        if recursive {
            for (start, end) in ranges {
                state.opened.remove(&start);
                state.forced_closed.insert(start);
                state.closed.insert(start, end);
            }
        } else {
            let (start, end) = ranges[0];
            state.opened.remove(&start);
            state.forced_closed.insert(start);
            state.closed.insert(start, end);
        }
        self.apply_fold_state(state);
        self.refresh_folds();
    }

    /// za: toggle the fold at/containing \p lnum.
    pub(crate) fn fold_toggle(&mut self, lnum: u32) {
        let window = self.window_id();
        if self.folded_at(window, lnum).is_some() {
            self.fold_open(lnum, false);
        } else {
            self.fold_close(lnum, false);
        }
    }

    /// Deepest fold nesting in the current window.
    pub(crate) fn fold_deepest(&mut self) -> i64 {
        self.sync_folds();
        self.view.current_window().map_or(0, |window| {
            window.folds.levels.iter().copied().max().unwrap_or(0) as i64
        })
    }

    /// zR: open all folds ('foldlevel' = deepest nesting of the buffer).
    pub(crate) fn fold_open_all(&mut self) {
        let deepest = self.fold_deepest();
        self.set_foldlevel(deepest);
    }

    /// zr: reduce folding by p count levels, clamped to the deepest nesting.
    pub(crate) fn fold_reduce(&mut self, count: i64) {
        let deepest = self.fold_deepest();
        let window = self.window_id();
        let current = self
            .options
            .window_number(rvim_types::Id::raw(window), "foldlevel");
        self.set_foldlevel((current + count).min(deepest));
    }

    /// zm: fold one more level, never below zero.
    pub(crate) fn fold_more(&mut self, count: i64) {
        let window = self.window_id();
        let current = self
            .options
            .window_number(rvim_types::Id::raw(window), "foldlevel");
        self.set_foldlevel((current - count).max(0));
    }

    /// zM: close all folds.
    pub(crate) fn fold_close_all(&mut self) {
        self.set_foldlevel(0);
    }

    /// Set 'foldlevel' for the current window and recompute closed folds.
    pub(crate) fn set_foldlevel(&mut self, level: i64) {
        let window = self.window_id();
        let raw = rvim_types::Id::raw(window);
        let _ = self.options.set_window(
            raw,
            "foldlevel",
            crate::options::OptionValue::Number(level.max(0)),
        );
        self.sync_window_options();
        if let Some(window) = self.view.window_mut(window) {
            window.folds.opened.clear();
            window.folds.forced_closed.clear();
            window.folds.foldlevel = i64::MIN;
            window.folds.dirty = true;
        }
        self.sync_folds();
    }

    /// zn / zN: set 'foldenable' explicitly.
    pub(crate) fn fold_set_enable(&mut self, enable: bool) {
        let window = self.window_id();
        let raw = rvim_types::Id::raw(window);
        let _ =
            self.options
                .set_window(raw, "foldenable", crate::options::OptionValue::Bool(enable));
        self.sync_window_options();
        self.refresh_folds();
    }

    /// zi: toggle 'foldenable'.
    pub(crate) fn fold_toggle_enable(&mut self) {
        let window = self.window_id();
        let raw = rvim_types::Id::raw(window);
        let enabled = self.options.window_bool(raw, "foldenable");
        let _ = self.options.set_window(
            raw,
            "foldenable",
            crate::options::OptionValue::Bool(!enabled),
        );
        self.sync_window_options();
        self.refresh_folds();
    }

    /// [z / ]z: move to the previous/next fold start.
    pub(crate) fn fold_jump(&mut self, forward: bool) {
        self.sync_folds();
        let Some(window) = self.view.current_window() else {
            return;
        };
        let current = self.cursor().lnum;
        let mut starts: Vec<u32> = window.folds.starts.keys().copied().collect();
        starts.sort_unstable();
        let target = if forward {
            starts.into_iter().find(|start| *start > current)
        } else {
            starts.into_iter().rev().find(|start| *start < current)
        };
        if let Some(lnum) = target {
            self.set_cursor(BufPos::new(lnum, 0));
            self.goto_motion(rvim_edit::Motion::FirstNonBlank);
        }
    }

    /// Create a fold spanning lines \p start..=\p end (zf, :fold).
    ///
    /// Manual folds are added to the window's manual fold list; with
    /// 'foldmethod' marker the fold markers are inserted into the text, as Vim
    /// does. Other fold methods refuse the operation with E350.
    pub(crate) fn create_fold(&mut self, start: u32, end: u32) -> Result<(), VimError> {
        if self.view.current_window().is_none() {
            return Ok(());
        }
        let start = start.max(1);
        let end = end.max(start).min(self.line_count().max(1));
        let window = self.window_id();
        match self.fold_options(window).method.as_str() {
            "manual" => {
                let mut state = self
                    .view
                    .window_mut(window)
                    .map(|window| window.folds.clone())
                    .unwrap_or_default();
                state.manual.retain(|(s, e)| *e < start || *s > end);
                state.manual.push((start, end));
                state.manual.sort_unstable();
                state.forced_closed.insert(start);
                state.opened.remove(&start);
                state.dirty = true;
                self.apply_fold_state(state);
                self.refresh_folds();
                Ok(())
            }
            "marker" => {
                self.insert_fold_markers(start, end)?;
                self.refresh_folds();
                Ok(())
            }
            _ => Err(VimError::new(
                350,
                "Cannot create fold with current 'foldmethod'",
            )),
        }
    }

    /// Append fold markers to the start and end line, optionally wrapped in
    /// 'commentstring'.
    fn insert_fold_markers(&mut self, start: u32, end: u32) -> Result<(), VimError> {
        let options = self.fold_options(self.window_id());
        let (start_marker, end_marker) = match options.marker.split_once(',') {
            Some((start, end)) => (start.to_string(), end.to_string()),
            None => ("{{{".to_string(), "}}}".to_string()),
        };
        let commentstring = self
            .current_buffer()
            .map(|buffer| {
                self.options
                    .buffer_string(rvim_types::Id::raw(buffer.id), "commentstring")
            })
            .unwrap_or_default();
        for (lnum, marker) in [(start, start_marker), (end, end_marker)] {
            let line = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
                .map(|line| line.to_vec())
                .unwrap_or_default();
            let mut text = String::from_utf8_lossy(&line).into_owned();
            if let Some((comment_start, comment_end)) = commentstring.split_once("%s") {
                let comment_start = comment_start.trim();
                let comment_end = comment_end.trim();
                if !comment_start.is_empty() {
                    text.push_str(comment_start);
                    text.push_str(&marker);
                    text.push_str(comment_end);
                } else {
                    text.push_str(&marker);
                }
            } else {
                text.push_str(&marker);
            }
            self.record_undo();
            if let Some(buffer) = self.current_buffer_mut() {
                buffer.text.replace_line(lnum, VimString::from(text));
            }
            self.commit_undo();
        }
        Ok(())
    }

    /// zd: delete the fold containing \p lnum (E351 for fold methods that
    /// cannot be edited).
    pub(crate) fn delete_fold(&mut self, lnum: u32, recursive: bool) -> Result<(), VimError> {
        self.sync_folds();
        let Some(window) = self.view.current_window() else {
            return Ok(());
        };
        let state = window.folds.clone();
        let target = state
            .starts
            .iter()
            .find(|(start, end)| **start <= lnum && lnum <= **end)
            .map(|(start, end)| (*start, *end))
            .or_else(|| {
                state
                    .manual
                    .iter()
                    .find(|(start, end)| *start <= lnum && lnum <= *end)
                    .copied()
            });
        let Some((start, end)) = target else {
            return Ok(());
        };
        match self.fold_options(self.window_id()).method.as_str() {
            "manual" => {
                let mut manual = state.manual.clone();
                manual.retain(|(s, e)| {
                    if recursive {
                        *e < start || *s > end
                    } else {
                        !(*s == start && *e == end)
                    }
                });
                let mut new_state = state;
                new_state.manual = manual;
                new_state.forced_closed.remove(&start);
                new_state.opened.remove(&start);
                new_state.dirty = true;
                self.apply_fold_state(new_state);
                self.refresh_folds();
                Ok(())
            }
            "marker" => {
                self.remove_fold_markers(start, end)?;
                self.refresh_folds();
                Ok(())
            }
            _ => Err(VimError::new(
                351,
                "Cannot delete fold with current 'foldmethod'",
            )),
        }
    }

    /// Remove fold markers from the start and end line (the reverse of
    /// insert_fold_markers).
    fn remove_fold_markers(&mut self, start: u32, end: u32) -> Result<(), VimError> {
        let options = self.fold_options(self.window_id());
        let (start_marker, end_marker) = match options.marker.split_once(',') {
            Some((start, end)) => (start.to_string(), end.to_string()),
            None => ("{{{".to_string(), "}}}".to_string()),
        };
        let commentstring = self
            .current_buffer()
            .map(|buffer| {
                self.options
                    .buffer_string(rvim_types::Id::raw(buffer.id), "commentstring")
            })
            .unwrap_or_default();
        let lines: Vec<(u32, String)> =
            vec![(start, start_marker.clone()), (end, end_marker.clone())];
        for (lnum, marker) in lines {
            let line = self
                .current_buffer()
                .and_then(|buffer| buffer.text.line(lnum))
                .map(|line| line.to_vec())
                .unwrap_or_default();
            let text = String::from_utf8_lossy(&line).into_owned();
            let Some(position) = text.find(&marker) else {
                continue;
            };
            let mut begin = position;
            let mut stop = position + marker.len();
            if let Some(ch) = text[stop..].chars().next() {
                if ch.is_ascii_digit() {
                    stop += ch.len_utf8();
                }
            }
            if let Some((comment_start, comment_end)) = commentstring.split_once("%s") {
                let comment_start = comment_start.trim();
                let comment_end = comment_end.trim();
                if !comment_start.is_empty()
                    && begin >= comment_start.len()
                    && text[begin - comment_start.len()..begin] == *comment_start
                    && text[stop..].starts_with(comment_end)
                {
                    begin -= comment_start.len();
                    stop += comment_end.len();
                }
            }
            let mut new_text = text;
            new_text.replace_range(begin..stop, "");
            self.record_undo();
            if let Some(buffer) = self.current_buffer_mut() {
                buffer.text.replace_line(lnum, VimString::from(new_text));
            }
            self.commit_undo();
        }
        Ok(())
    }

    /// Force the fold state to be recomputed (used by the Ex fold commands).
    pub(crate) fn sync_folds_public(&mut self) {
        self.sync_folds();
    }

    /// True when \p lnum is in a closed fold.
    pub(crate) fn fold_line_closed(&self, lnum: u32) -> bool {
        self.view
            .current_window()
            .is_some_and(|window| window.folds.is_hidden(lnum))
    }

    /// zE: eliminate every fold.
    pub(crate) fn eliminate_folds(&mut self) {
        let window = self.window_id();
        if let Some(window) = self.view.window_mut(window) {
            let mut state = window.folds.clone();
            state.manual.clear();
            state.forced_closed.clear();
            state.opened.clear();
            state.dirty = true;
            window.folds = state;
        }
        self.refresh_folds();
    }

    fn apply_fold_state(&mut self, state: FoldState) {
        let window = self.window_id();
        if let Some(window) = self.view.window_mut(window) {
            window.folds = state;
        }
    }

    fn window_id(&self) -> WindowId {
        self.view
            .current_window()
            .map_or(WindowId::from_raw(0), |window| window.id)
    }
}

fn manual_levels(manual: &[(u32, u32)], count: u32) -> Vec<i64> {
    let mut levels = vec![0i64; count as usize];
    for (start, end) in manual {
        let start = (*start).max(1);
        let end = (*end).min(count);
        for lnum in start..=end {
            if let Some(slot) = levels.get_mut((lnum - 1) as usize) {
                *slot += 1;
            }
        }
    }
    levels
}

fn marker_levels(options: &FoldOptions, lines: &[VimString]) -> Vec<i64> {
    let (start_marker, end_marker) = match options.marker.split_once(',') {
        Some((start, end)) => (start, end),
        None => ("{{{", "}}}"),
    };
    let mut levels = Vec::with_capacity(lines.len());
    let mut lvl: i64 = 0;
    let mut lvl_next: i64 = 0;
    for line in lines {
        let bytes = line.as_bytes();
        let mut position = 0usize;
        let mut level = lvl;
        let mut next = lvl_next;
        while position < bytes.len() {
            if !start_marker.is_empty() && bytes[position..].starts_with(start_marker.as_bytes()) {
                let after = position + start_marker.len();
                let mut digits = String::new();
                let mut scan = after;
                while scan < bytes.len() && bytes[scan].is_ascii_digit() {
                    digits.push(bytes[scan] as char);
                    scan += 1;
                }
                if let Ok(number) = digits.parse::<i64>() {
                    if number > 0 {
                        level = number;
                        next = number;
                    }
                } else {
                    level += 1;
                    next += 1;
                }
                position = scan.max(after);
                continue;
            }
            if !end_marker.is_empty() && bytes[position..].starts_with(end_marker.as_bytes()) {
                let after = position + end_marker.len();
                let mut digits = String::new();
                let mut scan = after;
                while scan < bytes.len() && bytes[scan].is_ascii_digit() {
                    digits.push(bytes[scan] as char);
                    scan += 1;
                }
                if let Ok(number) = digits.parse::<i64>() {
                    if number > 0 {
                        level = number;
                        next = number - 1;
                        if next > lvl {
                            next = lvl;
                        }
                    }
                } else {
                    next -= 1;
                }
                position = scan.max(after);
                continue;
            }
            position += 1;
        }
        if next < 0 {
            next = 0;
        }
        if level < 0 {
            level = 0;
        }
        levels.push(level);
        lvl = next;
        lvl_next = next;
    }
    for level in levels.iter_mut() {
        if *level > options.nest_max {
            *level = options.nest_max.max(0);
        }
    }
    levels
}

/// Undefined fold levels take the level of the next defined line; a trailing
/// run of undefined lines keeps level 0 (Vim skips over undefined lines).
fn fill_unknown_levels(levels: Vec<i64>) -> Vec<i64> {
    let mut out = levels;
    let mut index = out.len();
    let mut next = 0i64;
    while index > 0 {
        index -= 1;
        if out[index] < 0 {
            out[index] = next;
        } else {
            next = out[index];
        }
    }
    out
}

/// Default text of a closed fold when 'foldtext' is empty.
fn default_folded_text(start: u32, end: u32) -> String {
    let count = end.saturating_sub(start) + 1;
    if count == 1 {
        "+--  1 line folded ".to_string()
    } else {
        format!("+--{count:3} lines folded ")
    }
}

/// Vim's default foldtext() implementation for lines \p start..=\p end.
pub(crate) fn default_foldtext(
    lines: &[VimString],
    start: u32,
    end: u32,
    level: u16,
    marker: &str,
    commentstring: &str,
) -> String {
    if start == 0 || end < start || end as usize > lines.len() {
        return String::new();
    }
    let mut lnum = start;
    while lnum < end {
        let line = lines
            .get((lnum - 1) as usize)
            .map(|line| line.as_bytes())
            .unwrap_or(b"");
        if line.iter().any(|byte| *byte != b' ' && *byte != b'\t') {
            break;
        }
        lnum += 1;
    }
    let mut text = lines
        .get((lnum - 1) as usize)
        .map(|line| skip_white(line.as_bytes()).to_vec())
        .unwrap_or_default();
    // Skip a C comment opener, as Vim does.
    if text.starts_with(b"/*") || text.starts_with(b"//") {
        let rest = skip_white(&text[2..]).to_vec();
        if rest.is_empty() && lnum + 1 < end {
            let next = lines
                .get(lnum as usize)
                .map(|line| skip_white(line.as_bytes()).to_vec())
                .unwrap_or_default();
            text = if next.starts_with(b"*") {
                skip_white(&next[1..]).to_vec()
            } else {
                next
            };
        } else {
            text = rest;
        }
    } else if text.starts_with(b"*") {
        text = skip_white(&text[1..]).to_vec();
    }
    let count = end - start + 1;
    let dashes = "-".repeat(usize::from(level));
    let prefix = if count == 1 {
        format!("+-{dashes}{count:3} line: ")
    } else {
        format!("+-{dashes}{count:3} lines: ")
    };
    let body = String::from_utf8_lossy(&text).into_owned();
    let mut out = prefix;
    out.push_str(&body);
    foldtext_cleanup(&out, marker, commentstring)
}

fn skip_white(bytes: &[u8]) -> &[u8] {
    let mut index = 0;
    while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
        index += 1;
    }
    &bytes[index..]
}

/// Remove 'foldmarker' and 'commentstring' fragments from fold text, matching
/// Vim's foldtext_cleanup() closely enough for the common cases.
pub(crate) fn foldtext_cleanup(text: &str, marker: &str, commentstring: &str) -> String {
    let (start_marker, end_marker) = match marker.split_once(',') {
        Some((start, end)) => (start.to_string(), end.to_string()),
        None => ("{{{".to_string(), "}}}".to_string()),
    };
    let (comment_start, comment_end) = match commentstring.split_once("%s") {
        Some((start, end)) => (start.trim().to_string(), end.trim().to_string()),
        None => (String::new(), String::new()),
    };
    let mut out = text.to_string();
    for needle in [&start_marker, &end_marker] {
        if needle.is_empty() {
            continue;
        }
        while let Some(position) = out.find(needle.as_str()) {
            let mut begin = position;
            let mut stop = position + needle.len();
            if let Some(ch) = out[stop..].chars().next() {
                if ch.is_ascii_digit() {
                    stop += ch.len_utf8();
                }
            }
            if !comment_start.is_empty()
                && begin >= comment_start.len()
                && out[begin - comment_start.len()..begin] == comment_start
                && !comment_end.is_empty()
                && out[stop..].starts_with(&comment_end)
            {
                begin -= comment_start.len();
                stop += comment_end.len();
            }
            out.replace_range(begin..stop, "");
        }
    }
    out
}
