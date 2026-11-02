#![no_main]
//! Tables from raw bytes: rejected, or usable without a panic on any stream.
//!
//! Raw bytes rarely form a valid table, so most of each run builds one from
//! a histogram, writes its description and damages it, which reaches the
//! table builders and decoders.
use crush_sprint::bits::BitReader;
use crush_sprint::huffman::{HuffmanDecoder, build_code_lengths};
use crush_sprint::tans::{
    DecodeTable, MAX_SYMBOLS, normalize_counts, read_counts, write_counts,
};
use libfuzzer_sys::fuzz_target;

fn use_description(description: &[u8], stream: &[u8]) {
    let mut counts = [0i16; MAX_SYMBOLS];
    if let Ok((log, used, _)) = read_counts(description, MAX_SYMBOLS, &mut counts) {
        if let Ok(table) = DecodeTable::new(&counts[..used], log, 9) {
            if let Ok(mut reader) = BitReader::new(stream) {
                let _ = table.decode(&mut reader, 1000);
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    use_description(data, data);
    let (seed, rest) = data.split_at(data.len() / 2);

    let mut histogram = [0u32; MAX_SYMBOLS];
    for &byte in seed {
        histogram[usize::from(byte) % MAX_SYMBOLS] += 1;
    }
    let log = 5 + rest.first().copied().unwrap_or(0) % 5;
    if let Ok(normalized) = normalize_counts(&histogram, log) {
        let mut description = Vec::new();
        if write_counts(&normalized, log, &mut description).is_ok() {
            use_description(&description, rest);
            for pair in rest.chunks_exact(2) {
                let at = usize::from(pair[0]) * description.len() / 256;
                description[at] ^= pair[1];
            }
            use_description(&description, rest);
        }
    }

    let mut bytes = [0u32; 256];
    for &byte in seed {
        bytes[usize::from(byte)] += 1;
    }
    if let Ok(mut lengths) = build_code_lengths(&bytes) {
        if let Ok(decoder) = HuffmanDecoder::from_lengths(&lengths) {
            let mut output = vec![0u8; 1024];
            let _ = decoder.decode_4_streams(rest, &mut output);
        }
        for pair in rest.chunks_exact(2) {
            lengths[usize::from(pair[0])] ^= pair[1];
        }
        let _ = HuffmanDecoder::from_lengths(&lengths);
    }
});
