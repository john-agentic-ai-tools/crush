#![no_main]
use libfuzzer_sys::fuzz_target;
use crush_parallel::{decompress, EngineConfiguration};

fuzz_target!(|data: &[u8]| {
    let config = EngineConfiguration::default();
    // Must never panic — any input is valid, only errors are returned.
    let _ = decompress(data, &config);
});
