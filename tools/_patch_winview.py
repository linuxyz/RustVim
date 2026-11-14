import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "winnr" => {"""
arms = """            "winsaveview" => {
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
                dict.set(VimString::from("curswant"), Value::Number(cursor.col as i64 + 1));
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
                let skipcol = get("skipcol") as u16;
                self.editor.set_cursor(BufPos::new(lnum, col));
                if let Some(window) = self.editor.view.current_window_mut() {
                    window.viewport.topline = topline;
                    window.viewport.leftcol = leftcol;
                    window.viewport.skipcol = skipcol;
                }
                Ok(Some(Value::Number(0)))
            }
            "winnr" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
