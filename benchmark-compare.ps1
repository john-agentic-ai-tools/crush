# Benchmark comparison: crush vs gzip
# Compares compression speed, decompression speed, and compression ratio

param(
    [int]$SizeMB = 100
)

# Check dependencies
function Test-Dependencies {
    if (-not (Test-Path "target\release\crush.exe")) {
        Write-Host "Error: crush.exe not found" -ForegroundColor Red
        Write-Host "Build with: cargo build --release"
        exit 1
    }

    # Check for gzip (via Git Bash or WSL)
    $gzipFound = $false
    $paths = @(
        "C:\Program Files\Git\usr\bin\gzip.exe",
        "C:\Program Files (x86)\Git\usr\bin\gzip.exe"
    )

    foreach ($path in $paths) {
        if (Test-Path $path) {
            $script:GzipPath = $path
            $gzipFound = $true
            break
        }
    }

    if (-not $gzipFound) {
        Write-Host "Warning: gzip not found (install Git for Windows)" -ForegroundColor Yellow
        Write-Host ""
    }
}

# Generate test data
function New-TestData {
    param(
        [int]$SizeMB,
        [string]$Output,
        [string]$Type
    )

    Write-Host "Generating ${SizeMB}MB $Type test data..." -ForegroundColor Blue

    $bytes = $SizeMB * 1024 * 1024

    switch ($Type) {
        "text" {
            # Compressible text data
            $content = "The quick brown fox jumps over the lazy dog. " * 100
            $sb = [System.Text.StringBuilder]::new($bytes)
            while ($sb.Length -lt $bytes) {
                [void]$sb.Append($content)
            }
            [System.IO.File]::WriteAllText($Output, $sb.ToString().Substring(0, $bytes))
        }
        "binary" {
            # Random binary data
            $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
            $buffer = New-Object byte[] $bytes
            $rng.GetBytes($buffer)
            [System.IO.File]::WriteAllBytes($Output, $buffer)
        }
        "repetitive" {
            # Highly compressible repetitive data
            $pattern = "The quick brown fox jumps over the lazy dog. "
            $sb = [System.Text.StringBuilder]::new($bytes)
            while ($sb.Length -lt $bytes) {
                [void]$sb.Append($pattern)
            }
            [System.IO.File]::WriteAllText($Output, $sb.ToString().Substring(0, $bytes))
        }
    }
}

# Benchmark compression
function Measure-Compression {
    param(
        [string]$Tool,
        [string]$Input,
        [string]$Output,
        [string]$Label
    )

    Write-Host "Testing $Label compression..." -ForegroundColor Blue

    # Remove old output
    if (Test-Path $Output) {
        Remove-Item $Output -Force
    }

    # Time compression
    $start = Get-Date

    switch ($Tool) {
        "crush" {
            & ".\target\release\crush.exe" compress $Input -o $Output --force 2>&1 | Out-Null
        }
        "gzip" {
            if ($script:GzipPath) {
                Get-Content $Input -Raw | & $script:GzipPath > $Output
            }
        }
        "gzip-fast" {
            if ($script:GzipPath) {
                Get-Content $Input -Raw | & $script:GzipPath -1 > $Output
            }
        }
    }

    $end = Get-Date
    $duration = ($end - $start).TotalSeconds

    # Calculate stats
    $inputSize = (Get-Item $Input).Length
    $outputSize = (Get-Item $Output).Length
    $ratio = [math]::Round(100 * (1 - $outputSize / $inputSize), 2)
    $throughput = [math]::Round(($inputSize / 1MB) / $duration, 2)

    return @{
        Duration = $duration
        InputSize = $inputSize
        OutputSize = $outputSize
        Ratio = $ratio
        Throughput = $throughput
    }
}

# Benchmark decompression
function Measure-Decompression {
    param(
        [string]$Tool,
        [string]$Input,
        [string]$Output,
        [string]$Label
    )

    Write-Host "Testing $Label decompression..." -ForegroundColor Blue

    # Remove old output
    if (Test-Path $Output) {
        Remove-Item $Output -Force
    }

    # Time decompression
    $start = Get-Date

    switch ($Tool) {
        "crush" {
            & ".\target\release\crush.exe" decompress $Input -o $Output --force 2>&1 | Out-Null
        }
        "gzip" {
            if ($script:GzipPath) {
                & $script:GzipPath -dc $Input > $Output
            }
        }
        "gzip-fast" {
            if ($script:GzipPath) {
                & $script:GzipPath -dc $Input > $Output
            }
        }
    }

    $end = Get-Date
    $duration = ($end - $start).TotalSeconds

    # Calculate throughput
    $outputSize = (Get-Item $Output).Length
    $throughput = [math]::Round(($outputSize / 1MB) / $duration, 2)

    return @{
        Duration = $duration
        Throughput = $throughput
    }
}

# Print results table
function Show-Results {
    param(
        [string]$DataType,
        [array]$Results
    )

    Write-Host ""
    Write-Host "═══════════════════════════════════════════════════════════════════════════" -ForegroundColor Green
    Write-Host "Results for $DataType data:" -ForegroundColor Green
    Write-Host "═══════════════════════════════════════════════════════════════════════════" -ForegroundColor Green
    Write-Host ""

    $header = "{0,-15} {1,12} {2,15} {3,12} {4,15}" -f "Tool", "Compress", "Decompress", "Ratio", "Comp Size"
    $subheader = "{0,-15} {1,12} {2,15} {3,12} {4,15}" -f "", "(MB/s)", "(MB/s)", "(%)", "(MB)"
    Write-Host $header
    Write-Host $subheader
    Write-Host "───────────────────────────────────────────────────────────────────────────"

    foreach ($result in $Results) {
        $line = "{0,-15} {1,12} {2,15} {3,12} {4,15}" -f `
            $result.Tool, `
            $result.CompSpeed, `
            $result.DecompSpeed, `
            $result.Ratio, `
            $result.Size
        Write-Host $line
    }

    Write-Host ""
}

# Run benchmark
function Start-Benchmark {
    param(
        [string]$TestFile,
        [string]$DataType,
        [int]$SizeMB
    )

    Write-Host "╔═══════════════════════════════════════════════════════════════════════════╗" -ForegroundColor Yellow
    Write-Host "║  Benchmarking $DataType data (${SizeMB}MB)" -ForegroundColor Yellow
    Write-Host "╚═══════════════════════════════════════════════════════════════════════════╝" -ForegroundColor Yellow
    Write-Host ""

    $results = @()
    $tools = @(
        @{Tool = "crush"; Label = "Crush"},
        @{Tool = "gzip-fast"; Label = "gzip --fast"},
        @{Tool = "gzip"; Label = "gzip"}
    )

    foreach ($t in $tools) {
        # Skip gzip if not available
        if ($t.Tool -like "gzip*" -and -not $script:GzipPath) {
            continue
        }

        # Compress
        $compOutput = "$TestFile.$($t.Tool).gz"
        $compResult = Measure-Compression $t.Tool $TestFile $compOutput $t.Label

        # Decompress
        $decompOutput = "$TestFile.$($t.Tool).decompressed"
        $decompResult = Measure-Decompression $t.Tool $compOutput $decompOutput $t.Label

        # Store results
        $results += @{
            Tool = $t.Label
            CompSpeed = $compResult.Throughput
            DecompSpeed = $decompResult.Throughput
            Ratio = $compResult.Ratio
            Size = [math]::Round($compResult.OutputSize / 1MB, 2)
        }

        # Cleanup
        Remove-Item $compOutput -ErrorAction SilentlyContinue
        Remove-Item $decompOutput -ErrorAction SilentlyContinue
    }

    Show-Results $DataType $results
}

# Main execution
function Main {
    Write-Host "╔═══════════════════════════════════════════════════════════════════════════╗" -ForegroundColor Green
    Write-Host "║         Crush Compression Benchmark - Comprehensive Comparison            ║" -ForegroundColor Green
    Write-Host "╚═══════════════════════════════════════════════════════════════════════════╝" -ForegroundColor Green
    Write-Host ""

    Test-Dependencies

    # Test parameters
    $testDir = ".\benchmark-data"
    if (-not (Test-Path $testDir)) {
        New-Item -ItemType Directory -Path $testDir | Out-Null
    }

    # Prepare test data
    Write-Host "Preparing test data..." -ForegroundColor Yellow
    Write-Host ""

    # 1. Text data
    $textFile = "$testDir\test-text.bin"
    if (-not (Test-Path $textFile)) {
        New-TestData -SizeMB $SizeMB -Output $textFile -Type "text"
    }

    # 2. Repetitive data
    $repFile = "$testDir\test-repetitive.bin"
    if (-not (Test-Path $repFile)) {
        New-TestData -SizeMB $SizeMB -Output $repFile -Type "repetitive"
    }

    # 3. Binary data
    $binFile = "$testDir\test-binary.bin"
    if (-not (Test-Path $binFile)) {
        New-TestData -SizeMB $SizeMB -Output $binFile -Type "binary"
    }

    # Run benchmarks
    Start-Benchmark $textFile "Text" $SizeMB
    Start-Benchmark $repFile "Repetitive" $SizeMB
    Start-Benchmark $binFile "Binary/Random" $SizeMB

    # Summary
    Write-Host "═══════════════════════════════════════════════════════════════════════════" -ForegroundColor Green
    Write-Host "Benchmark complete!" -ForegroundColor Green
    Write-Host "═══════════════════════════════════════════════════════════════════════════" -ForegroundColor Green
    Write-Host ""
    Write-Host "Notes:"
    Write-Host "  - Compression ratio shows % size reduction"
    Write-Host "  - Higher ratio = better compression"
    Write-Host "  - MB/s = Megabytes per second throughput"
    $msg = "  - Test file size: " + $SizeMB + "MB per test"
    Write-Host $msg
    Write-Host ""
    Write-Host "To re-run with fresh test data: Remove-Item -Recurse .\benchmark-data"
}



# Run main function
Main
