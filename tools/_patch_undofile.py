import pathlib
p = pathlib.Path("crates/rvim-editor/src/lib.rs")
s = p.read_text()
if "pub mod undofile;" not in s:
    s = s.replace("pub mod viminfo;", "pub mod undofile;\npub mod viminfo;", 1)
p.write_text(s)
print("lib patched")
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
# methods before shutdown_swaps
anchor = """    pub fn shutdown_swaps(&mut self) {
        self.swaps.clear();
    }"""
methods = """    pub fn load_undo_file(&mut self, id: BufferId) {
        if !self.options.get_bool("undofile") {
            return;
        }
        let Some(buffer) = self.buffers.get(id) else {
            return;
        };
        let name = buffer.name_string();
        let Some(path) = crate::undofile::path_for(self, &name) else {
            return;
        };
        let Ok(previous) = crate::undofile::read(&path) else {
            return;
        };
        let current = buffer.text.lines_vec();
        if previous == current {
            return;
        }
        if let Some(buffer) = self.buffers.get_mut(id) {
            buffer
                .undo
                .seed(previous, current, BufPos::new(1, 0));
        }
    }

    pub fn save_undo_file(&mut self, id: BufferId) {
        if !self.options.get_bool("undofile") {
            return;
        }
        let Some(path) = self
            .buffers
            .get(id)
            .and_then(|buffer| crate::undofile::path_for(self, &buffer.name_string()))
        else {
            return;
        };
        let Some(buffer) = self.buffers.get(id) else {
            return;
        };
        let current = buffer.text.lines_vec();
        let Some(root) = buffer.undo.root_lines() else {
            let _ = std::fs::remove_file(path);
            return;
        };
        if root == current {
            let _ = std::fs::remove_file(path);
            return;
        }
        let _ = crate::undofile::write(&path, &root);
    }

    pub fn shutdown_swaps(&mut self) {
        let ids: Vec<BufferId> = self.buffers.iter().map(|(id, _)| id).collect();
        for id in ids {
            self.save_undo_file(id);
        }
        self.swaps.clear();
    }"""
assert anchor in s
s = s.replace(anchor, methods, 1)
# load in load_buffer after set_modified false
s = s.replace("""            buffer.set_modified(false);
        }
        Ok(())
    }

    pub(crate) fn add_unlisted_buffer(""", """            buffer.set_modified(false);
        }
        self.load_undo_file(id);
        Ok(())
    }

    pub(crate) fn add_unlisted_buffer(""", 1)
# load in open_path after buffer loaded config: use after set_modified false earlier
s = s.replace("""        buffer.load_bytes(&decoded.text);
        buffer.set_modified(false);""", """        buffer.load_bytes(&decoded.text);
        buffer.set_modified(false);
        let loaded_buffer_id = rvim_types::Id::raw(self.current_buffer);
        self.load_undo_file(loaded_buffer_id);""", 1)
# write hooks: after write_file_atomic in write_buffer and range write? add after message? Use specific occurrences
s = s.replace("""        self.messages.info(format!(
            "\\"{}\\" {}L, {}B written",
            path.display(),""", """        self.save_undo_file(buffer_id);
        self.messages.info(format!(
            "\\"{}\\" {}L, {}B written",
            path.display(),""", 1)
# second write_buffer occurrence: after write atomic in write_buffer
s = s.replace("""        } else {
            pfs::write_file_atomic(&path, &bytes).map_err(|e| VimError::new(13, e.to_string()))?;
        }
        let bytes_written = bytes.len();""", """        } else {
            pfs::write_file_atomic(&path, &bytes).map_err(|e| VimError::new(13, e.to_string()))?;
        }
        let written_buffer = self.current_buffer_id();
        self.save_undo_file(written_buffer);
        let bytes_written = bytes.len();""", 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-types/src/version.rs")
s = p.read_text()
s = s.replace('"cscope" | "netbeans_intg" | "persistent_undo" => false,', '"cscope" | "netbeans_intg" => false,', 1)
s = s.replace('"syntax" | "eval" | "user_commands" | "cmdline_hist" | "cmdline_info" | "viminfo" => true,', '"syntax" | "eval" | "user_commands" | "cmdline_hist" | "cmdline_info" | "viminfo"\n        | "persistent_undo" => true,', 1)
p.write_text(s)
print("version patched")
