import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """        let tag_index = runtime
            .find("doc/tags")
            .map(|path| TagIndex::load(&path))
            .unwrap_or_default();"""
add = anchor + """
        let runtimepath = runtime
            .runtimepath()
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(",");
        let _ = options.set_global(
            "runtimepath",
            crate::options::OptionValue::String(VimString::from(runtimepath.clone())),
        );
        let _ = options.set_global(
            "packpath",
            crate::options::OptionValue::String(VimString::from(runtimepath)),
        );"""
assert anchor in s
s = s.replace(anchor, add, 1)
# field
s = s.replace("""    pub(crate) redir_target: Option<RedirTarget>,""", """    loaded_packages: std::collections::HashSet<String>,
    pub(crate) redir_target: Option<RedirTarget>,""", 1)
s = s.replace("""            redir_target: None,""", """            loaded_packages: std::collections::HashSet::new(),
            redir_target: None,""", 1)
# methods before update_window_buffer
anchor2 = """    pub(crate) fn update_window_buffer(&mut self) {"""
methods = """    fn find_package(&self, name: &str) -> Option<std::path::PathBuf> {
        let packpath = self.options.get_string("packpath");
        for entry in rvim_runtime::split_runtimepath(&packpath) {
            let entry = rvim_platform::fs::expand_env(&entry.to_string_lossy());
            let packroot = entry.join("pack");
            let Ok(packages) = std::fs::read_dir(&packroot) else {
                continue;
            };
            for package in packages.flatten() {
                for kind in ["start", "opt"] {
                    let candidate = package.path().join(kind).join(name);
                    if candidate.is_dir() {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    fn add_runtimepath_entry(&mut self, dir: &std::path::Path) {
        let mut runtimepath = self.options.get_string("runtimepath");
        if !runtimepath.is_empty() {
            runtimepath.push(',');
        }
        runtimepath.push_str(&dir.to_string_lossy());
        let _ = self.options.set_global(
            "runtimepath",
            crate::options::OptionValue::String(VimString::from(runtimepath.clone())),
        );
        self.runtime = RuntimePaths::discover(
            self.runtime.vimruntime().map(|path| path.to_path_buf()),
            Some(runtimepath),
        );
    }

    fn source_package_plugins(&mut self, dir: &std::path::Path) -> Result<(), VimError> {
        let mut files = Vec::new();
        collect_vim_files(&dir.join("plugin"), &mut files);
        files.sort();
        for path in files {
            let text = rvim_platform::fs::read_file(&path)
                .map_err(|error| VimError::new(13, error.to_string()))?;
            self.source_viml(&text, &path.display().to_string())?;
        }
        Ok(())
    }

    pub(crate) fn packadd(&mut self, name: &str, load: bool) -> Result<(), VimError> {
        let Some(dir) = self.find_package(name) else {
            return Err(VimError::new(
                919,
                format!("E919: Directory not found in 'packpath': pack/*/opt/{name}"),
            ));
        };
        if !self.loaded_packages.insert(name.to_string()) {
            return Ok(());
        }
        self.add_runtimepath_entry(&dir);
        if load {
            self.source_package_plugins(&dir)?;
        }
        Ok(())
    }

    pub(crate) fn packloadall(&mut self) -> Result<(), VimError> {
        let packpath = self.options.get_string("packpath");
        let mut names = Vec::new();
        for entry in rvim_runtime::split_runtimepath(&packpath) {
            let entry = rvim_platform::fs::expand_env(&entry.to_string_lossy());
            let Ok(packages) = std::fs::read_dir(entry.join("pack")) else {
                continue;
            };
            for package in packages.flatten() {
                let Ok(entries) = std::fs::read_dir(package.path().join("start")) else {
                    continue;
                };
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        }
        names.sort();
        names.dedup();
        for name in names {
            self.packadd(&name, true)?;
        }
        Ok(())
    }

    pub(crate) fn update_window_buffer(&mut self) {"""
assert anchor2 in s
s = s.replace(anchor2, methods, 1)
# free helper at end of file
s += """

fn collect_vim_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_vim_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "vim") {
            out.push(path);
        }
    }
}
"""
p.write_text(s)
print("editor patched")
