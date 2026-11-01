//! Output files that blocks decode straight into, in parallel.
//!
//! A single `write` of the decoded data is the largest serial cost of
//! multi-core decompression, and positional writes to one file do not scale
//! (`perf/log/2026-11-hybrid-speed-codec.md`). Mapping the file does. The
//! exception is recorded in `specs/026-hybrid-speed-codec/unsafe-exceptions.md`.

use crate::commands::utils::MMAP_MIN_BYTES;
use crush_core::OutputTarget;
use crush_core::error::{CrushError, Result};
use memmap2::MmapMut;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// A new output file, written through a mapping when possible.
pub struct FileTarget {
    file: File,
    map: Option<MmapMut>,
    allow_map: bool,
}

impl FileTarget {
    /// Create the file. With `allow_map` false, it only takes sequential writes.
    pub fn create(path: &Path, allow_map: bool) -> io::Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        let allow_map = allow_map && file.metadata()?.is_file();
        Ok(Self {
            file,
            map: None,
            allow_map,
        })
    }
}

impl OutputTarget for FileTarget {
    fn output_slice(&mut self, len: usize) -> Result<Option<&mut [u8]>> {
        if !self.allow_map || (len as u64) < MMAP_MIN_BYTES {
            return Ok(None);
        }
        self.map = map_output(&self.file, len as u64).map_err(CrushError::Io)?;
        Ok(self.map.as_deref_mut())
    }

    fn writer(&mut self) -> &mut dyn Write {
        &mut self.file
    }
}

/// Reserve `len` bytes for `file`, and map them writable.
///
/// `None` means the file cannot reserve space, so the caller writes in order.
/// Mapping without reserved space would turn a full disk into a crash.
fn map_output(file: &File, len: u64) -> io::Result<Option<MmapMut>> {
    if !reserve(file, len)? {
        return Ok(None);
    }
    // SAFETY:
    // - `file` was created by this process, and `same_file` in the caller keeps
    //   it from being the mapped input, so no other mapping aliases this one
    //   within the process.
    // - `reserve` allocated every page, so a store cannot fault for lack of
    //   space.
    // - Another process truncating the output file during decompression would
    //   make stores past the new end fault. This is the same trade-off as the
    //   input mapping in `utils::read_input_file`.
    let map = unsafe { MmapMut::map_mut(file) }?;
    Ok(Some(map))
}

/// Allocate `len` bytes on disk. `false` means this file system cannot.
#[cfg(target_os = "linux")]
fn reserve(file: &File, len: u64) -> io::Result<bool> {
    use rustix::fs::{FallocateFlags, fallocate};
    use rustix::io::Errno;
    match fallocate(file, FallocateFlags::empty(), 0, len) {
        Ok(()) => Ok(true),
        Err(Errno::OPNOTSUPP | Errno::NOSYS) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Allocate `len` bytes on disk. On Windows, extending a file allocates it.
#[cfg(windows)]
fn reserve(file: &File, len: u64) -> io::Result<bool> {
    file.set_len(len)?;
    Ok(true)
}

/// Other platforms have no reservation here, so they write in order.
#[cfg(not(any(target_os = "linux", windows)))]
fn reserve(_file: &File, _len: u64) -> io::Result<bool> {
    Ok(false)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn output_too_large_for_the_disk_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out");
        let mut target = FileTarget::create(&path, true).expect("create");
        assert!(target.output_slice(1 << 62).is_err());
    }

    #[test]
    fn small_outputs_use_sequential_writes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out");
        let mut target = FileTarget::create(&path, true).expect("create");
        assert!(target.output_slice(100).expect("slice").is_none());
    }

    #[test]
    fn a_large_output_maps_to_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out");
        let len = MMAP_MIN_BYTES as usize;
        let mut target = FileTarget::create(&path, true).expect("create");
        match target.output_slice(len).expect("slice") {
            Some(slice) => {
                slice.fill(0xA5);
                drop(target);
                assert_eq!(fs::read(&path).expect("read"), vec![0xA5; len]);
            }
            None => {
                target.writer().write_all(&vec![0xA5; len]).expect("write");
                drop(target);
                assert_eq!(fs::metadata(&path).expect("stat").len(), len as u64);
            }
        }
    }

    #[test]
    fn disallowed_targets_use_sequential_writes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out");
        let mut target = FileTarget::create(&path, false).expect("create");
        assert!(
            target
                .output_slice(MMAP_MIN_BYTES as usize)
                .expect("slice")
                .is_none()
        );
    }
}
