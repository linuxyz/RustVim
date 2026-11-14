#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = rvim_viml::Parser::parse_complete(data);
    let _ = rvim_viml::parse_script(data);
});
