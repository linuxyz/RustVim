use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rvim_runtime::{detect_filetype, split_runtimepath, RuntimePaths};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
static CWD_LOCK: Mutex<()> = Mutex::new(());

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "rvim-runtime-test-{}-{}-{}",
            process::id(),
            label,
            id
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temporary directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn dir(&self, relative: &str) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(&path).expect("create temporary subdirectory");
        path
    }

    fn file(&self, relative: &str, contents: &[u8]) -> PathBuf {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directory");
        }
        fs::write(&path, contents).expect("write temporary file");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct CwdGuard {
    original: PathBuf,
}

impl CwdGuard {
    fn change_to(path: &Path) -> Self {
        let original = env::current_dir().expect("read current directory");
        env::set_current_dir(path).expect("change current directory");
        Self { original }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = env::set_current_dir(&self.original);
    }
}

fn rtp_entry(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == ',' || ch == ' ' {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn rtp_list(paths: &[&Path]) -> String {
    paths
        .iter()
        .map(|path| rtp_entry(path))
        .collect::<Vec<_>>()
        .join(",")
}

fn first_line(path: &Path) -> Vec<u8> {
    let bytes = fs::read(path).expect("read temporary file");
    match bytes.iter().position(|byte| *byte == b'\n') {
        Some(end) => bytes[..end].to_vec(),
        None => bytes,
    }
}

#[test]
fn find_all_preserves_runtimepath_and_after_order() {
    let runtime = TempDir::new("rt");
    let root = TempDir::new("roots");
    let one = root.dir("one");
    let two = root.dir("two");

    for base in [&one, &two] {
        fs::create_dir_all(base.join("plugin")).expect("create plugin directory");
        fs::write(base.join("plugin/order.vim"), b"\" order\n").expect("write plugin");
    }

    let one_after = one.join("after");
    let two_after = two.join("after");
    for base in [&one_after, &two_after] {
        fs::create_dir_all(base.join("plugin")).expect("create after plugin directory");
        fs::write(base.join("plugin/order.vim"), b"\" after\n").expect("write plugin");
    }

    // The first after directory is deliberately in the middle. Vim does not
    // move after entries; they are searched where the option puts them.
    let value = rtp_list(&[&one, &one_after, &two, &two_after]);
    let paths = RuntimePaths::discover(Some(runtime.path().to_path_buf()), Some(value));

    let expected = vec![
        one.join("plugin/order.vim"),
        one_after.join("plugin/order.vim"),
        two.join("plugin/order.vim"),
        two_after.join("plugin/order.vim"),
    ];
    assert_eq!(paths.find_all("plugin/order.vim"), expected);
    assert_eq!(paths.find("plugin/order.vim"), Some(expected[0].clone()));

    assert_eq!(paths.runtimepath()[0], one);
    assert_eq!(paths.runtimepath()[1], one_after);
    assert_eq!(paths.runtimepath()[2], two);
    assert_eq!(paths.runtimepath()[3], two_after);
    assert_eq!(paths.runtimepath()[4], runtime.path());

    assert_eq!(paths.rtp_for_filetype(), paths.expanded_runtimepath());
    assert_eq!(paths.rtp_for_filetype().len(), 5);
}

#[test]
fn empty_entries_search_the_current_directory() {
    let _lock = CWD_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let runtime = TempDir::new("empty-rt");
    let cwd = TempDir::new("empty-cwd");
    cwd.file("plugin/from-cwd.vim", b"\" cwd\n");

    let _cwd = CwdGuard::change_to(cwd.path());
    let paths = RuntimePaths::discover(Some(runtime.path().to_path_buf()), Some(String::new()));

    assert_eq!(paths.runtimepath()[0], PathBuf::from("."));
    assert_eq!(
        paths.runtimepath().last(),
        Some(&runtime.path().to_path_buf())
    );
    assert_eq!(
        paths.find("plugin/from-cwd.vim"),
        Some(PathBuf::from("./plugin/from-cwd.vim"))
    );
    assert_eq!(
        paths.find_all("plugin/from-cwd.vim"),
        vec![PathBuf::from("./plugin/from-cwd.vim")]
    );
}

#[test]
fn empty_entries_in_the_middle_stay_in_place() {
    let runtime = TempDir::new("middle-rt");
    let root = TempDir::new("middle-root");
    let one = root.dir("one");
    let two = root.dir("two");
    fs::create_dir_all(one.join("plugin")).expect("create plugin directory");
    fs::create_dir_all(two.join("plugin")).expect("create plugin directory");
    fs::write(one.join("plugin/x.vim"), b"").expect("write plugin");
    fs::write(two.join("plugin/x.vim"), b"").expect("write plugin");

    let value = format!("{},,{}", rtp_entry(&one), rtp_entry(&two));
    let paths = RuntimePaths::discover(Some(runtime.path().to_path_buf()), Some(value));

    assert_eq!(paths.runtimepath()[0], one);
    assert_eq!(paths.runtimepath()[1], PathBuf::from("."));
    assert_eq!(paths.runtimepath()[2], two);

    let parsed = split_runtimepath(",a,,b,");
    assert_eq!(
        parsed,
        vec![
            PathBuf::from("."),
            PathBuf::from("a"),
            PathBuf::from("."),
            PathBuf::from("b"),
            PathBuf::from("."),
        ]
    );
}

#[test]
fn escaped_commas_and_spaces_are_single_entries() {
    let runtime = TempDir::new("escape-rt");
    let parent = TempDir::new("escape-root");
    let comma = parent.dir("comma,dir");
    let space = parent.dir("space dir");
    parent.file("comma,dir/plugin/a.vim", b"\" comma\n");
    parent.file("space dir/plugin/b.vim", b"\" space\n");

    let value = format!("{},{}", rtp_entry(&comma), rtp_entry(&space));
    let paths = RuntimePaths::discover(Some(runtime.path().to_path_buf()), Some(value));

    assert_eq!(paths.runtimepath()[0], comma);
    assert_eq!(paths.runtimepath()[1], space);
    assert_eq!(paths.find("plugin/a.vim"), Some(comma.join("plugin/a.vim")));
    assert_eq!(paths.find("plugin/b.vim"), Some(space.join("plugin/b.vim")));
}

#[test]
fn vimruntime_is_appended_once_and_searched_separately() {
    let runtime = TempDir::new("append-rt");
    runtime.file("plugin/builtin.vim", b"\" runtime\n");
    let user = TempDir::new("append-user");
    user.file("plugin/builtin.vim", b"\" user\n");

    let paths = RuntimePaths::discover(
        Some(runtime.path().to_path_buf()),
        Some(rtp_entry(user.path())),
    );
    assert_eq!(paths.vimruntime(), Some(runtime.path()));
    assert_eq!(paths.runtimepath().len(), 2);
    assert_eq!(paths.runtimepath()[0], user.path());
    assert_eq!(paths.runtimepath()[1], runtime.path());
    assert_eq!(
        paths.find("plugin/builtin.vim"),
        Some(user.path().join("plugin/builtin.vim"))
    );
    assert_eq!(
        paths.find_in_vimruntime("plugin/builtin.vim"),
        Some(runtime.path().join("plugin/builtin.vim"))
    );
    assert_eq!(
        paths.find_all("plugin/builtin.vim"),
        vec![
            user.path().join("plugin/builtin.vim"),
            runtime.path().join("plugin/builtin.vim"),
        ]
    );

    let duplicate = RuntimePaths::discover(
        Some(runtime.path().to_path_buf()),
        Some(format!(
            "{},{}",
            rtp_entry(runtime.path()),
            rtp_entry(user.path())
        )),
    );
    assert_eq!(duplicate.runtimepath().len(), 2);
    assert_eq!(duplicate.runtimepath()[0], runtime.path());
}

#[test]
fn literal_vimruntime_is_expanded_and_not_duplicated() {
    let runtime = TempDir::new("literal-rt");
    runtime.file("plugin/builtin.vim", b"\" runtime\n");

    let paths = RuntimePaths::discover(
        Some(runtime.path().to_path_buf()),
        Some("$VIMRUNTIME".to_string()),
    );
    assert_eq!(paths.runtimepath(), &[PathBuf::from("$VIMRUNTIME")]);
    assert_eq!(
        paths.expanded_runtimepath(),
        vec![runtime.path().to_path_buf()]
    );
    assert_eq!(paths.rtp_for_filetype(), vec![runtime.path().to_path_buf()]);
    assert_eq!(
        paths.find("plugin/builtin.vim"),
        Some(runtime.path().join("plugin/builtin.vim"))
    );
}

#[test]
fn missing_runtime_files_are_reported_as_missing() {
    let root = TempDir::new("missing-root");
    let missing_runtime = root.path().join("no-such-runtime");
    let missing_rtp = root.path().join("no-such-rtp");
    let paths = RuntimePaths::discover(Some(missing_runtime), Some(rtp_entry(&missing_rtp)));
    assert!(paths.vimruntime().is_none());
    assert!(paths.find("plugin/missing.vim").is_none());
    assert!(paths.find_all("plugin/missing.vim").is_empty());
    assert!(paths.find_in_vimruntime("plugin/missing.vim").is_none());
    assert!(paths.find("../escape.vim").is_none());
    assert!(paths.find_all("/etc/passwd").is_empty());
}

#[test]
fn default_runtimepath_contains_runtime_and_after_directories() {
    let runtime = TempDir::new("default-rt");
    let paths = RuntimePaths::discover(Some(runtime.path().to_path_buf()), None);

    assert_eq!(paths.vimruntime(), Some(runtime.path()));
    assert!(
        paths
            .runtimepath()
            .iter()
            .any(|entry| entry == runtime.path()),
        "default runtimepath should contain $VIMRUNTIME"
    );
    let last = paths.runtimepath().last().expect("default runtimepath");
    assert!(
        last.ends_with("after"),
        "the user after directory should be last: {}",
        last.display()
    );
}

#[test]
fn detects_filetypes_from_temporary_files() {
    let files = TempDir::new("filetypes");
    let cases: &[(&str, &[u8], Option<&str>)] = &[
        ("module.rs", b"fn main() {}\n", Some("rust")),
        ("main.c", b"int main(void) { return 0; }\n", Some("c")),
        ("main.cpp", b"int main() { return 0; }\n", Some("cpp")),
        (
            "tool.py",
            b"#!/usr/bin/env python3\nprint('x')\n",
            Some("python"),
        ),
        ("script", b"#!/bin/bash\necho hi\n", Some("sh")),
        (
            "run",
            b"#!/usr/bin/env node\nconsole.log(1)\n",
            Some("javascript"),
        ),
        ("config.ts", b"export const x = 1;\n", Some("typescript")),
        ("data.json", b"{}\n", Some("json")),
        ("Cargo.toml", b"[package]\n", Some("toml")),
        ("config.yml", b"key: value\n", Some("yaml")),
        ("NOTES.md", b"# Notes\n", Some("markdown")),
        ("init.vim", b"set nocompatible\n", Some("vim")),
        ("page.html", b"<html></html>\n", Some("html")),
        ("style.css", b"body {}\n", Some("css")),
        ("main.go", b"package main\n", Some("go")),
        ("Main.java", b"class Main {}\n", Some("java")),
        ("init.lua", b"print('x')\n", Some("lua")),
        ("app.rb", b"puts 'x'\n", Some("ruby")),
        ("lib.pl", b"print 'x';\n", Some("perl")),
        ("index.php", b"<?php echo 1; ?>\n", Some("php")),
        ("Makefile", b"all:\n\techo hi\n", Some("make")),
        ("CMakeLists.txt", b"project(x)\n", Some("cmake")),
        ("paper.tex", b"\\documentclass{article}\n", Some("tex")),
        ("query.sql", b"SELECT 1;\n", Some("sql")),
        ("layout.xml", b"<root/>\n", Some("xml")),
        (
            "weird.unknown",
            b"<?xml version=\"1.0\"?>\n<root/>\n",
            Some("xml"),
        ),
        ("notes.txt", b"plain text\n", None),
    ];

    for (name, contents, expected) in cases {
        let path = files.file(name, contents);
        let line = first_line(&path);
        assert_eq!(
            detect_filetype(&path, &line),
            *expected,
            "filetype for {name}"
        );
    }
}
