#!/bin/bash
# Benchmark comparison: crush vs gzip vs pigz
# Compares compression speed, decompression speed, and compression ratio

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check dependencies
check_dependencies() {
    local missing=()

    command -v gzip >/dev/null 2>&1 || missing+=("gzip")
    command -v pigz >/dev/null 2>&1 || missing+=("pigz")

    if [ ${#missing[@]} -ne 0 ]; then
        echo -e "${YELLOW}Warning: Missing tools: ${missing[*]}${NC}"
        echo "Install with: apt-get install pigz (or equivalent)"
        echo ""
    fi

    if [ ! -f "target/release/crush" ]; then
        echo -e "${RED}Error: crush binary not found${NC}"
        echo "Build with: cargo build --release"
        exit 1
    fi
}

# Generate test data
generate_test_data() {
    local size_mb=$1
    local output=$2
    local type=$3

    echo -e "${BLUE}Generating ${size_mb}MB ${type} test data...${NC}"

    case $type in
        "text")
            # Compressible text data (source code)
            cat /dev/urandom | base64 | head -c ${size_mb}M > "$output"
            ;;
        "binary")
            # Less compressible binary data
            dd if=/dev/urandom of="$output" bs=1M count=$size_mb status=none
            ;;
        "repetitive")
            # Highly compressible repetitive data
            yes "The quick brown fox jumps over the lazy dog. " | head -c ${size_mb}M > "$output"
            ;;
    esac
}

# Benchmark compression
benchmark_compress() {
    local tool=$1
    local input=$2
    local output=$3
    local label=$4

    echo -e "${BLUE}Testing $label compression...${NC}"

    # Remove old output
    rm -f "$output"

    # Time compression
    local start=$(date +%s.%N)

    case $tool in
        "crush")
            ./target/release/crush compress "$input" -o "$output" --force >/dev/null 2>&1
            ;;
        "gzip")
            gzip -c "$input" > "$output"
            ;;
        "gzip-fast")
            gzip -1 -c "$input" > "$output"
            ;;
        "pigz")
            pigz -c "$input" > "$output"
            ;;
        "pigz-fast")
            pigz -1 -c "$input" > "$output"
            ;;
    esac

    local end=$(date +%s.%N)
    local duration=$(echo "$end - $start" | bc)

    # Calculate stats
    local input_size=$(stat -c%s "$input" 2>/dev/null || stat -f%z "$input")
    local output_size=$(stat -c%s "$output" 2>/dev/null || stat -f%z "$output")
    local ratio=$(echo "scale=2; 100 * (1 - $output_size / $input_size)" | bc)
    local throughput=$(echo "scale=2; ($input_size / 1048576) / $duration" | bc)

    echo "$duration,$input_size,$output_size,$ratio,$throughput"
}

# Benchmark decompression
benchmark_decompress() {
    local tool=$1
    local input=$2
    local output=$3
    local label=$4

    echo -e "${BLUE}Testing $label decompression...${NC}"

    # Remove old output
    rm -f "$output"

    # Time decompression
    local start=$(date +%s.%N)

    case $tool in
        "crush")
            ./target/release/crush decompress "$input" -o "$output" --force >/dev/null 2>&1
            ;;
        "gzip"|"gzip-fast")
            gzip -dc "$input" > "$output"
            ;;
        "pigz"|"pigz-fast")
            pigz -dc "$input" > "$output"
            ;;
    esac

    local end=$(date +%s.%N)
    local duration=$(echo "$end - $start" | bc)

    # Calculate throughput (based on decompressed size)
    local output_size=$(stat -c%s "$output" 2>/dev/null || stat -f%z "$output")
    local throughput=$(echo "scale=2; ($output_size / 1048576) / $duration" | bc)

    echo "$duration,$throughput"
}

# Print results table
print_results() {
    local data_type=$1
    shift
    local results=("$@")

    echo ""
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}Results for $data_type data:${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════════════${NC}"
    echo ""

    printf "%-15s %12s %15s %12s %15s\n" "Tool" "Compress" "Decompress" "Ratio" "Comp Size"
    printf "%-15s %12s %15s %12s %15s\n" "" "(MB/s)" "(MB/s)" "(%)" "(MB)"
    echo "───────────────────────────────────────────────────────────────────────────"

    for result in "${results[@]}"; do
        IFS='|' read -r tool comp_speed decomp_speed ratio size <<< "$result"
        printf "%-15s %12s %15s %12s %15s\n" "$tool" "$comp_speed" "$decomp_speed" "$ratio" "$size"
    done

    echo ""
}

# Main benchmark
run_benchmark() {
    local test_file=$1
    local data_type=$2
    local size_mb=$3

    echo -e "${YELLOW}╔═══════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${YELLOW}║  Benchmarking $data_type data (${size_mb}MB)                                 ${NC}"
    echo -e "${YELLOW}╚═══════════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""

    local results=()
    local tools=("crush" "gzip-fast" "gzip" "pigz-fast" "pigz")
    local labels=("Crush" "gzip --fast" "gzip" "pigz --fast" "pigz")

    for i in "${!tools[@]}"; do
        local tool="${tools[$i]}"
        local label="${labels[$i]}"

        # Skip if tool not available
        if [[ "$tool" == "pigz"* ]] && ! command -v pigz >/dev/null 2>&1; then
            continue
        fi

        # Compress
        local comp_output="${test_file}.${tool}.gz"
        local comp_result=$(benchmark_compress "$tool" "$test_file" "$comp_output" "$label")
        IFS=',' read -r comp_time input_size output_size ratio comp_speed <<< "$comp_result"

        # Decompress
        local decomp_output="${test_file}.${tool}.decompressed"
        local decomp_result=$(benchmark_decompress "$tool" "$comp_output" "$decomp_output" "$label")
        IFS=',' read -r decomp_time decomp_speed <<< "$decomp_result"

        # Store results
        local size_mb=$(echo "scale=2; $output_size / 1048576" | bc)
        results+=("$label|$comp_speed|$decomp_speed|$ratio|$size_mb")

        # Cleanup
        rm -f "$comp_output" "$decomp_output"
    done

    print_results "$data_type" "${results[@]}"
}

# Main execution
main() {
    echo -e "${GREEN}╔═══════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║         Crush Compression Benchmark - Comprehensive Comparison            ║${NC}"
    echo -e "${GREEN}╚═══════════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""

    check_dependencies

    # Test parameters
    local test_dir="./benchmark-data"
    mkdir -p "$test_dir"

    local size_mb=100  # 100MB test files

    # Test different data types
    echo -e "${YELLOW}Preparing test data...${NC}"
    echo ""

    # 1. Text/source code (medium compressibility)
    local text_file="$test_dir/test-text.bin"
    if [ ! -f "$text_file" ]; then
        generate_test_data $size_mb "$text_file" "text"
    fi

    # 2. Repetitive data (high compressibility)
    local rep_file="$test_dir/test-repetitive.bin"
    if [ ! -f "$rep_file" ]; then
        generate_test_data $size_mb "$rep_file" "repetitive"
    fi

    # 3. Binary/random data (low compressibility)
    local bin_file="$test_dir/test-binary.bin"
    if [ ! -f "$bin_file" ]; then
        generate_test_data $size_mb "$bin_file" "binary"
    fi

    # Run benchmarks
    run_benchmark "$text_file" "Text/Base64" $size_mb
    run_benchmark "$rep_file" "Repetitive" $size_mb
    run_benchmark "$bin_file" "Binary/Random" $size_mb

    # Summary
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}Benchmark complete!${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Notes:"
    echo "  - Compression ratio shows % size reduction"
    echo "  - Higher ratio = better compression"
    echo "  - MB/s = Megabytes per second throughput"
    echo "  - Test file size: ${size_mb}MB per test"
    echo ""
    echo "To re-run with fresh test data: rm -rf $test_dir"
}

# Run main function
main "$@"
