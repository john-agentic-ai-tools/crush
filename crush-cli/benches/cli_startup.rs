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

/// Benchmark: CLI startup time with --version flag
/// Target: <50ms
fn bench_version_flag(c: &mut Criterion) {
    let binary = crush_binary();

    let mut group = c.benchmark_group("cli_startup");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("version_flag", |b| {
        b.iter(|| {
            let output = Command::new(&binary)
                .arg("--version")
                .output()
                .expect("Failed to run crush --version");

            black_box(output)
        })
    });

    group.finish();
}

/// Benchmark: CLI startup time with invalid command (tests argument parsing)
/// Target: <50ms
fn bench_invalid_command(c: &mut Criterion) {
    let binary = crush_binary();

    let mut group = c.benchmark_group("cli_startup");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("invalid_command", |b| {
        b.iter(|| {
            let output = Command::new(&binary)
                .arg("nonexistent-command")
                .output()
                .expect("Failed to run crush with invalid command");

            black_box(output)
        })
    });

    group.finish();
}

/// Benchmark: CLI startup time with no arguments (shows help)
/// Target: <50ms
fn bench_no_args(c: &mut Criterion) {
    let binary = crush_binary();

    let mut group = c.benchmark_group("cli_startup");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("no_args", |b| {
        b.iter(|| {
            let output = Command::new(&binary)
                .output()
                .expect("Failed to run crush with no args");

            black_box(output)
        })
    });

    group.finish();
}

/// Benchmark: CLI startup with verbose flags
/// Measures the overhead of parsing multiple flags
fn bench_verbose_flags(c: &mut Criterion) {
    let binary = crush_binary();

    let mut group = c.benchmark_group("cli_startup");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    for verbosity in ["-v", "-vv", "-vvv"].iter() {
        group.bench_with_input(
            BenchmarkId::new("verbose", verbosity),
            verbosity,
            |b, &v| {
                b.iter(|| {
                    let output = Command::new(&binary)
                        .arg(v)
                        .arg("--version")
                        .output()
                        .expect("Failed to run crush with verbose flags");

                    black_box(output)
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_version_flag,
    bench_invalid_command,
    bench_no_args,
    bench_verbose_flags
);
criterion_main!(benches);
