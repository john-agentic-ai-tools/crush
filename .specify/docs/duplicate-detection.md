# Automated Duplicate Code Detection

## Overview

The Crush project uses [jscpd](https://jscpd.dev/) (Copy/Paste Detector) for automated duplicate code detection as part of the mandatory Post-MVP Cleanup Phase defined in the constitution.

## Quick Start

```powershell
# Run detection with default settings (20-line minimum)
.specify/scripts/powershell/detect-duplicates.ps1

# Get JSON output for automation
.specify/scripts/powershell/detect-duplicates.ps1 -Json

# Analyze specific directory
.specify/scripts/powershell/detect-duplicates.ps1 -Path "crush-core"

# Adjust minimum threshold
.specify/scripts/powershell/detect-duplicates.ps1 -MinLines 15
```

## Installation

The script uses `npx` to run jscpd, so no manual installation is required. Prerequisites:
- Node.js and npm installed
- Internet connection (first run only)

To install jscpd globally for faster execution:
```bash
npm install -g jscpd
```

## Configuration

Duplication detection settings are in `.jscpd.json` at the repository root:

```json
{
  "threshold": 0,
  "reporters": ["json", "console"],
  "format": ["rust"],
  "ignore": [
    "**/target/**",
    "**/node_modules/**",
    "**/.git/**",
    "**/specs/**",
    "**/.specify/**"
  ],
  "absolute": true,
  "minLines": 20,
  "minTokens": 50,
  "output": ".jscpd-report",
  "exitCode": 1
}
```

### Key Settings

- **minLines**: Minimum number of duplicate lines to report (default: 20)
- **minTokens**: Minimum number of duplicate tokens to report (default: 50)
- **ignore**: Directories and files to exclude from analysis
- **exitCode**: Exit code when duplicates are found (1 = duplicates found)

## Workflow Integration

### Constitution Requirements

Per Constitution v1.6.0, Section "MVP Delivery Workflow - Post-MVP Cleanup Phase":

1. **Run automated scan**:
   ```powershell
   .specify/scripts/powershell/detect-duplicates.ps1 -Json > duplication-report.json
   ```

2. **Review output**: Examine JSON for duplicates > 20 lines

3. **Prioritize targets**: Focus on highest-impact duplications (by line count)

4. **Extract utilities**: Create shared modules for identified patterns

5. **Verify changes**: Run tests and clippy after each extraction

6. **Document results**: Record findings in `specs/[feature]/cleanup-summary.md`

### Output Interpretation

**Console Output**:
```
===============================================================
  Code Duplication Detection Report
===============================================================

  Clone Groups Found:       6
  Total Duplicated Lines:   185
  Duplicate Instances:      6
  Minimum Threshold:        20 lines

===============================================================

Top Duplications:

  * cli_startup.rs : 1 <-> help_command.rs : 1
    Lines: 43 | Tokens: 0
  ...
```

**JSON Output**:
```json
{
  "success": true,
  "clones_found": 6,
  "total_duplicate_lines": 185,
  "duplicate_instances": 6,
  "report_path": "C:/Users/Admin/code/crush/.jscpd-report/jscpd-report.json",
  "duplicates": [
    {
      "format": "rust",
      "lines": 43,
      "file1": "crush-cli/benches/cli_startup.rs",
      "file2": "crush-cli/benches/help_command.rs",
      "start1": 1,
      "end1": 43,
      "start2": 1,
      "end2": 43
    }
  ]
}
```

### Exit Codes

- **0**: No duplications found (success)
- **1**: Duplications found (expected during cleanup phase)
- **> 1**: Error occurred during analysis

## CI Integration

To enforce duplication limits in CI:

```yaml
# Example GitHub Actions workflow
- name: Check for code duplication
  run: |
    .specify/scripts/powershell/detect-duplicates.ps1
  continue-on-error: true  # Don't fail build, just warn

# Or fail build if duplicates exceed threshold
- name: Enforce duplication limit
  run: |
    $result = .specify/scripts/powershell/detect-duplicates.ps1 -Json | ConvertFrom-Json
    if ($result.total_duplicate_lines -gt 200) {
      Write-Error "Duplicate code exceeds threshold: $($result.total_duplicate_lines) lines"
      exit 1
    }
```

## Cleanup Quality Gates

Per the constitution, cleanup must achieve:
- ✓ No code duplication > 20 lines between modules
- ✓ All extracted utilities have comprehensive tests
- ✓ Clippy clean after refactoring
- ✓ All tests pass after cleanup
- ✓ No regression in performance benchmarks

## Troubleshooting

**Issue**: Script fails with "jscpd not found"
- **Solution**: Ensure Node.js and npm are installed. Script will auto-install via npx.

**Issue**: Many false positives
- **Solution**: Increase `minLines` or `minTokens` in `.jscpd.json`

**Issue**: Missing legitimate duplicates
- **Solution**: Decrease thresholds or check `ignore` patterns

**Issue**: Slow execution
- **Solution**: Install jscpd globally: `npm install -g jscpd`

## References

- [jscpd Documentation](https://jscpd.dev/)
- [jscpd GitHub Repository](https://github.com/kucherenko/jscpd)
- [Crush Constitution v1.6.0](../.specify/memory/constitution.md)
- [Post-MVP Cleanup Phase Guidelines](../.specify/memory/constitution.md#post-mvp-cleanup-phase-mandatory)

## Example: Feature 006 Cleanup

During Feature 006 (Ctrl+C cancellation), the tool identified:
- 6 clone groups
- 185 duplicated lines
- Key targets: compress.rs ↔ decompress.rs utilities

Cleanup actions taken:
- Created `commands/utils.rs` module
- Extracted 8 shared functions
- Eliminated 210 lines of duplicated code
- All tests passing, clippy clean

See `specs/006-cancel-via-ctrl-c/cleanup-summary.md` for detailed results.
