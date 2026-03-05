// (C)opyleft 2013-2024 Frank Denis
// Licensed under the ICS license (https://opensource.org/licenses/ISC)

#![doc = include_str!("../README.md")]
#![warn(non_camel_case_types, non_upper_case_globals, unused_qualifications)]
#![deny(unsafe_code)]
#![allow(clippy::unreadable_literal, clippy::bool_comparison)]

mod hash;
mod header;
mod owned;

#[cfg(feature = "mmap")]
mod mmap;

pub use owned::OwnedStorage;

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

use header::HEADER_SIZE;

use std::cmp;
use std::f64;
use std::fmt::{self, Debug};
use std::hash::Hash;
use std::marker::PhantomData;

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
/// With the `mmap` feature, use `MmapStorage` for read-only memory-mapped files.
pub struct Bloom<T: ?Sized, S> {
    pub(crate) storage: S,
    pub(crate) bitmap_bits: u64,
    pub(crate) k_num: u32,
    pub(crate) sips: [SipHasher13; 2],
    pub(crate) _phantom: PhantomData<T>,
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

    pub(crate) fn from_storage(storage: S) -> Result<Self, &'static str> {
        let (bitmap_bits, k_num, seed) = header::parse(storage.bytes())?;
        Ok(Self {
            storage,
            bitmap_bits,
            k_num,
            sips: hash::sips_from_seed(&seed),
            _phantom: PhantomData,
        })
    }
}

// --- Write methods (StorageMut) ---

impl<T: ?Sized, S: StorageMut> Bloom<T, S> {
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
}

#[cfg(feature = "serde")]
mod serde_impl;

#[cfg(feature = "serde")]
pub use serde_impl::*;
