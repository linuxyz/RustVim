#![no_main]

use libfuzzer_sys::fuzz_target;
use rvim_syntax::SyntaxEngine;

fuzz_target!(|data: &[u8]| {
    let Ok(definition) = std::str::from_utf8(data) else {
        return;
    };
    let mut engine = SyntaxEngine::new();
    engine.set_enabled(true);
    let _ = engine.define(definition);
    let _ = engine.highlight_line(data);
});
