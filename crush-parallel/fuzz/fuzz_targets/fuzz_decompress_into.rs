#![no_main]
use crush_parallel::codec::DeflateCodec;
use crush_parallel::{decompress_into, decompressed_size, EngineConfiguration};
use libfuzzer_sys::fuzz_target;

// The slice API behind mapped output: sizes come from `decompressed_size`, and
// neither call may panic on any input.
fuzz_target!(|data: &[u8]| {
    let config = EngineConfiguration::default();
    let Ok(len) = decompressed_size::<DeflateCodec>(data, &config) else {
        return;
    };
    if len > 64 << 20 {
        return;
    }
    let mut output = vec![0u8; len];
    let _ = decompress_into(&DeflateCodec::default(), data, &config, &mut output);
});
