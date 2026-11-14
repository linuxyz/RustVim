import pathlib
p = pathlib.Path("crates/rvim-editor/src/options.rs")
s = p.read_text()
old = """        let local = !global_only
            && (local
                || matches!(
                    spec.scope,
                    OptionScope::Buffer
                        | OptionScope::Window
                        | OptionScope::Tab
                        | OptionScope::GlobalLocalBuffer
                        | OptionScope::GlobalLocalWindow
                ));"""
new = """        let explicit_local = local;
        let scoped = matches!(
            spec.scope,
            OptionScope::Buffer
                | OptionScope::Window
                | OptionScope::Tab
                | OptionScope::GlobalLocalBuffer
                | OptionScope::GlobalLocalWindow
        );
        let local = !global_only && (explicit_local || scoped);
        let both = !global_only
            && !explicit_local
            && matches!(
                spec.scope,
                OptionScope::GlobalLocalBuffer | OptionScope::GlobalLocalWindow
            );"""
assert old in s
s = s.replace(old, new, 1)
# bool branch set global too
old2 = """            } else {
                store.set_global(spec.name, OptionValue::Bool(state))?;
            }
            continue;"""
new2 = """            } else {
                store.set_global(spec.name, OptionValue::Bool(state))?;
            }
            if both {
                store.set_global(spec.name, OptionValue::Bool(state))?;
            }
            continue;"""
assert old2 in s
s = s.replace(old2, new2, 1)
# non-bool both
old3 = """        let append = token.contains("+=");
        store.set_from_string(spec.name, value, local, buffer, window, append)?;"""
new3 = """        let append = token.contains("+=");
        store.set_from_string(spec.name, value, local, buffer, window, append)?;
        if both {
            store.set_from_string(spec.name, value, false, buffer, window, append)?;
        }"""
assert old3 in s
s = s.replace(old3, new3, 1)
# reset both
old4 = """            if local {
                let value = default_value(spec.default);
                if matches!(
                    spec.scope,
                    OptionScope::Window | OptionScope::GlobalLocalWindow
                ) {
                    store.set_window(window, spec.name, value)?;
                } else {
                    store.set_buffer(buffer, spec.name, value)?;
                }
            } else {
                store.reset_global(spec.name)?;
            }
            continue;"""
new4 = """            if local {
                let value = default_value(spec.default);
                if matches!(
                    spec.scope,
                    OptionScope::Window | OptionScope::GlobalLocalWindow
                ) {
                    store.set_window(window, spec.name, value)?;
                } else {
                    store.set_buffer(buffer, spec.name, value)?;
                }
            } else {
                store.reset_global(spec.name)?;
            }
            if both {
                store.reset_global(spec.name)?;
            }
            continue;"""
assert old4 in s
s = s.replace(old4, new4, 1)
p.write_text(s)
print("patched")
