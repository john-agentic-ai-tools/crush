//! `BlockCodec` contract tests (`specs/026-hybrid-speed-codec/contracts/block-codec.md`).
#![allow(clippy::expect_used, clippy::missing_panics_doc)]

use crush_core::error::{CrushError, Result};
use crush_parallel::codec::{BlockCodec, DeflateCodec};
use crush_parallel::{
    EngineConfiguration, compress_with, decompress_block_with, decompress_with, load_index,
};
use std::io::Cursor;

/// Toy codec: a length-checked byte inversion, so tests need no real entropy coder.
struct Invert;

impl BlockCodec for Invert {
    const CODEC_ID: u8 = 200;
    type Encoder = ();
    type Decoder = ();

    fn encoder(&self) -> Result<()> {
        Ok(())
    }

    fn decoder(&self) -> Result<()> {
        Ok(())
    }

    fn compress_block(&self, (): &mut (), input: &[u8], out: &mut Vec<u8>) -> Result<bool> {
        out.clear();
        out.extend(input.iter().map(|b| !b));
        Ok(true)
    }

    fn decompress_block(&self, (): &mut (), payload: &[u8], output: &mut [u8]) -> Result<()> {
        if payload.len() != output.len() {
            return Err(CrushError::InvalidFormat("bad payload length".to_owned()));
        }
        for (o, p) in output.iter_mut().zip(payload) {
            *o = !p;
        }
        Ok(())
    }
}

fn data() -> Vec<u8> {
    (0..300_000u32)
        .flat_map(|i| (i / 7).to_le_bytes())
        .collect()
}

fn config(workers: usize) -> EngineConfiguration {
    EngineConfiguration::builder()
        .block_size(65_536)
        .workers(workers)
        .build()
        .expect("config")
}

#[test]
fn a_custom_codec_round_trips_through_the_engine() {
    let input = data();
    let packed = compress_with(&Invert, &input, &config(2)).expect("compress");
    let output = decompress_with(&Invert, &packed, &config(2)).expect("decompress");
    assert_eq!(output, input);
}

#[test]
fn output_does_not_depend_on_worker_count() {
    let input = data();
    let one = compress_with(&Invert, &input, &config(1)).expect("compress");
    let four = compress_with(&Invert, &input, &config(4)).expect("compress");
    assert_eq!(one, four);
}

#[test]
fn a_block_decodes_alone() {
    let input = data();
    let packed = compress_with(&Invert, &input, &config(2)).expect("compress");
    let mut reader = Cursor::new(&packed);
    let index = load_index(&mut reader).expect("index");
    let block =
        decompress_block_with(&Invert, &mut reader, &index, 3, &config(1)).expect("block 3");
    assert_eq!(block, input[3 * 65_536..4 * 65_536]);
}

#[test]
fn a_file_is_refused_by_the_wrong_codec() {
    let packed = compress_with(&Invert, &data(), &config(2)).expect("compress");
    assert!(decompress_with(&DeflateCodec::default(), &packed, &config(2)).is_err());
}

#[test]
fn deflate_codec_matches_the_plain_entry_points() {
    let input = data();
    let plain = crush_parallel::compress(&input, &config(2)).expect("compress");
    let generic = compress_with(&DeflateCodec::default(), &input, &config(2)).expect("compress");
    assert_eq!(plain[16..], generic[16..]);
}
