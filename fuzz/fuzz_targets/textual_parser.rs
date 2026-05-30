// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use iec_syntax::parse_project;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let _ = parse_project("fuzz.st", source);
    }
});
