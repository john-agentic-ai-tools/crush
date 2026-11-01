# Contract: CLI Behavior

No new flags. Sprint uses the existing plugin interface.

| Command | Behavior |
|---|---|
| `crush compress --plugin sprint <file>` | Compresses with sprint. Also works with piped input. |
| `crush decompress <file>` | Detects sprint from the file's magic bytes. |
| `crush decompress --block N <file>` | Returns one block. Works for every CRSH codec, and is no longer limited to `parallel-deflate`. |
| `crush plugins list` | Lists `sprint`. |
| `crush inspect <file>` | Reports plugin `sprint`, sizes and block count. |
| `crush compress <file>` with no plugin named | Unchanged until promotion. |

## Errors

| Condition | Result |
|---|---|
| Corrupt or forged file | Non-zero exit, clear error, no partial output left behind |
| Older Crush reading a sprint file | Non-zero exit, unknown plugin error |
| Newer unknown `sprint_version` | Non-zero exit, unsupported version error |
| `--block N` out of range | Non-zero exit, error naming the valid range |
| Ctrl-C | Stops promptly, removes partial output, existing cancel exit status |

## Promotion (User Story 4)

After all gates pass, files of 25 MiB or more and piped input without `--plugin` route to sprint. Decompression of older files does not change.
