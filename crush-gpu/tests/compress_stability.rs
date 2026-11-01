//! Tile compression (feature 027, US2): archive bytes never depend on the
//! thread count, and match the serial encoder's, pinned by hash from the last
//! serial build.
#![allow(clippy::expect_used)]
// Plain `assert!(a == b)`: `assert_eq!` would print megabytes on failure.
#![allow(clippy::manual_assert_eq)]

use crush_core::CrushError;
use crush_gpu::engine::{EngineConfig, compress, decompress};
use std::sync::atomic::AtomicBool;

/// Hash of `corpus()` compressed, from the last serial build.
const PINNED: u64 = 15_922_416_221_075_715_487;

/// Text, random and zero regions over several tiles, with a short last tile.
fn corpus() -> Vec<u8> {
    let mut out = Vec::with_capacity(400_000);
    let mut x = 0x1234_5678_9abc_def0_u64;
    let words: [&[u8]; 5] = [b"crush ", b"block ", b"parallel ", b"tile ", b"frame "];
    while out.len() < 150_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(words[(x % 5) as usize]);
    }
    while out.len() < 280_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.resize(400_000, 0);
    out
}

fn fnv(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn in_pool<T: Send>(threads: usize, f: impl FnOnce() -> T + Send) -> T {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("pool")
        .install(f)
}

fn cpu_config() -> EngineConfig {
    EngineConfig {
        force_cpu: true,
        ..EngineConfig::default()
    }
}

#[test]
fn output_matches_the_serial_encoder_at_every_pool_size() {
    let data = corpus();
    let cancel = AtomicBool::new(false);
    for threads in [1, 2, 8] {
        let packed = in_pool(threads, || {
            compress(&data, &cpu_config(), &cancel).expect("compress")
        });
        assert_eq!(fnv(&packed), PINNED, "{threads} threads");
        assert!(decompress(&packed, &cpu_config(), &cancel).expect("decompress") == data);
    }
}

#[test]
fn short_inputs_round_trip() {
    let data = corpus();
    let cancel = AtomicBool::new(false);
    for len in [1, 100, 65_536, 65_537] {
        let packed = compress(&data[..len], &cpu_config(), &cancel).expect("compress");
        assert!(decompress(&packed, &cpu_config(), &cancel).expect("decompress") == data[..len]);
    }
}

#[test]
fn a_set_flag_stops_compression() {
    let data = corpus();
    let cancel = AtomicBool::new(true);
    for threads in [1, 4] {
        in_pool(threads, || {
            let result = compress(&data, &cpu_config(), &cancel);
            assert!(matches!(result, Err(CrushError::Cancelled)));
        });
    }
}
