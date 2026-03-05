use std::convert::TryFrom;
use std::io;
use std::marker::PhantomData;

#[cfg(feature = "random")]
use getrandom::getrandom;

use crate::header::{HEADER_SIZE, VERSION};
use crate::Bloom;
use super::{Sealed, Storage, StorageMut};

/// Heap-allocated storage. This is the default storage for [`Bloom`](crate::Bloom).
pub struct OwnedStorage(pub(crate) Vec<u8>);

impl Sealed for OwnedStorage {}

impl Storage for OwnedStorage {
    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl StorageMut for OwnedStorage {
    fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl OwnedStorage {
    pub(crate) fn new(len_bytes: usize, k_num: u32, seed: &[u8; 32]) -> Self {
        let mut bytes = vec![0; HEADER_SIZE + len_bytes];
        let header = &mut bytes[0..HEADER_SIZE];
        crate::header::set_version(header, VERSION);
        crate::header::set_len_bytes(header, len_bytes as u64);
        crate::header::set_k_num(header, k_num);
        crate::header::set_seed(header, seed);
        Self(bytes)
    }
}

impl<T: ?Sized> Clone for Bloom<T, OwnedStorage> {
    fn clone(&self) -> Self {
        Self {
            storage: OwnedStorage(self.storage.bytes().to_vec()),
            bitmap_bits: self.bitmap_bits,
            k_num: self.k_num,
            sips: self.sips,
            _phantom: PhantomData,
        }
    }
}

impl<T: ?Sized> Bloom<T, OwnedStorage> {
    /// Serialize the bloom filter to an opaque byte vector.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.storage.bytes().to_vec()
    }

    /// Transform the bloom filter into a byte vector.
    pub fn into_bytes(self) -> Vec<u8> {
        self.storage.0
    }

    /// Flush pending writes to persistent storage.
    /// For in-memory filters, this is a no-op.
    pub fn flush(&self) -> io::Result<()> {
        Ok(())
    }

    /// Create a new bloom filter structure.
    /// bitmap_size is the size in bytes (not bits) that will be allocated in
    /// memory. items_count is an estimation of the maximum number of items
    /// to store. seed is a random value used to generate the hash functions.
    pub fn new_with_seed(
        bitmap_size: usize,
        items_count: usize,
        seed: &[u8; 32],
    ) -> Result<Self, &'static str> {
        assert!(bitmap_size > 0 && items_count > 0);
        let bitmap_bits = u64::try_from(bitmap_size)
            .unwrap()
            .checked_mul(8u64)
            .unwrap();
        let k_num = Self::optimal_k_num(bitmap_bits, items_count);
        let storage = OwnedStorage::new(bitmap_size, k_num, seed);
        Self::from_storage(storage)
    }

    /// Create a new bloom filter structure.
    /// bitmap_size is the size in bytes (not bits) that will be allocated in
    /// memory. items_count is an estimation of the maximum number of items
    /// to store.
    #[cfg(feature = "random")]
    pub fn new(bitmap_size: usize, items_count: usize) -> Result<Self, &'static str> {
        let mut seed = [0u8; 32];
        getrandom(&mut seed).map_err(|_| "Could not generate random seed")?;
        Self::new_with_seed(bitmap_size, items_count, &seed)
    }

    /// Create a new bloom filter structure.
    /// items_count is an estimation of the maximum number of items to store.
    /// fp_p is the wanted rate of false positives, in ]0.0, 1.0[
    #[cfg(feature = "random")]
    pub fn new_for_fp_rate(items_count: usize, fp_p: f64) -> Result<Self, &'static str> {
        let bitmap_size = Self::compute_bitmap_size(items_count, fp_p);
        Bloom::new(bitmap_size, items_count)
    }

    /// Create a new bloom filter structure.
    /// items_count is an estimation of the maximum number of items to store.
    /// fp_p is the wanted rate of false positives, in ]0.0, 1.0[
    pub fn new_for_fp_rate_with_seed(
        items_count: usize,
        fp_p: f64,
        seed: &[u8; 32],
    ) -> Result<Self, &'static str> {
        let bitmap_size = Self::compute_bitmap_size(items_count, fp_p);
        Bloom::new_with_seed(bitmap_size, items_count, seed)
    }

    /// Create a bloom filter from a slice of bytes, previously generated with `as_slice`.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, &'static str> {
        Self::from_storage(OwnedStorage(bytes.to_vec()))
    }

    /// Transform a byte vector into a bloom filter.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, &'static str> {
        Self::from_storage(OwnedStorage(bytes))
    }

    #[doc(hidden)]
    /// Reallocate large heap allocated objects in the bitmap using the provided function.
    pub fn realloc_large_heap_allocated_objects(self, f: fn(Vec<u8>) -> Vec<u8>) -> Self {
        let previous_bytes = self.storage.0;
        let previous_len = previous_bytes.len();
        let new_bytes = f(previous_bytes);
        assert_eq!(previous_len, new_bytes.len());
        assert_eq!(
            crate::header::get_version(&new_bytes[0..HEADER_SIZE]),
            VERSION,
        );
        Self {
            storage: OwnedStorage(new_bytes),
            bitmap_bits: self.bitmap_bits,
            k_num: self.k_num,
            sips: self.sips,
            _phantom: PhantomData,
        }
    }
}
