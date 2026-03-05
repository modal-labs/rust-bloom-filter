use std::fs::File;
use std::io;

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
    fn into_bytes(self) -> Vec<u8> {
        self.0[..].to_vec()
    }
}

impl MmapStorage {
    #[allow(unsafe_code)]
    pub(crate) fn from_file(file: &File) -> io::Result<Self> {
        // SAFETY: The returned mapping owns its lifetime independently of `file`,
        // and we only expose it through safe slice APIs.
        //
        // `MmapOptions::map` (as opposed to `map_mut`) produces a mapping with
        // PROT_READ | MAP_SHARED — no write permission at the kernel level.
        let mmap = unsafe { MmapOptions::new().map(file) }?;
        Ok(Self(mmap))
    }
}
