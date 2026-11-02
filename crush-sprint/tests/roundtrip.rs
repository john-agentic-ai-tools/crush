//! T032: whole blocks and files round trip, and output is deterministic.
#![allow(clippy::expect_used, clippy::cast_possible_truncation)]
// Plain `assert!(a == b)`: `assert_eq!` would print megabytes on failure.
#![allow(clippy::manual_assert_eq)]

use crush_parallel::EngineConfiguration;
use crush_parallel::codec::BlockCodec;
use crush_parallel::engine::{compress_with, decompress_with};
use crush_sprint::SprintCodec;
use crush_sprint::format::BLOCK_SIZE;
use proptest::prelude::*;

fn block(codec: SprintCodec, data: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = codec.encoder().expect("encoder");
    let mut out = Vec::new();
    codec
        .compress_block(&mut encoder, data, &mut out)
        .expect("compress")
        .then_some(out)
}

/// Round trips through one block; a block the codec stores has nothing to decode.
fn check(data: &[u8]) {
    let codec = SprintCodec;
    let Some(payload) = block(codec, data) else {
        return;
    };
    let mut decoder = codec.decoder().expect("decoder");
    let mut output = vec![0xAA; data.len()];
    codec
        .decompress_block(&mut decoder, &payload, &mut output)
        .expect("decompress");
    assert!(output == data, "{} bytes differ", data.len());
}

fn xorshift(len: usize) -> Vec<u8> {
    let mut x = 0x9E37_79B9_7F4A_7C15_u64;
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(len);
    out
}

/// Text, random and zero regions, crossing segment and block boundaries.
fn mixed(len: usize) -> Vec<u8> {
    let line = b"2026-11-01 18:00:01 INFO served path=/api/v1/items status=200\n";
    let mut out: Vec<u8> = line.iter().copied().cycle().take(len / 3).collect();
    out.extend_from_slice(&xorshift(len / 3));
    out.resize(len, 0);
    out
}

#[test]
fn empty_and_tiny_blocks() {
    for len in 0..=32 {
        check(&vec![b'a'; len]);
        check(&xorshift(len));
    }
}

#[test]
fn sizes_around_a_segment_edge() {
    for len in [131_071, 131_072, 131_073, 262_144, 262_145] {
        check(&mixed(len));
    }
}

#[test]
fn a_full_block_of_each_kind() {
    check(&vec![0u8; BLOCK_SIZE]);
    check(&mixed(BLOCK_SIZE));
    check(&xorshift(BLOCK_SIZE));
}

/// Offsets reach the whole block, past a 2 `MiB` window.
#[test]
fn matches_span_the_block() {
    let head = xorshift(64 * 1024);
    let mut data = head.clone();
    data.extend_from_slice(
        &xorshift(3 << 20)
            .iter()
            .map(|b| b ^ 0x5A)
            .collect::<Vec<_>>(),
    );
    data.extend_from_slice(&head);
    let codec = SprintCodec;
    let payload = block(codec, &data).expect("compressed");
    assert!(
        payload.len() < data.len() - 32 * 1024,
        "the far copy must match"
    );
    check(&data);
}

#[test]
fn repetitive_data_compresses_hard_and_random_stays_near_size() {
    let codec = SprintCodec;
    let zeros = vec![0u8; BLOCK_SIZE];
    assert!(block(codec, &zeros).expect("compressed").len() < BLOCK_SIZE / 1000);
    if let Some(p) = block(codec, &xorshift(BLOCK_SIZE)) {
        assert!(p.len() <= BLOCK_SIZE + BLOCK_SIZE / 512);
    }
}

#[test]
fn engine_round_trip_with_all_thread_counts() {
    let data = mixed(2 * BLOCK_SIZE + 12_345);
    let codec = SprintCodec;
    let mut first: Option<Vec<u8>> = None;
    for workers in [1, 2, 4] {
        let config = EngineConfiguration::builder()
            .block_size(BLOCK_SIZE as u32)
            .workers(workers)
            .build()
            .expect("config");
        let packed = compress_with(&codec, &data, &config).expect("compress");
        assert!(decompress_with(&codec, &packed, &config).expect("decompress") == data);
        match &first {
            None => first = Some(packed),
            Some(bytes) => assert!(*bytes == packed, "output depends on {workers} workers"),
        }
    }
}

/// A reused encoder must not remember the block before.
#[test]
fn output_does_not_depend_on_earlier_blocks() {
    let codec = SprintCodec;
    let (a, b) = (mixed(1 << 20), xorshift(1 << 20));
    let fresh = block(codec, &a);
    let mut encoder = codec.encoder().expect("encoder");
    let mut out = Vec::new();
    codec
        .compress_block(&mut encoder, &b, &mut out)
        .expect("first");
    let kept = codec
        .compress_block(&mut encoder, &a, &mut out)
        .expect("second")
        .then_some(out);
    assert!(fresh == kept);
}

#[test]
fn a_reused_output_buffer_is_replaced() {
    let codec = SprintCodec;
    let data = mixed(1 << 18);
    let mut encoder = codec.encoder().expect("encoder");
    let mut out = vec![0xFF; 1 << 20];
    assert!(
        codec
            .compress_block(&mut encoder, &data, &mut out)
            .expect("compress")
    );
    assert!(Some(out) == block(codec, &data));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn random_bytes(data in prop::collection::vec(any::<u8>(), 0..20_000)) {
        check(&data);
    }

    #[test]
    fn small_alphabets(data in prop::collection::vec(0u8..4, 0..20_000)) {
        check(&data);
    }

    #[test]
    fn repeated_runs(runs in prop::collection::vec((any::<u8>(), 1usize..5000), 1..50)) {
        let data: Vec<u8> = runs.iter().flat_map(|&(b, n)| std::iter::repeat_n(b, n)).collect();
        check(&data);
    }

    #[test]
    fn repeated_phrases(
        phrase in prop::collection::vec(any::<u8>(), 1..300),
        total in 0usize..200_000,
    ) {
        let data: Vec<u8> = phrase.iter().copied().cycle().take(total).collect();
        check(&data);
    }
}
