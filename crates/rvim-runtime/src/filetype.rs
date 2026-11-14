//! A small, dependency-free filetype detector.
//!
//! This is intentionally not a replacement for Vim's filetype.vim and
//! scripts.vim. It covers the common languages and file names needed before
//! the full runtime filetype machinery is available, and it is deterministic:
//! it only looks at the path and the supplied first line.
//!
//! Detection order matches Vim's useful default: file name first, then
//! extension, then shebang and obvious first-line content. A recognized
//! extension therefore wins over a contradictory shebang.

use std::path::Path;

/// Detect a Vim filetype from a path and the first line of the file.
///
/// The first line is used for shebang detection and a few unmistakable
/// content signatures (XML and PHP declarations, an HTML doctype). It may
/// include the trailing newline; bytes after the first line are ignored. A
/// UTF-8 BOM is skipped.
///
/// The return value is a Vim filetype name (lowercase, such as "rust" or
/// "cpp"), never an extension. Unknown files return None.
pub fn detect_filetype(path: &Path, first_line: &[u8]) -> Option<&'static str> {
    if let Some(filetype) = filename_filetype(path) {
        return Some(filetype);
    }
    if let Some(filetype) = extension_filetype(path) {
        return Some(filetype);
    }

    let line = first_line_bytes(first_line);
    shebang_filetype(line).or_else(|| content_filetype(line))
}

fn filename_filetype(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?;
    match name {
        "Makefile" | "makefile" | "GNUmakefile" | "BSDmakefile" | "Kbuild" => Some("make"),
        "CMakeLists.txt" => Some("cmake"),
        "Rakefile" | "Gemfile" | "Guardfile" | "Capfile" | "Vagrantfile" => Some("ruby"),
        "vimrc" | "gvimrc" | "_vimrc" | "_gvimrc" | ".vimrc" | ".gvimrc" | ".exrc" | "exrc" => {
            Some("vim")
        }
        ".bashrc" | ".bash_profile" | ".bash_login" | ".bash_logout" | ".profile" | ".zshrc"
        | ".zshenv" | ".zprofile" | ".zlogin" | ".kshrc" | "PKGBUILD" | "APKBUILD" => Some("sh"),
        _ => {
            if name.ends_with(".cmake.in") {
                return Some("cmake");
            }
            if name.starts_with("Makefile.") {
                return Some("make");
            }
            None
        }
    }
}

fn extension_filetype(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
    match extension {
        "rs" => Some("rust"),
        "c" | "h" => Some("c"),
        "C" | "H" | "cc" | "cp" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx" | "h++" | "ipp"
        | "tpp" | "inl" => Some("cpp"),
        "py" | "pyw" | "pyi" => Some("python"),
        "js" | "mjs" | "cjs" | "jsx" => Some("javascript"),
        "ts" | "mts" | "cts" | "tsx" => Some("typescript"),
        "json" | "jsonc" | "jsonl" => Some("json"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "md" | "markdown" | "mdown" | "mkd" | "mkdn" | "mdwn" | "mdx" => Some("markdown"),
        "sh" | "bash" | "bats" => Some("sh"),
        "vim" => Some("vim"),
        "html" | "htm" | "xhtml" | "shtml" => Some("html"),
        "css" => Some("css"),
        "go" => Some("go"),
        "java" => Some("java"),
        "lua" | "rockspec" => Some("lua"),
        "rb" | "ruby" | "rake" | "gemspec" | "ru" | "jbuilder" | "podspec" | "thor" | "rabl" => {
            Some("ruby")
        }
        "pl" | "pm" | "pod" | "t" | "psgi" => Some("perl"),
        "php" | "php3" | "php4" | "php5" | "php7" | "php8" | "phtml" | "phpt" => Some("php"),
        "mk" | "mak" | "make" => Some("make"),
        "cmake" => Some("cmake"),
        "tex" | "sty" | "cls" | "ltx" | "dtx" | "ins" => Some("tex"),
        "sql" => Some("sql"),
        "xml" | "xsl" | "xslt" | "xsd" | "svg" | "rss" | "atom" | "plist" | "wsdl" | "xul" => {
            Some("xml")
        }
        _ => None,
    }
}

fn first_line_bytes(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let end = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap_or(bytes.len());
    let line = &bytes[..end];
    if line.last() == Some(&b'\r') {
        &line[..line.len() - 1]
    } else {
        line
    }
}

fn shebang_filetype(line: &[u8]) -> Option<&'static str> {
    let line = line.strip_prefix(b"#!")?;
    let line = line.trim_ascii_start();
    let text = std::str::from_utf8(line).ok()?;
    let mut words = text.split_ascii_whitespace();
    let first = words.next()?;
    let program = if basename(first).eq_ignore_ascii_case("env") {
        env_program(&mut words)?
    } else {
        basename(first)
    };
    interpreter_filetype(program)
}

fn env_program<'a, I>(words: &mut I) -> Option<&'a str>
where
    I: Iterator<Item = &'a str>,
{
    let mut skip_next = false;
    for word in words.by_ref() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if matches!(word, "-u" | "--unset" | "-C" | "--chdir") {
            skip_next = true;
            continue;
        }
        if let Some(value) = word.strip_prefix("--split-string=") {
            return Some(basename(value));
        }
        if let Some(value) = word.strip_prefix("-S") {
            let value = value.strip_prefix('=').unwrap_or(value);
            if !value.is_empty() {
                return Some(basename(value));
            }
            continue;
        }
        if word.starts_with('-') || is_env_assignment(word) {
            continue;
        }
        return Some(basename(word));
    }
    None
}

fn is_env_assignment(word: &str) -> bool {
    let Some((name, _value)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn interpreter_filetype(program: &str) -> Option<&'static str> {
    let lower = program.to_ascii_lowercase();
    let name = lower.strip_suffix(".exe").unwrap_or(&lower);

    if name.starts_with("python") || name.starts_with("pypy") {
        return Some("python");
    }
    if name.starts_with("node") {
        return Some("javascript");
    }
    if matches!(name, "ts-node" | "tsx" | "deno") {
        return Some("typescript");
    }
    if matches!(name, "rust-script" | "cargo-script" | "rustc") {
        return Some("rust");
    }
    if matches!(
        name,
        "sh" | "bash"
            | "dash"
            | "ash"
            | "ksh"
            | "mksh"
            | "pdksh"
            | "oksh"
            | "zsh"
            | "yash"
            | "posh"
    ) || name.starts_with("ksh")
    {
        return Some("sh");
    }
    if name.starts_with("ruby") || name == "jruby" {
        return Some("ruby");
    }
    if name.starts_with("perl") {
        return Some("perl");
    }
    if name.starts_with("php") {
        return Some("php");
    }
    if name.starts_with("lua") {
        return Some("lua");
    }
    if name.starts_with("vim") {
        return Some("vim");
    }
    if matches!(name, "make" | "gmake" | "bmake") {
        return Some("make");
    }
    if name == "cmake" {
        return Some("cmake");
    }
    if name.starts_with("java") {
        return Some("java");
    }
    None
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn content_filetype(line: &[u8]) -> Option<&'static str> {
    let line = line.trim_ascii_start();
    if starts_with_ignore_ascii_case(line, b"<?php") {
        return Some("php");
    }
    if starts_with_ignore_ascii_case(line, b"<?xml") {
        return Some("xml");
    }
    if starts_with_ignore_ascii_case(line, b"<!doctype html")
        || starts_with_ignore_ascii_case(line, b"<html")
    {
        return Some("html");
    }
    None
}

fn starts_with_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn detects_common_extensions() {
        let cases = [
            ("main.rs", "rust"),
            ("main.c", "c"),
            ("main.h", "c"),
            ("main.cpp", "cpp"),
            ("main.C", "cpp"),
            ("main.hpp", "cpp"),
            ("main.py", "python"),
            ("main.js", "javascript"),
            ("main.ts", "typescript"),
            ("data.json", "json"),
            ("config.toml", "toml"),
            ("config.yaml", "yaml"),
            ("config.yml", "yaml"),
            ("README.md", "markdown"),
            ("run.sh", "sh"),
            ("init.vim", "vim"),
            ("index.html", "html"),
            ("style.css", "css"),
            ("main.go", "go"),
            ("Main.java", "java"),
            ("init.lua", "lua"),
            ("app.rb", "ruby"),
            ("lib.pl", "perl"),
            ("index.php", "php"),
            ("rules.mk", "make"),
            ("tool.cmake", "cmake"),
            ("paper.tex", "tex"),
            ("query.sql", "sql"),
            ("layout.xml", "xml"),
        ];
        for (name, expected) in cases {
            assert_eq!(
                detect_filetype(Path::new(name), b""),
                Some(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn detects_special_file_names() {
        let cases = [
            ("Makefile", "make"),
            ("GNUmakefile", "make"),
            ("Makefile.am", "make"),
            ("CMakeLists.txt", "cmake"),
            ("project.cmake.in", "cmake"),
            ("Rakefile", "ruby"),
            ("vimrc", "vim"),
            (".vimrc", "vim"),
            (".bashrc", "sh"),
        ];
        for (name, expected) in cases {
            assert_eq!(
                detect_filetype(Path::new(name), b""),
                Some(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn detects_shebangs() {
        let cases = [
            ("#!/usr/bin/env python3", "python"),
            ("#!/usr/bin/env -S python3 -u", "python"),
            ("#!/usr/bin/python3.12", "python"),
            ("#!/bin/bash -eu", "sh"),
            ("#!/bin/sh", "sh"),
            ("#!/usr/bin/env zsh", "sh"),
            ("#!/usr/bin/env node", "javascript"),
            ("#!/usr/bin/env ts-node", "typescript"),
            ("#!/usr/bin/ruby", "ruby"),
            ("#!/usr/bin/perl -w", "perl"),
            ("#!/usr/bin/php", "php"),
            ("#!/usr/bin/lua5.4", "lua"),
            ("#!/usr/bin/vim -S", "vim"),
            ("#!/usr/bin/env rust-script", "rust"),
            ("#!/usr/bin/env -u FOO ruby", "ruby"),
            ("#!/usr/bin/env FOO=bar python3", "python"),
        ];
        for (line, expected) in cases {
            assert_eq!(
                detect_filetype(Path::new("unknown"), line.as_bytes()),
                Some(expected),
                "{line}"
            );
        }
    }

    #[test]
    fn extension_wins_over_contradictory_shebang() {
        assert_eq!(
            detect_filetype(Path::new("tool.py"), b"#!/bin/sh\n"),
            Some("python")
        );
    }

    #[test]
    fn detects_obvious_first_line_content() {
        let cases = [
            ("<?xml version=\"1.0\"?>", "xml"),
            ("<?php echo 1; ?>", "php"),
            ("<!DOCTYPE html>", "html"),
            ("<html lang=\"en\">", "html"),
        ];
        for (line, expected) in cases {
            assert_eq!(
                detect_filetype(Path::new("unknown"), line.as_bytes()),
                Some(expected),
                "{line}"
            );
        }
    }

    #[test]
    fn handles_bom_crlf_and_unknown_files() {
        assert_eq!(
            detect_filetype(
                Path::new("unknown"),
                b"\xEF\xBB\xBF#!/usr/bin/env python3\r\n"
            ),
            Some("python")
        );
        assert_eq!(detect_filetype(Path::new("notes.txt"), b"plain text"), None);
        assert_eq!(detect_filetype(Path::new("unknown"), b""), None);
        assert_eq!(detect_filetype(Path::new("script"), b"#!/nonsense"), None);
    }
}
