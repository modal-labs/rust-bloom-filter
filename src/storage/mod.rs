mod owned;
pub use owned::OwnedStorage;

#[cfg(feature = "mmap")]
mod mmap;
#[cfg(feature = "mmap")]
pub use mmap::MmapStorage;

mod sealed {
    pub trait Sealed {}
}

pub(crate) use sealed::Sealed;

/// Trait for bloom filter storage backends.
///
/// This trait is sealed — it cannot be implemented outside this crate.
pub trait Storage: Sealed {
    /// View the raw bytes (header + bitmap).
    fn bytes(&self) -> &[u8];

    /// Consume the storage and return the bytes as a vector.
    fn into_bytes(self) -> Vec<u8>;
}
