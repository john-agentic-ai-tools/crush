#![no_main]
//! Round trips of arbitrary input, both as given and tiled to just over three
//! 64-kilobyte blocks, so block boundaries, stored blocks and long matches
//! across a block's whole length are exercised.
use crush_lz::{FastLzConfig, compress, decompress};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let config = FastLzConfig::builder()
        .block_size_log2(16)
        .build()
        .expect("valid config");
    let check = |input: &[u8]| {
        let packed = compress(input, &config).expect("compress");
        let out = decompress(&packed, input.len()).expect("decompress");
        assert_eq!(out, input, "round trip mismatch");
    };
    check(data);
    if !data.is_empty() {
        let tiled: Vec<u8> = data.iter().copied().cycle().take(3 * 65_536 + 7).collect();
        check(&tiled);
    }
});
