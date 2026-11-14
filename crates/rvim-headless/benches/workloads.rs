use std::time::Instant;

use rvim_editor::Editor;
use rvim_regex::{compile, find_all, Magic};
use rvim_syntax::SyntaxEngine;
use rvim_ui_api::Size;

fn main() {
    // Startup and open of a large file (50k lines).
    let path = std::env::temp_dir().join(format!("rvim-bench-large-{}.txt", std::process::id()));
    let mut text = String::with_capacity(2_000_000);
    for index in 0..50_000 {
        text.push_str(&format!("line {index} with some words and punctuation\n"));
    }
    std::fs::write(&path, text.as_bytes()).unwrap();
    let started = Instant::now();
    let mut editor = Editor::new(Size::new(24, 80));
    editor.swap_enabled = false;
    editor.open_file(&path).unwrap();
    let open = started.elapsed();
    let started = Instant::now();
    for _ in 0..200 {
        let _ = editor.render();
    }
    let render = started.elapsed();
    let _ = std::fs::remove_file(&path);

    // Regex engine: compile once, scan a large haystack repeatedly.
    let pattern = compile(b"\\v(\\w+)\\@(\\w+)\\.com", Magic::VeryMagic, false).unwrap();
    let mut haystack = String::new();
    for index in 0..2_000 {
        haystack.push_str(&format!("user{index}@example{index}.com and more text\n"));
    }
    let started = Instant::now();
    let mut matches = 0usize;
    for _ in 0..20 {
        matches += find_all(&pattern, haystack.as_bytes()).len();
    }
    let regex = started.elapsed();

    // Syntax engine: a handful of rules over a large file.
    let mut engine = SyntaxEngine::new();
    engine.set_enabled(true);
    engine
        .define("keyword Keyword if else while for return")
        .unwrap();
    engine.define("match Comment +//.*$+").unwrap();
    engine.define("region String start=+\"+ end=+\"+").unwrap();
    let lines: Vec<Vec<u8>> = (0..5_000)
        .map(|index| {
            format!("if value{index} == \"string {index}\" // trailing comment").into_bytes()
        })
        .collect();
    let started = Instant::now();
    let mut spans = 0usize;
    for line in &lines {
        spans += engine.highlight_line(line).len();
    }
    let syntax = started.elapsed();

    println!("open 50k lines:  {:.3} ms", open.as_secs_f64() * 1000.0);
    println!("render x200:     {:.3} ms", render.as_secs_f64() * 1000.0);
    println!(
        "regex scan x20:  {:.3} ms ({matches} matches)",
        regex.as_secs_f64() * 1000.0
    );
    println!(
        "syntax 5k lines: {:.3} ms ({spans} spans)",
        syntax.as_secs_f64() * 1000.0
    );
}
