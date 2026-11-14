use std::time::Instant;

use rvim_editor::Editor;
use rvim_ui_api::Size;

fn main() {
    let path = std::env::temp_dir().join("rvim-bench.txt");
    let mut text = String::new();
    for index in 0..2000 {
        text.push_str(&format!("line {index} with some words and punctuation\n"));
    }
    std::fs::write(&path, text.as_bytes()).unwrap();

    let started = Instant::now();
    let mut editor = Editor::new(Size::new(24, 80));
    editor.swap_enabled = false;
    editor.open_file(&path).unwrap();
    let startup = started.elapsed();

    let started = Instant::now();
    for _ in 0..200 {
        let _ = editor.render();
    }
    let render = started.elapsed();

    let started = Instant::now();
    editor.execute_ex("set number");
    editor.execute_ex("normal! G");
    editor.execute_ex("normal! gg");
    let navigation = started.elapsed();

    println!(
        "startup/open 2000 lines: {:.3} ms",
        startup.as_secs_f64() * 1000.0
    );
    println!(
        "render 80x24 x200:       {:.3} ms",
        render.as_secs_f64() * 1000.0
    );
    println!(
        "navigation:              {:.3} ms",
        navigation.as_secs_f64() * 1000.0
    );
    let _ = std::fs::remove_file(path);
}
