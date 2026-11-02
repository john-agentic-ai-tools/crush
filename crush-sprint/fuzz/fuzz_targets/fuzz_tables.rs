#![no_main]
//! Table descriptions and code lengths from raw bytes: rejected, or usable
//! without a panic on any bit stream.
use crush_sprint::bits::BitReader;
use crush_sprint::huffman::HuffmanDecoder;
use crush_sprint::tans::{DecodeTable, MAX_SYMBOLS, read_counts};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut counts = [0i16; MAX_SYMBOLS];
    if let Ok((log, used, rest)) = read_counts(data, MAX_SYMBOLS, &mut counts) {
        if let Ok(table) = DecodeTable::new(&counts[..used], log, 9) {
            if let Ok(mut reader) = BitReader::new(rest) {
                let _ = table.decode(&mut reader, 1000);
            }
        }
    }

    let (lengths, streams) = data.split_at(data.len().min(256));
    if let Ok(decoder) = HuffmanDecoder::from_lengths(lengths) {
        let mut output = vec![0u8; 1024];
        let _ = decoder.decode_4_streams(streams, &mut output);
    }
});
