#![no_main]

use libfuzzer_sys::fuzz_target;
use rvim_regex::{compile, Magic};

fuzz_target!(|data: &[u8]| {
    if let Ok(regex) = compile(data, Magic::Magic, false) {
        let _ = regex.find(data);
    }
});
