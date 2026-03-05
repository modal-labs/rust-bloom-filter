// (C)opyleft 2013-2024 Frank Denis
// Licensed under the ICS license (https://opensource.org/licenses/ISC)

#![doc = include_str!("../README.md")]
#![warn(non_camel_case_types, non_upper_case_globals, unused_qualifications)]
#![deny(unsafe_code)]
#![allow(clippy::unreadable_literal, clippy::bool_comparison)]

mod hash;
mod header;
pub mod storage;

pub use storage::{OwnedStorage, Storage};

#[cfg(feature = "mmap")]
pub use storage::MmapStorage;

use header::HEADER_SIZE;

use std::cmp;
use std::convert::TryFrom;
use std::f64;
use std::fmt::{self, Debug};
use std::hash::Hash;
use std::io;
use std::marker::PhantomData;

#[cfg(feature = "random")]
use getrandom::getrandom;
use siphasher::sip::SipHasher13;

pub mod reexports {
    #[cfg(feature = "random")]
    pub use ::getrandom;
    pub use siphasher;
    #[cfg(feature = "serde")]
    pub use siphasher::reexports::serde;
}

/// Bloom filter structure, generic over storage backend.
///
/// The default storage is [`OwnedStorage`] (heap-allocated).
/// Use [`MmapStorage`] for read-only memory-mapped files.
pub struct Bloom<T: ?Sized, S> {
    storage: S,
    bitmap_bits: u64,
    k_num: u32,
    sips: [SipHasher13; 2],
    _phantom: PhantomData<T>,
}

// --- Debug ---

impl<T: ?Sized, S> Debug for Bloom<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Bloom filter with {} bits, {} hash functions and seed: {:?} ",
            self.bitmap_bits,
            self.k_num,
            self.seed()
        )
    }
}

// --- Clone (OwnedStorage only) ---

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

// --- Methods independent of storage ---

impl<T: ?Sized, S> Bloom<T, S> {
    /// Return the number of bits in the filter.
    pub fn len(&self) -> u64 {
        self.bitmap_bits
    }

    /// Return the number of hash functions used for `check` and `set`.
    pub fn number_of_hash_functions(&self) -> u32 {
        self.k_num
    }

    /// Return the seed used to generate the hash functions.
    pub fn seed(&self) -> [u8; 32] {
        let mut seed = [0u8; 32];
        seed[0..16].copy_from_slice(&self.sips[0].key());
        seed[16..32].copy_from_slice(&self.sips[1].key());
        seed
    }

    /// Compute a recommended bitmap size for items_count items
    /// and a fp_p rate of false positives.
    /// fp_p obviously has to be within the ]0.0, 1.0[ range.
    pub fn compute_bitmap_size(items_count: usize, fp_p: f64) -> usize {
        assert!(items_count > 0);
        assert!(fp_p > 0.0 && fp_p < 1.0);
        let log2 = f64::consts::LN_2;
        let log2_2 = log2 * log2;
        ((items_count as f64) * f64::ln(fp_p) / (-8.0 * log2_2)).ceil() as usize
    }

    fn optimal_k_num(bitmap_bits: u64, items_count: usize) -> u32 {
        let m = bitmap_bits as f64;
        let n = items_count as f64;
        let k_num = (m / n * f64::ln(2.0f64)).round() as u32;
        cmp::max(k_num, 1)
    }
}

// --- Read methods (any Storage) ---

impl<T: ?Sized, S: Storage> Bloom<T, S> {
    /// Check if an item is present in the set.
    /// There can be false positives, but no false negatives.
    pub fn check(&self, item: &T) -> bool
    where
        T: Hash,
    {
        let bits = &self.storage.bytes()[HEADER_SIZE..];
        hash::check(&self.sips, bits, self.bitmap_bits, self.k_num, item)
    }

    /// Test if there are no elements in the set.
    pub fn is_empty(&self) -> bool {
        self.storage.bytes()[HEADER_SIZE..]
            .iter()
            .all(|&b| b == 0)
    }

    /// View the bloom filter as an opaque slice of bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.storage.bytes()
    }

    /// Create a bloom filter from a pre-built storage backend.
    ///
    /// The storage must contain a valid serialized bloom filter
    /// (as produced by [`to_bytes`](Bloom::to_bytes) or [`as_slice`](Bloom::as_slice)).
    pub fn from_storage(storage: S) -> Result<Self, &'static str> {
        let (bitmap_bits, k_num, sips) = header::parse(storage.bytes())?;
        Ok(Self {
            storage,
            bitmap_bits,
            k_num,
            sips,
            _phantom: PhantomData,
        })
    }
}

// --- Write methods + constructors (OwnedStorage) ---

impl<T: ?Sized> Bloom<T, OwnedStorage> {
    /// Serialize the bloom filter to an opaque byte vector.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.storage.bytes().to_vec()
    }

    /// Transform the bloom filter into a byte vector.
    pub fn into_bytes(self) -> Vec<u8> {
        self.storage.0
    }

    /// Record the presence of an item.
    pub fn set(&mut self, item: &T)
    where
        T: Hash,
    {
        let mut hashes = [0u64, 0u64];
        for k_i in 0..self.k_num {
            let bit_offset =
                (hash::bloom_hash(&self.sips, &mut hashes, item, k_i) % self.bitmap_bits) as usize;
            let bits = &mut self.storage.bytes_mut()[HEADER_SIZE..];
            let byte_offset = bit_offset / 8;
            let bit_shift = bit_offset % 8;
            bits[byte_offset] |= 1 << bit_shift;
        }
    }

    /// Record the presence of an item in the set, and return the previous state of this item.
    pub fn check_and_set(&mut self, item: &T) -> bool
    where
        T: Hash,
    {
        let mut hashes = [0u64, 0u64];
        let mut found = true;
        for k_i in 0..self.k_num {
            let bit_offset =
                (hash::bloom_hash(&self.sips, &mut hashes, item, k_i) % self.bitmap_bits) as usize;
            let bits = &mut self.storage.bytes_mut()[HEADER_SIZE..];
            let byte_offset = bit_offset / 8;
            let bit_shift = bit_offset % 8;
            if (bits[byte_offset] & (1 << bit_shift)) == 0 {
                found = false;
                bits[byte_offset] |= 1 << bit_shift;
            }
        }
        found
    }

    /// Clear all of the bits in the filter, removing all keys from the set.
    pub fn clear(&mut self) {
        for byte in self.storage.bytes_mut()[HEADER_SIZE..].iter_mut() {
            *byte = 0;
        }
    }

    /// Set all of the bits in the filter, making it appear like every key is in the set.
    pub fn fill(&mut self) {
        for byte in self.storage.bytes_mut()[HEADER_SIZE..].iter_mut() {
            *byte = !0;
        }
    }

    /// Flush pending writes to persistent storage.
    /// For in-memory filters, this is a no-op.
    pub fn flush(&self) -> io::Result<()> {
        Ok(())
    }

    fn sync(&mut self) {
        let seed = self.seed();
        let header = &mut self.storage.bytes_mut()[0..HEADER_SIZE];
        header::set_k_num(header, self.k_num);
        header::set_seed(header, &seed);
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
        let storage = OwnedStorage::new(bitmap_size);
        let sips = header::sips_from_seed(seed);
        let mut res = Self {
            storage,
            bitmap_bits,
            k_num,
            sips,
            _phantom: PhantomData,
        };
        res.sync();
        Ok(res)
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
            header::get_version(&new_bytes[0..HEADER_SIZE]),
            header::VERSION,
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

#[cfg(feature = "serde")]
mod serde_impl;

#[cfg(feature = "serde")]
pub use serde_impl::*;
