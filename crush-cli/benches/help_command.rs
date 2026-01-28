use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::process::Command;
use std::time::Duration;

/// Helper to get the crush binary path
fn crush_binary() -> std::path::PathBuf {
    // Build the binary first in release mode for accurate benchmarking
    let output = Command::new("cargo")
        .args(["build", "--release", "--bin", "crush"])
        .output()
        .expect("Failed to build crush binary");

    if !output.status.success() {
        panic!("Failed to build crush: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Find the target directory (could be in workspace root or current dir)
    let mut path = std::env::current_dir().expect("Failed to get current dir");

    // If we're in crush-cli, go up one level to workspace root
    if path.ends_with("crush-cli") {
        path.pop();
    }

    path.push("target");
    path.push("release");
    path.push("crush");

    #[cfg(windows)]
    path.set_extension("exe");

    if !path.exists() {
        panic!("Crush binary not found at: {}", path.display());
    }

    path
}

/// Benchmark: Root --help command
/// Target: <100ms
fn bench_root_help(c: &mut Criterion) {
    let binary = crush_binary();

    let mut group = c.benchmark_group("help_command");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("root_help", |b| {
        b.iter(|| {
            let output = Command::new(&binary)
                .arg("--help")
                .output()
                .expect("Failed to run crush --help");

            // Verify help was actually generated (non-empty output)
            assert!(!output.stdout.is_empty(), "Help output should not be empty");

            black_box(output)
        })
    });

    group.finish();
}

/// Benchmark: Subcommand help (compress, decompress, etc.)
/// Target: <100ms each
fn bench_subcommand_help(c: &mut Criterion) {
    let binary = crush_binary();

    let mut group = c.benchmark_group("help_command");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    let subcommands = ["compress", "decompress", "inspect", "config", "plugins"];

    for subcommand in subcommands.iter() {
        group.bench_with_input(
            BenchmarkId::new("subcommand", subcommand),
            subcommand,
            |b, &cmd| {
                b.iter(|| {
                    let output = Command::new(&binary)
                        .arg(cmd)
                        .arg("--help")
                        .output()
                        .expect(&format!("Failed to run crush {} --help", cmd));

                    // Verify help was actually generated
                    assert!(!output.stdout.is_empty(), "Help output should not be empty");

                    black_box(output)
                })
            },
        );
    }

    group.finish();
}

/// Benchmark: help subcommand (crush help <command>)
/// Tests the help command itself
fn bench_help_subcommand(c: &mut Criterion) {
    let binary = crush_binary();

    let mut group = c.benchmark_group("help_command");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    let commands = ["compress", "decompress", "inspect"];

    for command in commands.iter() {
        group.bench_with_input(
            BenchmarkId::new("help_cmd", command),
            command,
            |b, &cmd| {
                b.iter(|| {
                    let output = Command::new(&binary)
                        .arg("help")
                        .arg(cmd)
                        .output()
                        .expect(&format!("Failed to run crush help {}", cmd));

                    black_box(output)
                })
            },
        );
    }

    group.finish();
}

/// Benchmark: Short help (-h vs --help)
/// Measures if there's a difference between short and long flags
fn bench_short_vs_long_help(c: &mut Criterion) {
    let binary = crush_binary();

    let mut group = c.benchmark_group("help_command");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("short_flag", |b| {
        b.iter(|| {
            let output = Command::new(&binary)
                .arg("-h")
                .output()
                .expect("Failed to run crush -h");

            black_box(output)
        })
    });

    group.bench_function("long_flag", |b| {
        b.iter(|| {
            let output = Command::new(&binary)
                .arg("--help")
                .output()
                .expect("Failed to run crush --help");

            black_box(output)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_root_help,
    bench_subcommand_help,
    bench_help_subcommand,
    bench_short_vs_long_help
);
criterion_main!(benches);
