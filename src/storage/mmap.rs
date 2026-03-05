use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use memmap2::{Mmap, MmapOptions};

use super::{Sealed, Storage};

/// Read-only memory-mapped storage.
///
/// Mapped with `PROT_READ | MAP_SHARED` — the kernel prevents writes.
pub struct MmapStorage(pub(crate) Mmap);

impl Sealed for MmapStorage {}

impl Storage for MmapStorage {
    fn bytes(&self) -> &[u8] {
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
        // `MmapOptions::map` (as opposed to `map_mut`) produces a mapping with
        // PROT_READ | MAP_SHARED — no write permission at the kernel level.
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
