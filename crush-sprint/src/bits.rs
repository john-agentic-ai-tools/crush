//! Backward bit streams.
//!
//! A writer packs fields from the low bits up and ends with a sentinel bit.
//! A reader starts at the final byte and walks toward the first, so the last
//! field written is the first read. Reads load fixed-size words; a read past
//! the start returns zeros and is reported by [`BitReader::overflowed`], so
//! hot loops need no per-read check.

use crush_core::error::{Result, ValidationError};

/// The most bits one [`BitReader::read`] or [`BitWriter::write`] can carry.
pub const MAX_FIELD_BITS: u32 = 57;

/// Writes a backward bit stream.
#[derive(Debug, Default)]
pub struct BitWriter {
    buf: Vec<u8>,
    pos: usize,
    bits: u64,
    nbits: u32,
}

impl BitWriter {
    /// An empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(64)
    }

    /// An empty writer that expects about `bytes` of output.
    #[must_use]
    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            buf: vec![0; bytes + 16],
            pos: 0,
            bits: 0,
            nbits: 0,
        }
    }

    /// Queue a field without flushing. `value` must fit in `n` bits, and the
    /// bits queued since the last [`Self::flush`] must fit in 57.
    #[inline(always)]
    pub fn add(&mut self, value: u64, n: u32) {
        self.bits |= value << self.nbits;
        self.nbits += n;
    }

    /// Move whole queued bytes to the buffer.
    #[inline(always)]
    pub fn flush(&mut self) {
        if self.pos + 8 > self.buf.len() {
            self.buf.resize(self.buf.len() * 2 + 16, 0);
        }
        self.buf[self.pos..self.pos + 8].copy_from_slice(&self.bits.to_le_bytes());
        let bytes = self.nbits >> 3;
        self.pos += bytes as usize;
        self.nbits &= 7;
        self.bits = self.bits.checked_shr(bytes * 8).unwrap_or(0);
    }

    /// Write one field of `n` bits, at most [`MAX_FIELD_BITS`].
    #[inline]
    pub fn write(&mut self, value: u64, n: u32) {
        self.add(value, n);
        self.flush();
    }

    /// Add the sentinel and return the stream.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        self.write(1, 1);
        let len = self.pos + usize::from(self.nbits > 0);
        self.buf.truncate(len);
        self.buf
    }
}

/// Reads a backward bit stream.
#[derive(Debug)]
pub struct BitReader<'a> {
    data: &'a [u8],
    /// End of the loaded window, in bytes.
    window_end: usize,
    /// The eight bytes before `window_end`, newest bits on top.
    container: u64,
    /// Bits of the container already used.
    consumed: u32,
}

impl<'a> BitReader<'a> {
    /// Start at the sentinel in the final byte.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if `data` is empty or its final byte has no sentinel.
    pub fn new(data: &'a [u8]) -> Result<Self> {
        let last = data.last().copied().unwrap_or(0);
        if last == 0 {
            return Err(ValidationError::CorruptedData("bit stream has no sentinel".into()).into());
        }
        let mut reader = Self {
            data,
            window_end: data.len(),
            container: 0,
            consumed: last.leading_zeros() + 1,
        };
        reader.load();
        Ok(reader)
    }

    /// Load the window, padding below byte 0 with zeros.
    fn load(&mut self) {
        let end = self.window_end;
        let mut word = [0u8; 8];
        if end >= 8 {
            word.copy_from_slice(&self.data[end - 8..end]);
        } else {
            word[8 - end..].copy_from_slice(&self.data[..end]);
        }
        self.container = u64::from_le_bytes(word);
    }

    /// Top up the container so that at least 57 bits can be peeked.
    #[inline(always)]
    pub fn refill(&mut self) {
        let bytes = (self.consumed >> 3) as usize;
        match self.window_end.checked_sub(bytes + 8) {
            Some(start) => {
                self.window_end -= bytes;
                self.consumed &= 7;
                let word = self.data.get(start..start + 8);
                self.container = word
                    .and_then(|w| <[u8; 8]>::try_from(w).ok())
                    .map_or(0, u64::from_le_bytes);
            }
            None => self.refill_near_start(bytes),
        }
    }

    #[cold]
    fn refill_near_start(&mut self, bytes: usize) {
        let step = bytes.min(self.window_end);
        self.window_end -= step;
        self.consumed -= (step * 8) as u32;
        self.load();
    }

    /// The next 11 bits, without consuming them.
    #[inline(always)]
    #[must_use]
    pub fn peek_11(&self) -> usize {
        (self.container.wrapping_shl(self.consumed) >> 53) as usize
    }

    /// Mark `n` bits as used.
    #[inline(always)]
    pub fn consume(&mut self, n: u32) {
        self.consumed += n;
    }

    /// The unread bits, oldest on top, as of the last refill. Pair with
    /// [`Self::consume`] to pull several fields out with one shift each.
    #[inline(always)]
    #[must_use]
    pub fn window(&self) -> u64 {
        self.container.wrapping_shl(self.consumed)
    }

    /// Read a field of `n` bits with no refill. Fields since the last
    /// [`Self::refill`] must total at most 57 bits.
    #[inline(always)]
    pub fn take(&mut self, n: u32) -> u64 {
        let value = (self.container.wrapping_shl(self.consumed) >> 1) >> (63 - n);
        self.consumed += n;
        value
    }

    /// Read a field of `n` bits, at most [`MAX_FIELD_BITS`].
    #[inline]
    pub fn read(&mut self, n: u32) -> u64 {
        self.refill();
        let value = (self.container.wrapping_shl(self.consumed) >> 1) >> (63 - n);
        self.consumed += n;
        value
    }

    /// Unread bits. Negative once a read has passed the start of the stream.
    #[inline]
    #[must_use]
    pub fn bits_left(&self) -> i64 {
        self.window_end as i64 * 8 - i64::from(self.consumed)
    }

    /// True when every bit was read, and no more.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.bits_left() == 0
    }

    /// True when a read passed the start of the stream.
    #[must_use]
    pub fn overflowed(&self) -> bool {
        self.bits_left() < 0
    }
}
