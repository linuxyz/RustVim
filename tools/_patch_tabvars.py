import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
s = s.replace("use rvim_types::{Arena, BufPos, BufferId, VimError, VimString, WindowId};", "use rvim_types::{Arena, BufPos, BufferId, TabId, VimError, VimString, WindowId};", 1)
s = s.replace("""    pub jumplist: Vec<(BufferId, BufPos)>,""", """    pub jumplist: Vec<(BufferId, BufPos)>,
    pub tab_vars: HashMap<TabId, HashMap<String, rvim_viml::Value>>,""", 1)
s = s.replace("""            jumplist: Vec::new(),""", """            jumplist: Vec::new(),
            tab_vars: HashMap::new(),""", 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = '''            "wildmenumode" => Ok(Some(Value::Number(i64::from('''
arms = '''            "tabpagenr" => {
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
                let value = args
                    .first()
                    .map(|value| value.echo_string())
                    .unwrap_or_default();
                let ids = self.editor.view.tab_ids();
                let tab = if value == "$" {
                    ids.len()
                } else {
                    value
                        .parse::<usize>()
                        .ok()
                        .filter(|index| *index >= 1 && *index <= ids.len())
                        .unwrap_or_else(|| {
                            ids.iter()
                                .position(|id| *id == self.editor.view.current_tab)
                                .unwrap_or(0)
                                + 1
                        })
                };
                let windows = ids
                    .get(tab.saturating_sub(1))
                    .and_then(|id| self.editor.view.tabs.get(id))
                    .map_or(0, |page| rvim_view::layout::all_windows(&page.root).len());
                Ok(Some(Value::Number(windows.max(1) as i64)))
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
                    .and_then(|id| self.editor.tab_vars.get(id))
                    .and_then(|vars| vars.get(&name))
                    .cloned()
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
                    self.editor
                        .tab_vars
                        .entry(id)
                        .or_default()
                        .insert(name, value);
                }
                Ok(Some(Value::Number(0)))
            }
            "wildmenumode" => Ok(Some(Value::Number(i64::from('''
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("host patched")
