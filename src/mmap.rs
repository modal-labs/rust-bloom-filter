// (C)opyleft 2013-2024 Frank Denis
// Licensed under the ICS license (https://opensource.org/licenses/ISC)

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use memmap2::{Mmap, MmapOptions};

use crate::{Bloom, Storage};

/// Read-only memory-mapped storage.
///
/// Mapped with `PROT_READ | MAP_PRIVATE` — provides a stable snapshot
/// that is unaffected by concurrent modifications to the underlying file.
/// Multiple mappings of the same file share physical memory through the
/// kernel page cache (no duplication).
///
/// # Safety note
///
/// If the underlying file is **truncated** while the mapping is alive,
/// accessing the mapped region beyond the new file size will cause a
/// `SIGBUS` signal (undefined behaviour in Rust). Callers must ensure
/// the file is not truncated for the lifetime of this value.
pub struct MmapStorage(pub(crate) Mmap);

impl Storage for MmapStorage {
    fn bytes(&self) -> &[u8] {
        &self.0[..]
    }
}

impl MmapStorage {
    /// Create read-only memory-mapped storage from an open file.
    ///
    /// The file is mapped with `PROT_READ | MAP_PRIVATE`.
    #[allow(unsafe_code)]
    pub fn from_file(file: &File) -> io::Result<Self> {
        // SAFETY: The returned mapping owns its lifetime independently of `file`,
        // and we only expose it through safe slice APIs.
        //
        // `MmapOptions::map_copy_read_only` produces a mapping with
        // PROT_READ | MAP_PRIVATE — a stable read-only snapshot.
        let mmap = unsafe { MmapOptions::new().map_copy_read_only(file) }?;
        Ok(Self(mmap))
    }

    /// Create read-only memory-mapped storage from a file path.
    ///
    /// The file is opened read-only and mapped with `PROT_READ | MAP_PRIVATE`.
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).open(path)?;
        Self::from_file(&file)
    }
}

fn invalid_data(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

impl<T: ?Sized> Bloom<T, MmapStorage> {
    /// Open a read-only memory-mapped bloom filter from a file.
    ///
    /// The file is mapped with `PROT_READ | MAP_PRIVATE`.
    pub fn from_file(file: &File) -> io::Result<Self> {
        Self::from_storage(MmapStorage::from_file(file)?).map_err(invalid_data)
    }

    /// Open a read-only memory-mapped bloom filter from a file path.
    ///
    /// The file is opened read-only and mapped with `PROT_READ | MAP_PRIVATE`.
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::from_storage(MmapStorage::from_path(path)?).map_err(invalid_data)
    }
}
