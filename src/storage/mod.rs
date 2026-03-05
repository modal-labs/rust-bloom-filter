mod owned;
pub use owned::OwnedStorage;

#[cfg(feature = "mmap")]
mod mmap;
#[cfg(feature = "mmap")]
pub use mmap::MmapStorage;

/// Trait for bloom filter storage backends.
pub trait Storage {
    /// View the raw bytes (header + bitmap).
    fn bytes(&self) -> &[u8];
}

/// Trait for mutable bloom filter storage backends.
pub trait StorageMut: Storage {
    /// View the raw bytes mutably.
    fn bytes_mut(&mut self) -> &mut [u8];
}
