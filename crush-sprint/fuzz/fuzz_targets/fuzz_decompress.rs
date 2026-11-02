#![no_main]
//! Bytes into the sprint block decoder: it must fill the output or return an
//! error, and never panic or hang.
//!
//! Byte 0 picks the mode. Even: the bytes follow a valid version byte, so
//! the fuzzer spends its time in segments. Odd: a valid block is compressed
//! from the bytes and then damaged by them, which reaches Huffman and tANS
//! tables an empty corpus would take far longer to build.
use crush_parallel::codec::BlockCodec;
use crush_sprint::SprintCodec;
use crush_sprint::format::SPRINT_VERSION;
use libfuzzer_sys::fuzz_target;

const MAX_OUTPUT: usize = 1 << 20;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let declared = usize::from(u16::from_le_bytes([data[1], data[2]])) * 16;
    let body = &data[3..];
    let codec = SprintCodec;
    let mut decoder = codec.decoder().expect("decoder");
    if data[0] % 2 == 0 {
        let mut payload = vec![SPRINT_VERSION];
        payload.extend_from_slice(body);
        let mut output = vec![0u8; declared.min(MAX_OUTPUT)];
        let _ = codec.decompress_block(&mut decoder, &payload, &mut output);
    } else {
        let (seed, damage) = body.split_at(body.len() / 2);
        let source: Vec<u8> = seed.iter().copied().cycle().take(seed.len() * 16).collect();
        let mut encoder = codec.encoder().expect("encoder");
        let mut payload = Vec::new();
        if codec
            .compress_block(&mut encoder, &source, &mut payload)
            .expect("compress")
        {
            for pair in damage.chunks_exact(2) {
                let at = usize::from(pair[0]) * payload.len() / 256;
                payload[at] ^= pair[1];
            }
            let mut output = vec![0u8; source.len()];
            let _ = codec.decompress_block(&mut decoder, &payload, &mut output);
        }
    }
});
