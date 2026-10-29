#![no_main]
//! Round-trip through the incompressibility probe. Fuzz inputs are small, and
//! the probe only looks at blocks of 128 KB or more, so each input becomes a
//! 256 KB block: random bytes seeded from the first 8 input bytes, with
//! compressible runs written at offsets and lengths taken from the rest.
use crush_parallel::{compress, decompress, EngineConfiguration};
use libfuzzer_sys::fuzz_target;

const LEN: usize = 256 * 1024;

fuzz_target!(|data: &[u8]| {
    let (head, rest) = data.split_at(data.len().min(8));
    let mut seed = [0u8; 8];
    seed[..head.len()].copy_from_slice(head);
    let mut state = u64::from_le_bytes(seed) | 1;
    let mut block: Vec<u8> = (0..LEN / 8)
        .flat_map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()
        })
        .collect();
    for run in rest.chunks_exact(3) {
        let start = usize::from(u16::from_le_bytes([run[0], run[1]])) * 4 % LEN;
        let end = (start + usize::from(run[2]) * 64).min(LEN);
        for (i, b) in block[start..end].iter_mut().enumerate() {
            *b = run[0].wrapping_add((i % 7) as u8);
        }
    }

    let config = EngineConfiguration::default();
    let compressed = compress(&block, &config).expect("compress failed");
    let recovered = decompress(&compressed, &config).expect("roundtrip decompress failed");
    assert_eq!(block, recovered, "roundtrip data mismatch");
});
