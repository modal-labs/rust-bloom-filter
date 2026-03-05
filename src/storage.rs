use std::io;

#[cfg(feature = "mmap")]
use memmap2::{Mmap, MmapMut, MmapOptions};
#[cfg(feature = "mmap")]
use std::convert::TryFrom;
#[cfg(feature = "mmap")]
use std::fs::File;

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

/// Trait for mutable bloom filter storage backends.
///
/// This trait is sealed — it cannot be implemented outside this crate.
pub trait StorageMut: Storage {
    /// View the raw bytes mutably.
    fn bytes_mut(&mut self) -> &mut [u8];
}

/// Trait for storage backends that support flushing to persistent storage.
///
/// This trait is sealed — it cannot be implemented outside this crate.
pub trait Flush: sealed::Sealed {
    /// Flush pending writes. For in-memory storage this is a no-op.
    fn flush(&self) -> io::Result<()>;
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

impl StorageMut for OwnedStorage {
    fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl Flush for OwnedStorage {
    fn flush(&self) -> io::Result<()> {
        Ok(())
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
}

/// Read-write memory-mapped storage.
#[cfg(feature = "mmap")]
pub struct MmapStorage(pub(crate) MmapMut);

#[cfg(feature = "mmap")]
impl sealed::Sealed for MmapStorage {}

#[cfg(feature = "mmap")]
impl Storage for MmapStorage {
    fn bytes(&self) -> &[u8] {
        &self.0[..]
    }
    fn into_bytes(self) -> Vec<u8> {
        self.0[..].to_vec()
    }
}

#[cfg(feature = "mmap")]
impl StorageMut for MmapStorage {
    fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }
}

#[cfg(feature = "mmap")]
impl Flush for MmapStorage {
    fn flush(&self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(feature = "mmap")]
impl MmapStorage {
    #[allow(unsafe_code)]
    pub(crate) fn from_file(file: &File) -> io::Result<Self> {
        // SAFETY: The returned mapping owns its lifetime independently of `file`,
        // and we only expose it through safe slice APIs.
        let mmap = unsafe { MmapOptions::new().map_mut(file) }?;
        Ok(Self(mmap))
    }

    pub(crate) fn create_in_file(file: &File, len_bytes: usize) -> io::Result<Self> {
        let total_len = BITMAP_HEADER_SIZE
            .checked_add(len_bytes)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Too big"))?;
        let total_len_u64 = u64::try_from(total_len)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Too big"))?;
        let len_bytes_u64 = u64::try_from(len_bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Too big"))?;
        file.set_len(total_len_u64)?;

        let mut storage = Self::from_file(file)?;
        storage.0.fill(0);
        let header = &mut storage.0[0..BITMAP_HEADER_SIZE];
        crate::bitmap::set_version(header, VERSION);
        crate::bitmap::set_len_bytes(header, len_bytes_u64);
        crate::bitmap::set_k_num(header, 0);
        Ok(storage)
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
