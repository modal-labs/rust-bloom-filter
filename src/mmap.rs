// (C)opyleft 2013-2024 Frank Denis
// Licensed under the ICS license (https://opensource.org/licenses/ISC)

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use memmap2::{Mmap, MmapOptions};

use crate::Bloom;

/// Read-only memory-mapped storage.
///
/// Mapped with `PROT_READ | MAP_SHARED`. Multiple mappings of the same
/// file share physical memory through the kernel page cache.
///
/// # Safety notes
///
/// The mapping reflects the file's contents in the page cache. If
/// another process modifies the file while the mapping is alive, those
/// changes may become visible (this is standard mmap behaviour).
///
/// If the underlying file is **truncated** while the mapping is alive,
/// accessing pages beyond the new file size causes `SIGBUS` (undefined
/// behaviour in Rust). Callers must ensure the file is not truncated
/// for the lifetime of this value.
pub struct MmapStorage(Mmap);

impl AsRef<[u8]> for MmapStorage {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl MmapStorage {
    /// Create read-only memory-mapped storage from an open file.
    ///
    /// The file is mapped with `PROT_READ | MAP_SHARED`.
    #[allow(unsafe_code)]
    pub fn from_file(file: &File) -> io::Result<Self> {
        // SAFETY: The returned mapping owns its lifetime independently of `file`,
        // and we only expose it through safe slice APIs.
        //
        // `MmapOptions::map` produces a read-only mapping with
        // PROT_READ | MAP_SHARED.
        let mmap = unsafe { MmapOptions::new().map(file) }?;
        Ok(Self(mmap))
    }

    /// Create read-only memory-mapped storage from a file path.
    ///
    /// The file is opened read-only and mapped with `PROT_READ | MAP_SHARED`.
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).open(path)?;
        Self::from_file(&file)
    }
}

impl<T: ?Sized> Bloom<T, MmapStorage> {
    /// Open a read-only memory-mapped bloom filter from a file.
    ///
    /// The file is mapped with `PROT_READ | MAP_SHARED`.
    pub fn from_file(file: &File) -> io::Result<Self> {
        Self::from_storage(MmapStorage::from_file(file)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Open a read-only memory-mapped bloom filter from a file path.
    ///
    /// The file is opened read-only and mapped with `PROT_READ | MAP_SHARED`.
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::from_storage(MmapStorage::from_path(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}
