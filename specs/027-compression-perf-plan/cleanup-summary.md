# Cleanup Summary

- `detect-duplicates.ps1 -Json -Path crush-lz/src`: 0 clones.
- `detect-duplicates.ps1 -Json -Path crush-gpu/src`: 0 clones.
- Replaced `push_block` in `crush-lz/src/compress.rs` with `encode_block`, shared by the serial and parallel paths.
- Moved per-tile work in `crush-gpu/src/engine.rs` into `encode_tile`.
