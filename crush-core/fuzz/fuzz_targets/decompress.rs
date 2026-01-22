#![no_main]

use crush_core::{decompress, init_plugins};
use libfuzzer_sys::fuzz_target;
use std::sync::Once;

static INIT: Once = Once::new();

fuzz_target!(|data: &[u8]| {
    // Initialize plugins once
    INIT.call_once(|| {
        init_plugins().expect("Plugin initialization failed");
    });

    // Fuzz decompress function with arbitrary input
    // Should never panic regardless of input
    // Most inputs will fail validation (bad header, CRC, etc.) which is expected
    let _ = decompress(data);
});
