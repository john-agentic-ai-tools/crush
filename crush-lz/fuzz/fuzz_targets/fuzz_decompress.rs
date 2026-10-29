#![no_main]
//! Arbitrary bytes into the fast-lz decoder: it must return Ok or Err, never
//! panic, hang, or allocate beyond what the payload's blocks declare.
//!
//! Byte 0 picks the mode: even = prepend a valid frame header (64-kilobyte
//! blocks) so the fuzzer spends its time in block and sequence decoding;
//! odd = feed the bytes raw, frame header included.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 5 {
        return;
    }
    let declared = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
    let expected = declared.min(16 << 20);
    let body = &data[5..];
    if data[0] % 2 == 0 {
        let mut payload = crush_lz::format::FrameHeader::new(16)
            .expect("valid header")
            .to_bytes()
            .to_vec();
        payload.extend_from_slice(body);
        let _ = crush_lz::decompress(&payload, expected);
    } else {
        let _ = crush_lz::decompress(body, expected);
    }
});
