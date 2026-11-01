//! Cancellation stops work before the remaining blocks run (R12).
#![allow(clippy::expect_used, clippy::missing_panics_doc)]

use crush_core::error::{CrushError, Result};
use crush_parallel::codec::BlockCodec;
use crush_parallel::{EngineConfiguration, compress_with, decompress_with};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Copies blocks, counts them, and raises `flag` once `after` blocks are done.
struct Tripwire {
    flag: Arc<AtomicBool>,
    after: usize,
    calls: AtomicUsize,
}

impl Tripwire {
    fn tick(&self) {
        if self.calls.fetch_add(1, Ordering::SeqCst) + 1 >= self.after {
            self.flag.store(true, Ordering::Release);
        }
    }
}

impl BlockCodec for Tripwire {
    const CODEC_ID: u8 = 201;
    type Encoder = ();
    type Decoder = ();

    fn encoder(&self) -> Result<()> {
        Ok(())
    }

    fn decoder(&self) -> Result<()> {
        Ok(())
    }

    fn compress_block(&self, (): &mut (), input: &[u8], out: &mut Vec<u8>) -> Result<bool> {
        self.tick();
        out.clear();
        out.extend_from_slice(input);
        Ok(true)
    }

    fn decompress_block(&self, (): &mut (), payload: &[u8], output: &mut [u8]) -> Result<()> {
        self.tick();
        output.copy_from_slice(payload);
        Ok(())
    }
}

const BLOCKS: usize = 64;

fn config(flag: Option<Arc<AtomicBool>>) -> EngineConfiguration {
    let builder = EngineConfiguration::builder().block_size(65_536).workers(1);
    match flag {
        Some(f) => builder.cancel_flag(f),
        None => builder,
    }
    .build()
    .expect("config")
}

fn wire(flag: &Arc<AtomicBool>, after: usize) -> Tripwire {
    Tripwire {
        flag: Arc::clone(flag),
        after,
        calls: AtomicUsize::new(0),
    }
}

#[test]
fn compression_stops_before_remaining_blocks() {
    let flag = Arc::new(AtomicBool::new(false));
    let codec = wire(&flag, 3);
    let input = vec![7u8; 65_536 * BLOCKS];
    let result = compress_with(&codec, &input, &config(Some(flag)));
    assert!(matches!(result, Err(CrushError::Cancelled)));
    assert!(codec.calls.load(Ordering::SeqCst) < BLOCKS);
}

#[test]
fn decompression_stops_before_remaining_blocks() {
    let flag = Arc::new(AtomicBool::new(false));
    let unused = wire(&flag, usize::MAX);
    let input = vec![7u8; 65_536 * BLOCKS];
    let packed = compress_with(&unused, &input, &config(None)).expect("compress");

    let codec = wire(&flag, 3);
    let result = decompress_with(&codec, &packed, &config(Some(flag)));
    assert!(matches!(result, Err(CrushError::Cancelled)));
    assert!(codec.calls.load(Ordering::SeqCst) < BLOCKS);
}
