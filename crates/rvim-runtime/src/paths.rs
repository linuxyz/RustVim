//! Vim 'runtimepath' discovery and file lookup.

use std::env;
use std::path::{Component, Path, PathBuf};

use rvim_platform::env as platform_env;

/// The Vim runtime tree and the ordered directories searched for runtime
/// files.
///
/// Construct this with RuntimePaths::discover. The configured entries are
/// exposed verbatim by RuntimePaths::runtimepath; environment variables and
/// tilde prefixes are expanded only when a lookup is performed (see
/// RuntimePaths::expanded_runtimepath). This mirrors Vim, where the option
/// value keeps the placeholders and the search expands them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePaths {
    vimruntime: Option<PathBuf>,
    runtimepath: Vec<PathBuf>,
}

impl RuntimePaths {
    /// Discover the runtime tree and runtimepath.
    ///
    /// vimruntime is an explicit override, normally the result of a
    /// --runtime-style command line option. When it is Some, it wins and is
    /// used only if it names an existing directory; when it is None, the
    /// $VIMRUNTIME environment variable and a small set of install layouts are
    /// tried. A missing runtime tree produces None rather than a fictitious
    /// path.
    ///
    /// runtimepath is the raw option value. When it is Some, it is split with
    /// split_runtimepath exactly as supplied and $VIMRUNTIME is appended if it
    /// is not already present. When it is None, the platform default (the user
    /// directory, system vimfiles, the runtime tree and their after
    /// directories) is used.
    pub fn discover(vimruntime: Option<PathBuf>, runtimepath: Option<String>) -> Self {
        let vimruntime = resolve_vimruntime(vimruntime);
        let runtimepath = match runtimepath {
            Some(value) => {
                let mut entries = split_runtimepath(&value);
                append_vimruntime(&mut entries, vimruntime.as_deref());
                entries
            }
            None => default_runtimepath(vimruntime.as_deref()),
        };
        Self {
            vimruntime,
            runtimepath,
        }
    }

    /// The discovered $VIMRUNTIME directory, if one exists.
    ///
    /// An explicit path that does not exist is not reported. Callers can use
    /// RuntimePaths::find_in_vimruntime to search it.
    pub fn vimruntime(&self) -> Option<&Path> {
        self.vimruntime.as_deref()
    }

    /// The configured runtimepath, in the order it was supplied.
    ///
    /// Entries are not normalized, deduplicated or reordered. Empty entries
    /// from the option value are represented as Path::new("."). Use
    /// RuntimePaths::expanded_runtimepath for entries with tilde prefixes and
    /// environment variables expanded.
    pub fn runtimepath(&self) -> &[PathBuf] {
        &self.runtimepath
    }

    /// Runtimepath entries with tilde, $VAR and $NAME-in-braces expanded.
    ///
    /// The order, empty-entry dot values and duplicates are preserved. If a
    /// variable is not set, the literal $NAME text is kept, matching Vim's
    /// behaviour of not silently deleting part of a path.
    pub fn expanded_runtimepath(&self) -> Vec<PathBuf> {
        self.runtimepath
            .iter()
            .map(|entry| expand_entry(entry, self.vimruntime.as_deref()))
            .collect()
    }

    /// Find the first runtime-relative file in runtimepath order.
    ///
    /// name must be a relative path such as "ftplugin/rust.vim". Absolute
    /// paths and paths containing ".." are rejected. The check is for a
    /// regular file; directories are not returned. The returned path retains
    /// the runtimepath entry's spelling (for example "./plugin/x.vim" for an
    /// empty entry).
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        let relative = relative_runtime_name(name)?;
        for entry in &self.runtimepath {
            let candidate = expand_entry(entry, self.vimruntime.as_deref()).join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// Find every runtime-relative file in runtimepath order.
    ///
    /// This is the lookup order used by Vim's :runtime!: every matching entry
    /// is returned, including duplicates, and after directories are searched
    /// where they were configured. A missing file yields an empty vector.
    pub fn find_all(&self, name: &str) -> Vec<PathBuf> {
        let Some(relative) = relative_runtime_name(name) else {
            return Vec::new();
        };
        let mut matches = Vec::new();
        for entry in &self.runtimepath {
            let candidate = expand_entry(entry, self.vimruntime.as_deref()).join(relative);
            if candidate.is_file() {
                matches.push(candidate);
            }
        }
        matches
    }

    /// Find a runtime-relative file only in $VIMRUNTIME.
    ///
    /// This ignores user runtimepath entries. It returns None when no runtime
    /// tree was discovered or the file does not exist.
    pub fn find_in_vimruntime(&self, name: &str) -> Option<PathBuf> {
        let runtime = self.vimruntime.as_deref()?;
        let relative = relative_runtime_name(name)?;
        let candidate = runtime.join(relative);
        candidate.is_file().then_some(candidate)
    }

    /// The runtimepath used for filetype-specific files (ftplugin/, indent/,
    /// syntax/), in load order.
    ///
    /// Vim searches these directories in runtimepath order; after directories
    /// are not moved, because the option value is authoritative about
    /// ordering. This returns the expanded entries and performs no filesystem
    /// filtering, so callers can decide whether to skip missing directories.
    pub fn rtp_for_filetype(&self) -> Vec<PathBuf> {
        self.expanded_runtimepath()
    }
}

/// Split a Vim comma-separated path option into entries.
///
/// Commas separate entries. A backslash immediately before a comma or an
/// ASCII space escapes it: the backslash is removed and the character becomes
/// part of the entry. A backslash before any other character is kept, which is
/// essential for Windows-style paths such as C:\vim.
///
/// Empty entries mean the current directory, as they do for Vim's
/// 'runtimepath'; they are returned as PathBuf::from("."). A completely empty
/// input therefore has one entry, dot.
///
/// The result is a pure parse: no filesystem access, normalization or
/// environment-variable expansion is performed.
pub fn split_runtimepath(value: &str) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(&next) = chars.peek() {
                    if next == ',' || next == ' ' {
                        current.push(next);
                        chars.next();
                    } else {
                        current.push('\\');
                    }
                } else {
                    current.push('\\');
                }
            }
            ',' => {
                entries.push(path_entry(&current));
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    entries.push(path_entry(&current));
    entries
}

fn path_entry(entry: &str) -> PathBuf {
    if entry.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(entry)
    }
}

fn relative_runtime_name(name: &str) -> Option<&Path> {
    let path = Path::new(name);
    if path.as_os_str().is_empty() {
        return None;
    }
    let has_unsafe_component = path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if has_unsafe_component {
        None
    } else {
        Some(path)
    }
}

fn resolve_vimruntime(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return path.is_dir().then_some(path);
    }
    if let Some(path) = platform_env::vimruntime() {
        if path.is_dir() {
            return Some(path);
        }
    }
    vimruntime_candidates()
        .into_iter()
        .find(|path| path.is_dir())
}

fn vimruntime_candidates() -> Vec<PathBuf> {
    let version_dir = vim_version_dir();
    let mut candidates = Vec::new();
    if let Some(vim) = env_path("VIM") {
        candidates.push(vim.join("runtime"));
        candidates.push(vim.join(&version_dir));
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(dir) = executable.parent() {
            candidates.push(dir.join("runtime"));
            candidates.push(dir.join("../runtime"));
            candidates.push(dir.join("../share/rvim/runtime"));
            candidates.push(dir.join("../share/vim").join(&version_dir));
        }
    }
    #[cfg(unix)]
    {
        candidates.push(PathBuf::from("/usr/local/share/rvim/runtime"));
        candidates.push(PathBuf::from("/usr/local/share/vim").join(&version_dir));
        candidates.push(PathBuf::from("/opt/homebrew/share/rvim/runtime"));
        candidates.push(PathBuf::from("/opt/homebrew/share/vim").join(&version_dir));
        candidates.push(PathBuf::from("/usr/share/rvim/runtime"));
        candidates.push(PathBuf::from("/usr/share/vim").join(&version_dir));
    }
    candidates.push(PathBuf::from("runtime"));
    candidates
}

fn vim_version_dir() -> String {
    let mut parts = rvim_types::VIM_VERSION.split('.');
    let major = parts.next().unwrap_or("9");
    let minor = parts.next().unwrap_or("2");
    format!("vim{major}{minor}")
}

fn default_runtimepath(vimruntime: Option<&Path>) -> Vec<PathBuf> {
    let home = platform_env::home();
    let user_dir = default_user_dir(&home);
    let vim_dir = vim_install_dir(vimruntime);

    let mut entries = vec![user_dir.clone()];
    if let Some(vim) = &vim_dir {
        entries.push(vim.join("vimfiles"));
    }
    if let Some(runtime) = vimruntime {
        entries.push(runtime.to_path_buf());
    }
    if let Some(vim) = &vim_dir {
        entries.push(vim.join("vimfiles").join("after"));
    }
    entries.push(user_dir.join("after"));
    entries
}

#[cfg(windows)]
fn default_user_dir(home: &Path) -> PathBuf {
    home.join("vimfiles")
}

#[cfg(not(windows))]
fn default_user_dir(home: &Path) -> PathBuf {
    env_path("XDG_CONFIG_HOME")
        .map(|dir| dir.join("vim"))
        .unwrap_or_else(|| home.join(".vim"))
}

fn vim_install_dir(vimruntime: Option<&Path>) -> Option<PathBuf> {
    if let Some(vim) = env_path("VIM") {
        return Some(vim);
    }
    let runtime = vimruntime?;
    let name = runtime.file_name()?.to_str()?;
    if name == "runtime" || name.starts_with("vim") {
        runtime.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    platform_env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn append_vimruntime(entries: &mut Vec<PathBuf>, vimruntime: Option<&Path>) {
    let Some(vimruntime) = vimruntime else {
        return;
    };
    let already_present = entries.iter().any(|entry| {
        let expanded = expand_entry(entry, Some(vimruntime));
        paths_equivalent(&expanded, vimruntime)
    });
    if !already_present {
        entries.push(vimruntime.to_path_buf());
    }
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    if left.exists() && right.exists() {
        if let (Ok(left), Ok(right)) = (left.canonicalize(), right.canonicalize()) {
            if left == right {
                return true;
            }
        }
    }
    normalized_components(left).eq(normalized_components(right))
}

fn normalized_components(path: &Path) -> impl Iterator<Item = Component<'_>> {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
}

fn expand_entry(entry: &Path, vimruntime: Option<&Path>) -> PathBuf {
    let Some(text) = entry.to_str() else {
        return entry.to_path_buf();
    };

    let mut expanded = String::with_capacity(text.len());
    let rest = if text == "~" {
        return platform_env::home();
    } else if let Some(rest) = text.strip_prefix("~/") {
        expanded.push_str(&platform_env::home().to_string_lossy());
        expanded.push('/');
        rest
    } else {
        text
    };

    expand_variables(rest, vimruntime, &mut expanded);
    PathBuf::from(expanded)
}

fn expand_variables(text: &str, vimruntime: Option<&Path>, output: &mut String) {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'$' {
            if let Some((name, next)) = variable_at(text, index + 1) {
                if let Some(value) = variable_value(name, vimruntime) {
                    output.push_str(&value);
                } else {
                    output.push_str(&text[index..next]);
                }
                index = next;
                continue;
            }
        }
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        output.push(ch);
        index += ch.len_utf8();
    }
}

fn variable_at(text: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    if start >= bytes.len() {
        return None;
    }
    if bytes[start] == b'{' {
        let end = text[start + 1..].find('}')? + start + 1;
        let name = &text[start + 1..end];
        if name.is_empty() || !name.bytes().all(is_variable_byte) {
            return None;
        }
        return Some((name, end + 1));
    }

    let first = *bytes.get(start)?;
    if !first.is_ascii_alphabetic() && first != b'_' {
        return None;
    }
    let mut end = start + 1;
    while end < bytes.len() && is_variable_byte(bytes[end]) {
        end += 1;
    }
    Some((&text[start..end], end))
}

fn is_variable_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn variable_value(name: &str, vimruntime: Option<&Path>) -> Option<String> {
    if name == "VIMRUNTIME" {
        if let Some(runtime) = vimruntime {
            return Some(runtime.to_string_lossy().into_owned());
        }
    }
    platform_env::var(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(value: &[&str]) -> Vec<PathBuf> {
        value.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn splits_a_plain_list() {
        assert_eq!(
            split_runtimepath("/one,/two,/three"),
            paths(&["/one", "/two", "/three"])
        );
    }

    #[test]
    fn empty_entries_mean_the_current_directory() {
        assert_eq!(split_runtimepath(""), paths(&["."]));
        assert_eq!(
            split_runtimepath("/one,,/two,"),
            paths(&["/one", ".", "/two", "."])
        );
        assert_eq!(split_runtimepath(",/one"), paths(&[".", "/one"]));
        assert_eq!(
            split_runtimepath("/one, ,/two"),
            paths(&["/one", " ", "/two"])
        );
    }

    #[test]
    fn backslashes_escape_commas_and_spaces() {
        assert_eq!(split_runtimepath(r"/a\,b,/c\ d"), paths(&["/a,b", "/c d"]));
        assert_eq!(split_runtimepath(r"/trailing\,"), paths(&["/trailing,"]));
        assert_eq!(split_runtimepath(r"/trailing\ "), paths(&["/trailing "]));
    }

    #[test]
    fn other_backslashes_are_preserved() {
        assert_eq!(
            split_runtimepath(r"C:\vim,C:\my\ files"),
            paths(&[r"C:\vim", r"C:\my files"])
        );
        assert_eq!(split_runtimepath(r"/a\/b"), paths(&[r"/a\/b"]));
        assert_eq!(split_runtimepath(r"/a\\,b"), paths(&[r"/a\,b"]));
        assert_eq!(split_runtimepath(r"/ends\"), paths(&[r"/ends\"]));
    }

    #[test]
    fn relative_names_reject_absolute_and_parent_paths() {
        assert!(relative_runtime_name("ftplugin/rust.vim").is_some());
        assert!(relative_runtime_name("/etc/passwd").is_none());
        assert!(relative_runtime_name("../outside").is_none());
        assert!(relative_runtime_name("ftplugin/../outside").is_none());
        assert!(relative_runtime_name("").is_none());
    }

    #[test]
    fn variable_expansion_keeps_unknown_names() {
        let runtime = Path::new("/opt/vim/runtime");
        assert_eq!(
            expand_entry(Path::new("$VIMRUNTIME/doc"), Some(runtime)),
            PathBuf::from("/opt/vim/runtime/doc")
        );
        assert_eq!(
            expand_entry(Path::new("$RVIM_RUNTIME_TEST_UNSET/doc"), Some(runtime)),
            PathBuf::from("$RVIM_RUNTIME_TEST_UNSET/doc")
        );
        assert_eq!(
            expand_entry(Path::new(concat!("$", "{VIMRUNTIME}/doc")), Some(runtime)),
            PathBuf::from("/opt/vim/runtime/doc")
        );
    }

    #[test]
    fn missing_explicit_runtime_is_reported_as_none() {
        let missing = env::temp_dir().join(format!(
            "rvim-runtime-does-not-exist-{}-{}",
            std::process::id(),
            1338_u32
        ));
        let discovered = RuntimePaths::discover(Some(missing), Some("/tmp/rtp".to_string()));
        assert!(discovered.vimruntime().is_none());
        assert_eq!(discovered.runtimepath(), paths(&["/tmp/rtp"]).as_slice());
    }
}
