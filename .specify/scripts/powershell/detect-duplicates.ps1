#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Detects code duplications using jscpd for post-MVP cleanup analysis.

.DESCRIPTION
    Runs jscpd (JavaScript Copy/Paste Detector) to identify duplicate code blocks
    across the codebase. Outputs results in JSON format for automated processing
    or human-readable console format.

.PARAMETER Json
    Output results in JSON format for scripting and automation.

.PARAMETER Path
    Target path to analyze (defaults to repository root).

.PARAMETER MinLines
    Minimum number of duplicate lines to report (defaults to 20).

.PARAMETER ReportPath
    Directory to save the full jscpd report (defaults to .jscpd-report).

.EXAMPLE
    .\detect-duplicates.ps1
    Run duplication detection with default settings and console output.

.EXAMPLE
    .\detect-duplicates.ps1 -Json
    Run detection and output results as JSON for automation.

.EXAMPLE
    .\detect-duplicates.ps1 -Path "crush-cli" -MinLines 15
    Analyze only the crush-cli directory with 15-line minimum threshold.

.NOTES
    Requires: Node.js and npm (jscpd will be run via npx if not globally installed)
    Constitution Reference: Section "MVP Delivery Workflow - Post-MVP Cleanup Phase"
#>

param(
    [switch]$Json,
    [string]$Path = ".",
    [int]$MinLines = 20,
    [string]$ReportPath = ".jscpd-report"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Resolve paths relative to repository root
$scriptDir = Split-Path -Parent $PSCommandPath
$repoRoot = Resolve-Path (Join-Path $scriptDir "../../..")
$targetPath = if ($Path -eq ".") { $repoRoot } else { Resolve-Path (Join-Path $repoRoot $Path) }
$configPath = Join-Path $repoRoot ".jscpd.json"
$reportDir = Join-Path $repoRoot $ReportPath

# Ensure config file exists
if (-not (Test-Path $configPath)) {
    $errorMsg = "jscpd configuration not found at $configPath"
    if ($Json) {
        @{
            success = $false
            error = $errorMsg
        } | ConvertTo-Json -Depth 10
    } else {
        Write-Error $errorMsg
    }
    exit 1
}

# Update config with runtime parameters
$config = Get-Content $configPath | ConvertFrom-Json
$config.minLines = $MinLines
$config.output = $ReportPath
$config | ConvertTo-Json -Depth 10 | Set-Content $configPath

# Check if jscpd is available
$jscpdAvailable = $null -ne (Get-Command jscpd -ErrorAction SilentlyContinue)
$useNpx = -not $jscpdAvailable

if ($useNpx) {
    Write-Verbose "jscpd not found globally, using npx"
}

# Build command
$jscpdCmd = if ($useNpx) { "npx" } else { "jscpd" }
$jscpdArgs = @(
    if ($useNpx) { "jscpd" }
    $targetPath
    "--config", $configPath
)

# Run jscpd
Write-Verbose "Running: $jscpdCmd $($jscpdArgs -join ' ')"

# Run jscpd and suppress all output (we'll read the JSON report instead)
# Note: jscpd may return exit code 1 when duplicates are found (this is expected)
# Use cmd.exe to run npx to avoid PowerShell stderr handling issues
if ($useNpx) {
    $cmdArgs = @("/c", "npx", "jscpd", $targetPath, "--config", $configPath, ">", "nul", "2>&1")
    & cmd.exe $cmdArgs
} else {
    $jscpdArgs = @($targetPath, "--config", $configPath)
    & jscpd $jscpdArgs >$null 2>&1
}

# Give jscpd a moment to write the report
Start-Sleep -Milliseconds 1000

# Parse results
$jsonReportPath = Join-Path $reportDir "jscpd-report.json"
if (-not (Test-Path $jsonReportPath)) {
    if ($Json) {
        @{
            success = $true
            duplicates_found = 0
            total_lines = 0
            files_affected = 0
            message = "No duplications detected"
        } | ConvertTo-Json -Depth 10
    } else {
        Write-Host "[OK] No code duplications found (threshold: $MinLines lines)" -ForegroundColor Green
    }
    exit 0
}

$report = Get-Content $jsonReportPath | ConvertFrom-Json

# Calculate summary statistics from the report
$totalDuplicates = $report.statistics.total.clones
$totalLines = $report.statistics.total.duplicatedLines
$duplicateCount = ($report.duplicates | Measure-Object).Count

# Output results
if ($Json) {
    @{
        success = $true
        clones_found = $totalDuplicates
        total_duplicate_lines = $totalLines
        duplicate_instances = $duplicateCount
        report_path = $jsonReportPath
        duplicates = $report.duplicates | ForEach-Object {
            @{
                format = $_.format
                lines = $_.lines
                tokens = $_.tokens
                file1 = $_.firstFile.name
                file2 = $_.secondFile.name
                start1 = $_.firstFile.start
                end1 = $_.firstFile.end
                start2 = $_.secondFile.start
                end2 = $_.secondFile.end
            }
        }
    } | ConvertTo-Json -Depth 10
} else {
    Write-Host ""
    Write-Host "===============================================================" -ForegroundColor Cyan
    Write-Host "  Code Duplication Detection Report" -ForegroundColor Cyan
    Write-Host "===============================================================" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "  Clone Groups Found:       $totalDuplicates" -ForegroundColor Yellow
    Write-Host "  Total Duplicated Lines:   $totalLines" -ForegroundColor Yellow
    Write-Host "  Duplicate Instances:      $duplicateCount" -ForegroundColor Yellow
    Write-Host "  Minimum Threshold:        $MinLines lines" -ForegroundColor Gray
    Write-Host ""
    Write-Host "===============================================================" -ForegroundColor Cyan
    Write-Host ""

    if ($totalDuplicates -gt 0) {
        Write-Host "Top Duplications:" -ForegroundColor White
        Write-Host ""

        $report.duplicates | Sort-Object -Property lines -Descending | Select-Object -First 5 | ForEach-Object {
            $file1 = Split-Path -Leaf $_.firstFile.name
            $file2 = Split-Path -Leaf $_.secondFile.name
            Write-Host "  * $file1 : $($_.firstFile.start) <-> $file2 : $($_.secondFile.start)" -ForegroundColor White
            Write-Host "    Lines: $($_.lines) | Tokens: $($_.tokens)" -ForegroundColor Gray
            Write-Host ""
        }

        Write-Host "Full report: $jsonReportPath" -ForegroundColor Gray
        Write-Host ""
        Write-Host "[WARNING] Cleanup required. Run refactoring to eliminate duplicates." -ForegroundColor Yellow
    }
}

# Exit with appropriate code
if ($totalDuplicates -gt 0) {
    exit 1
} else {
    exit 0
}
