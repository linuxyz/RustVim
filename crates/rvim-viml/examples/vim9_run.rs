//! Execute a Vim9 script and print the messages produced by :echo/:echomsg.
//!
//! Used by the differential harness:
//!   cargo run -q -p rvim-viml --example vim9_run -- FILE
//! One message per line on stdout, like Vim's :redir capture.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: vim9_run FILE");
        return ExitCode::from(2);
    }
    let source = match std::fs::read(&args[1]) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", args[1]);
            return ExitCode::from(2);
        }
    };
    match rvim_viml::vim9::eval::run_script(&source) {
        Ok(output) => {
            for message in output.messages {
                println!("{message}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {}", error.error_text());
            ExitCode::FAILURE
        }
    }
}
