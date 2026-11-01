# Contract: BlockCodec

The trait that lets the parallel engine run any codec. It lives in `crush-parallel/src/codec.rs`. Names may be refined in implementation, but the guarantees below are binding.

```rust
pub trait BlockCodec: Sync {
    /// Value stored in the container header. 0 is reserved for DEFLATE.
    const CODEC_ID: u8;
    /// Per-worker compression state, reused across blocks.
    type Encoder: Send;
    /// Per-worker decompression state, reused across blocks.
    type Decoder: Send;

    fn encoder(&self) -> Result<Self::Encoder>;
    fn decoder(&self) -> Result<Self::Decoder>;

    /// Compress one block into `out`, replacing its contents.
    /// Returns Ok(false) when the block should be stored raw.
    fn compress_block(
        &self,
        encoder: &mut Self::Encoder,
        input: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<bool>;

    /// Decode one block into `output`, whose length is the block's
    /// validated uncompressed size.
    fn decompress_block(
        &self,
        decoder: &mut Self::Decoder,
        payload: &[u8],
        output: &mut [u8],
    ) -> Result<()>;
}
```

## Guarantees from implementations

- MUST NOT panic on any payload. All failures return `Err`.
- `decompress_block` MUST fill `output` exactly, or return an error.
- MUST NOT read or depend on any other block.
- `compress_block` MUST use only the state held in `encoder`, and its output MUST NOT depend on which worker runs it or on earlier blocks.
- Identical input and configuration MUST produce identical output across runs and worker counts.

## Guarantees from the engine

- Validates block sizes against the index before calling a codec, and never allocates from an unvalidated size.
- Owns CRCs, the stored-block fallback, the index, ordering, progress and cancellation.
- Checks for cancellation before each block.
- Uses static dispatch, so the hot path has no virtual call.

## Compatibility

`DeflateCodec` (`CODEC_ID = 0`) MUST produce the same bytes as before the refactor, in CRSH format version 1. A golden-file test enforces this.
