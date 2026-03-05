#[cfg(feature = "mmap")]
use memmap2::{Mmap, MmapOptions};
#[cfg(feature = "mmap")]
use std::fs::File;
#[cfg(feature = "mmap")]
use std::io;

use crate::bitmap::{BITMAP_HEADER_SIZE, VERSION};

mod sealed {
    pub trait Sealed {}
}

/// Trait for bloom filter storage backends.
///
/// This trait is sealed — it cannot be implemented outside this crate.
pub trait Storage: sealed::Sealed {
    /// View the raw bytes (header + bitmap).
    fn bytes(&self) -> &[u8];

    /// Consume the storage and return the bytes as a vector.
    fn into_bytes(self) -> Vec<u8>;
}

/// Heap-allocated storage. This is the default storage for [`Bloom`](crate::Bloom).
pub struct OwnedStorage(pub(crate) Vec<u8>);

impl sealed::Sealed for OwnedStorage {}

impl Storage for OwnedStorage {
    fn bytes(&self) -> &[u8] {
        &self.0
    }
    fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl OwnedStorage {
    pub(crate) fn new(len_bytes: usize) -> Self {
        let mut bytes = vec![0; BITMAP_HEADER_SIZE + len_bytes];
        let header = &mut bytes[0..BITMAP_HEADER_SIZE];
        crate::bitmap::set_version(header, VERSION);
        crate::bitmap::set_len_bytes(header, len_bytes as u64);
        crate::bitmap::set_k_num(header, 0);
        Self(bytes)
    }

    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

/// Read-only memory-mapped storage.
///
/// Mapped with `PROT_READ | MAP_SHARED` — the kernel prevents writes.
#[cfg(feature = "mmap")]
pub struct MmapReadOnlyStorage(pub(crate) Mmap);

#[cfg(feature = "mmap")]
impl sealed::Sealed for MmapReadOnlyStorage {}

#[cfg(feature = "mmap")]
impl Storage for MmapReadOnlyStorage {
    fn bytes(&self) -> &[u8] {
        &self.0[..]
    }
    fn into_bytes(self) -> Vec<u8> {
        self.0[..].to_vec()
    }
}

#[cfg(feature = "mmap")]
impl MmapReadOnlyStorage {
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
