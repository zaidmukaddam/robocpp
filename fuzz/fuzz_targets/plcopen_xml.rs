// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use iec_plcopen::import_plcopen_xml;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(xml) = std::str::from_utf8(data) {
        let _ = import_plcopen_xml("fuzz.xml", xml);
    }
});
