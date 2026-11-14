import pathlib
p = pathlib.Path("crates/rvim-editor/src/viml_host.rs")
s = p.read_text()
anchor = """            "getpos" => {"""
arms = """            "virtcol" => {
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
                let width = rvim_view::view::display_prefix_width(
                    line,
                    col.min(u16::MAX as u32) as u16,
                    &options,
                );
                Ok(Some(Value::Number(width as i64 + 1)))
            }
            "virtcol2col" => {
                let values = match args.first() {
                    Some(Value::List(list)) => list.borrow().items.clone(),
                    _ => Vec::new(),
                };
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
                let mut col = 0u32;
                while col < line.len() as u32 {
                    let width = rvim_view::view::display_prefix_width(
                        &line,
                        col.min(u16::MAX as u32) as u16,
                        &options,
                    ) as u32
                        + 1;
                    if width >= target {
                        break;
                    }
                    let step = rvim_view::view::display_prefix_width(
                        &line,
                        (col + 1).min(u16::MAX as u32) as u16,
                        &options,
                    ) as u32
                        + 1;
                    col += step.saturating_sub(width).max(1);
                }
                Ok(Some(Value::Number(col as i64 + 1)))
            }
            "getpos" => {"""
assert anchor in s
s = s.replace(anchor, arms, 1)
p.write_text(s)
print("patched")
