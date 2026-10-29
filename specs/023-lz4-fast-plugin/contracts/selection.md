# Contract: content-aware plugin selection (crush-core)

## Plugin trait additions (`crush_core::plugin::CompressionAlgorithm`)

Both are provided methods with defaults, so existing plugins (`deflate`, `parallel-deflate`, `gpu-deflate`) and third-party plugins compile and behave unchanged.

```rust
/// True if automatic selection may use this plugin only after
/// `estimate_ratio` shows the input compresses well with it.
fn content_aware(&self) -> bool { false }

/// Cheap estimate of input_len / compressed_len for `input`, from samples.
/// `None` = no estimate available (plugin is then not auto-selected).
/// Must not modify state visible to other calls; must cost ≲ 2% of
/// compressing `input` with this plugin.
fn estimate_ratio(&self, input: &[u8]) -> Option<f64> { None }
```

## Selector behaviour (`PluginSelector::select_for_input(&self, input: &[u8]) -> Result<PluginMetadata>`)

| Weights | Content-aware plugin | Result |
|---|---|---|
| `throughput < 0.8` (balanced, best, default) | any | excluded; existing scoring over the other plugins (unchanged choice) |
| `throughput ≥ 0.8` (fast) | estimate ≥ 1.10 | chosen (highest estimate if several) |
| `throughput ≥ 0.8` | estimate < 1.10 or `None` | excluded; existing scoring over the other plugins |

- `PluginSelector::select()` (no input) keeps its current signature and excludes content-aware plugins entirely.
- `select_by_name` is unchanged: naming `fast-lz` always uses it (FR-009).
- `compress_with_options` calls `select_for_input(input)` when no plugin name is set.

## CLI (`crush-cli`)

- `crush plugins list` shows `fast-lz` (FR-001).
- `crush compress --plugin fast-lz FILE` uses it at any size.
- `crush compress --level fast FILE` (no plugin) on inputs < 25 MB: crush-core's content-aware selection decides; ≥ 25 MB or stdin: unchanged (`parallel-deflate` / `gpu-deflate`), see research R7.
- `crush decompress` / `crush inspect` recognise `fast-lz` files by the outer magic with no flags (FR-003, FR-011).
