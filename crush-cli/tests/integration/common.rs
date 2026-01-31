use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create a Command for the crush binary
pub fn crush_cmd() -> Command {
    Command::cargo_bin("crush").expect("Failed to find crush binary")
}

/// Create a temporary directory for test files
pub fn test_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

/// Create a test file with given content
pub fn create_test_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).expect("Failed to write test file");
    path
}

/// Read file contents as bytes
pub fn read_file(path: &Path) -> Vec<u8> {
    fs::read(path).expect("Failed to read file")
}

/// Assert that two files have identical content
pub fn assert_files_equal(path1: &Path, path2: &Path) {
    let content1 = read_file(path1);
    let content2 = read_file(path2);
    assert_eq!(
        content1, content2,
        "Files differ: {} vs {}",
        path1.display(),
        path2.display()
    );
}

/// Create a test file with random data of given size
pub fn create_random_file(dir: &Path, name: &str, size: usize) -> PathBuf {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};

    let path = dir.join(name);
    let mut data = Vec::with_capacity(size);
    let mut hasher = RandomState::new().build_hasher();

    for i in 0..size {
        i.hash(&mut hasher);
        data.push((hasher.finish() & 0xFF) as u8);
    }

    fs::write(&path, &data).expect("Failed to write random test file");
    path
}

/// Assert that a file exists
pub fn assert_file_exists(path: &Path) {
    assert!(
        path.exists(),
        "File does not exist: {}",
        path.display()
    );
}

/// Assert that a file does not exist
pub fn assert_file_not_exists(path: &Path) {
    assert!(
        !path.exists(),
        "File should not exist: {}",
        path.display()
    );
}

/// Get file size in bytes
pub fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .expect("Failed to get file metadata")
        .len()
}

/// Assert that output file is smaller than input file
pub fn assert_compressed(input: &Path, output: &Path) {
    let input_size = file_size(input);
    let output_size = file_size(output);
    assert!(
        output_size < input_size,
        "Output ({} bytes) is not smaller than input ({} bytes)",
        output_size,
        input_size
    );
}
