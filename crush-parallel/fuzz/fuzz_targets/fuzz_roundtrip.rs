#![no_main]
use libfuzzer_sys::fuzz_target;
use crush_parallel::{compress, decompress, EngineConfiguration};

fuzz_target!(|data: &[u8]| {
    let config = EngineConfiguration::default();
    if let Ok(compressed) = compress(data, &config) {
        let recovered = decompress(&compressed, &config).expect("roundtrip decompress failed");
        assert_eq!(data, recovered.as_slice(), "roundtrip data mismatch");
    }
});
