#![no_main]
//! Any input must round trip exactly through one block. Inputs repeat a
//! little to reach matches and sequence tables, and a reused encoder and
//! decoder must behave like fresh ones.
use crush_parallel::codec::BlockCodec;
use crush_sprint::SprintCodec;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&repeat, body)) = data.split_first() else {
        return;
    };
    let source: Vec<u8> = body
        .iter()
        .copied()
        .cycle()
        .take(body.len() * (usize::from(repeat) % 8 + 1))
        .collect();
    let codec = SprintCodec;
    let mut encoder = codec.encoder().expect("encoder");
    let mut decoder = codec.decoder().expect("decoder");
    for _ in 0..2 {
        let mut payload = Vec::new();
        if codec
            .compress_block(&mut encoder, &source, &mut payload)
            .expect("compress")
        {
            let mut output = vec![0u8; source.len()];
            codec
                .decompress_block(&mut decoder, &payload, &mut output)
                .expect("decompress");
            assert_eq!(output, source);
        }
    }
});
