//! Parallel `fast-lz` compression (feature 027, US1): output never depends on
//! the thread count or the entry point, and matches the serial encoder's
//! bytes, pinned by hash from the last serial build.
#![allow(clippy::expect_used)]
// Plain `assert!(a == b)`: `assert_eq!` would print megabytes on failure.
#![allow(clippy::manual_assert_eq)]

use crush_core::plugin::CompressionAlgorithm;
use crush_core::{CrushError, PluginError};
use crush_lz::{FastLzConfig, FastLzPlugin, compress, decompress};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Hash of `corpus()` compressed, from the last serial build.
const PINNED: u64 = 9_550_317_885_279_242_457;

/// Text, random and zero regions, over several default blocks, with a short
/// last block.
fn corpus() -> Vec<u8> {
    let mut out = Vec::with_capacity(5_500_000);
    let mut x = 0x1234_5678_9abc_def0_u64;
    let words: [&[u8]; 5] = [b"crush ", b"block ", b"parallel ", b"tile ", b"frame "];
    while out.len() < 2_000_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(words[(x % 5) as usize]);
    }
    while out.len() < 3_500_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.resize(5_500_000, 0);
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

fn live() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[test]
fn output_matches_the_serial_encoder_at_every_pool_size() {
    let data = corpus();
    for threads in [1, 2, 3, 8] {
        let packed = in_pool(threads, || {
            compress(&data, &FastLzConfig::default()).expect("compress")
        });
        assert_eq!(fnv(&packed), PINNED, "{threads} threads");
        assert!(decompress(&packed, data.len()).expect("decompress") == data);
    }
}

#[test]
fn every_entry_point_writes_the_same_bytes() {
    let data = corpus();
    let plugin = FastLzPlugin;
    for threads in [1, 4] {
        in_pool(threads, || {
            let whole = plugin.compress(&data, live()).expect("compress");

            let mut appended = b"prefix".to_vec();
            plugin
                .compress_into(&data, live(), &mut appended)
                .expect("compress_into");
            assert!(appended[..6] == *b"prefix" && appended[6..] == whole);

            let mut streamed = Vec::new();
            let written = plugin
                .compress_to(&data, live(), &mut streamed)
                .expect("compress_to");
            assert_eq!(written, streamed.len() as u64);
            assert!(streamed == whole);
        });
    }
}

#[test]
fn inputs_up_to_one_block_round_trip() {
    let data = corpus();
    for len in [0, 1, 7, 64 * 1024, 1024 * 1024] {
        let packed = compress(&data[..len], &FastLzConfig::default()).expect("compress");
        assert!(decompress(&packed, len).expect("decompress") == data[..len]);
    }
}

fn is_cancelled<T>(result: &crush_core::Result<T>) -> bool {
    matches!(
        result,
        Err(CrushError::Plugin(PluginError::Cancelled) | CrushError::Cancelled)
    )
}

#[test]
fn a_set_flag_stops_compression() {
    let data = corpus();
    let flag = Arc::new(AtomicBool::new(true));
    for threads in [1, 4] {
        in_pool(threads, || {
            assert!(is_cancelled(
                &FastLzPlugin.compress(&data, Arc::clone(&flag))
            ));
            let mut sink = Vec::new();
            assert!(is_cancelled(&FastLzPlugin.compress_to(
                &data,
                Arc::clone(&flag),
                &mut sink
            )));
        });
    }
}

/// Sets the flag after its first write.
struct CancelAfterFirstWrite {
    flag: Arc<AtomicBool>,
}

impl Write for CancelAfterFirstWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.flag.store(true, Ordering::Release);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn cancelling_during_the_output_stops_compression() {
    let data = corpus();
    for threads in [1, 4] {
        in_pool(threads, || {
            let flag = live();
            let mut sink = CancelAfterFirstWrite {
                flag: Arc::clone(&flag),
            };
            assert!(is_cancelled(
                &FastLzPlugin.compress_to(&data, flag, &mut sink)
            ));
        });
    }
}
