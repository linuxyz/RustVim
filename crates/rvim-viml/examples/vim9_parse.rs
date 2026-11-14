//! Parse a Vim9 script and report syntax errors.
//!
//! Used by tools/vim9_diff.py as the parser acceptance gate:
//!   cargo run -q -p rvim-viml --example vim9_parse -- FILE
//! prints "OK" on success (exit 0) or "error: E&lt;code&gt;: ..." on stderr (exit 1).

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: vim9_parse [--tokens] FILE");
        return ExitCode::from(2);
    }
    let mut tokens = false;
    let mut path = None;
    for arg in &args[1..] {
        if arg == "--tokens" {
            tokens = true;
        } else if path.is_none() {
            path = Some(arg.clone());
        } else {
            eprintln!("usage: vim9_parse [--tokens] FILE");
            return ExitCode::from(2);
        }
    }
    let Some(path) = path else {
        eprintln!("usage: vim9_parse [--tokens] FILE");
        return ExitCode::from(2);
    };
    let source = match std::fs::read(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {path}: {error}");
            return ExitCode::from(2);
        }
    };
    if tokens {
        match rvim_viml::vim9::lexer::tokenize(&source) {
            Ok(tokens) => {
                for token in tokens {
                    println!("{:?} @{}:{}", token.kind, token.line, token.col);
                }
                return ExitCode::SUCCESS;
            }
            Err(error) => {
                eprintln!("error: {}", error.error_text());
                return ExitCode::FAILURE;
            }
        }
    }
    match rvim_viml::vim9::parser::parse_script(&source) {
        Ok(script) => {
            let _ = script.items.len();
            println!("OK");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {}", error.error_text());
            ExitCode::FAILURE
        }
    }
}
