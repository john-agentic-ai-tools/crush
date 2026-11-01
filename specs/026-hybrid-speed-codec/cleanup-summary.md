# Cleanup Summary

## User Story 1

- `detect-duplicates.ps1 -Json -Path crush-parallel/src`: 0 clones.
- Moved DEFLATE specifics and the incompressible probe from `crush-parallel/src/block.rs` to `crush-parallel/src/codec.rs`.
- Merged the duplicated plugin configuration in `crush-parallel/src/lib.rs` into `plugin_config`.
- Removed `compress_block`, `decompress_block_payload` and the public `block` module, which nothing outside the crate used.
