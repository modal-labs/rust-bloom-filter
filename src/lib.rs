// (C)opyleft 2013-2024 Frank Denis
// Licensed under the ICS license (https://opensource.org/licenses/ISC)

#![doc = include_str!("../README.md")]
#![warn(non_camel_case_types, non_upper_case_globals, unused_qualifications)]
#![deny(unsafe_code)]
#![allow(clippy::unreadable_literal, clippy::bool_comparison)]

mod header;

#[cfg(feature = "mmap")]
mod mmap;

use std::cmp;
use std::convert::TryInto;
use std::f64;
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::num::{NonZeroU64, NonZeroUsize};

#[cfg(feature = "random")]
use getrandom::getrandom;

use siphasher::sip::SipHasher13;

use header::{HEADER_SIZE, VERSION};

#[cfg(feature = "mmap")]
pub use mmap::MmapStorage;

/// A read-only bloom filter backed by a memory-mapped file.
#[cfg(feature = "mmap")]
pub type MmapBloom<T> = Bloom<T, MmapStorage>;

pub mod reexports {
    #[cfg(feature = "random")]
    pub use ::getrandom;
    pub use siphasher;
    #[cfg(feature = "serde")]
    pub use siphasher::reexports::serde;
}

/// Bloom filter structure, generic over storage backend.
///
/// Use `Bloom<T, Vec<u8>>` for heap-allocated filters, or [`MmapBloom`] for
/// read-only memory-mapped files (requires the `mmap` feature).
pub struct Bloom<T: ?Sized, S> {
    storage: S,
    bitmap_bits: NonZeroU64,
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
            self.bitmap_bits.get(),
            self.k_num,
            self.seed()
        )
    }
}

// --- Methods independent of storage ---

impl<T: ?Sized, S> Bloom<T, S> {
    /// Return the number of bits in the filter.
    pub fn len(&self) -> u64 {
        self.bitmap_bits.get()
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
    pub fn compute_bitmap_size(items_count: NonZeroUsize, fp_p: f64) -> NonZeroUsize {
        assert!(fp_p > 0.0 && fp_p < 1.0);
        let log2 = f64::consts::LN_2;
        let log2_2 = log2 * log2;
        let size = ((items_count.get() as f64) * f64::ln(fp_p) / (-8.0 * log2_2)).ceil() as usize;
        NonZeroUsize::new(size).expect("bitmap size must be at least 1 byte")
    }

    fn sips_from_seed(seed: &[u8; 32]) -> [SipHasher13; 2] {
        let k1: [u8; 16] = seed[0..16].try_into().unwrap();
        let k2: [u8; 16] = seed[16..32].try_into().unwrap();
        [SipHasher13::new_with_key(&k1), SipHasher13::new_with_key(&k2)]
    }

    fn bloom_hash(&self, hashes: &mut [u64; 2], item: &T, k_i: u32) -> u64
    where
        T: Hash,
    {
        if k_i < 2 {
            let mut sip = self.sips[k_i as usize];
            item.hash(&mut sip);
            let hash = sip.finish();
            hashes[k_i as usize] = hash;
            hash
        } else {
            (hashes[0]).wrapping_add((k_i as u64).wrapping_mul(hashes[1]))
                % 0xFFFF_FFFF_FFFF_FFC5u64 // largest u64 prime
        }
    }

    fn optimal_k_num(bitmap_bits: NonZeroU64, items_count: NonZeroUsize) -> u32 {
        let m = bitmap_bits.get() as f64;
        let n = items_count.get() as f64;
        let k_num = (m / n * f64::ln(2.0f64)).round() as u32;
        cmp::max(k_num, 1)
    }
}

// --- Read methods (any AsRef<[u8]> storage) ---

impl<T: ?Sized, S: AsRef<[u8]>> Bloom<T, S> {
    /// Check if an item is present in the set.
    /// There can be false positives, but no false negatives.
    pub fn check(&self, item: &T) -> bool
    where
        T: Hash,
    {
        let bits = &self.storage.as_ref()[HEADER_SIZE..];
        let mut hashes = [0u64, 0u64];
        for k_i in 0..self.k_num {
            let bit_offset =
                (self.bloom_hash(&mut hashes, item, k_i) % self.bitmap_bits.get()) as usize;
            let byte_offset = bit_offset / 8;
            let bit_shift = bit_offset % 8;
            if (bits[byte_offset] & (1 << bit_shift)) == 0 {
                return false;
            }
        }
        true
    }

    /// Test if there are no elements in the set.
    pub fn is_empty(&self) -> bool {
        self.storage.as_ref()[HEADER_SIZE..]
            .iter()
            .all(|&b| b == 0)
    }

    /// View the bloom filter as an opaque slice of bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.storage.as_ref()
    }

    pub(crate) fn parse(storage: S) -> Result<Self, &'static str> {
        let bytes = storage.as_ref();
        if bytes.len() < HEADER_SIZE {
            return Err("Invalid size");
        }
        let header = &bytes[0..HEADER_SIZE];
        let bits = &bytes[HEADER_SIZE..];

        if header[0] != VERSION {
            return Err("Version mismatch");
        }
        let k_num = u32::from_le_bytes(header[9..][0..4].try_into().unwrap());
        if k_num == 0 {
            return Err("Invalid number of keys");
        }
        let len_bytes_u64 = u64::from_le_bytes(header[1..][0..8].try_into().unwrap());
        let len_bytes: usize = len_bytes_u64.try_into().map_err(|_| "Too big")?;
        if bits.len() != len_bytes {
            return Err("Invalid size");
        }
        if len_bytes == 0 {
            return Err("Bitmap cannot be empty");
        }

        let bitmap_bits =
            NonZeroU64::new((bits.len() as u64).checked_mul(8).unwrap()).unwrap();
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&header[13..][0..32]);

        Ok(Self {
            storage,
            bitmap_bits,
            k_num,
            sips: Self::sips_from_seed(&seed),
            _phantom: PhantomData,
        })
    }
}

// --- Write methods (any AsMut<[u8]> storage) ---

impl<T: ?Sized, S: AsRef<[u8]> + AsMut<[u8]>> Bloom<T, S> {
    /// Record the presence of an item.
    pub fn set(&mut self, item: &T)
    where
        T: Hash,
    {
        let mut hashes = [0u64, 0u64];
        for k_i in 0..self.k_num {
            let bit_offset =
                (self.bloom_hash(&mut hashes, item, k_i) % self.bitmap_bits.get()) as usize;
            let bits = &mut self.storage.as_mut()[HEADER_SIZE..];
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
                (self.bloom_hash(&mut hashes, item, k_i) % self.bitmap_bits.get()) as usize;
            let bits = &mut self.storage.as_mut()[HEADER_SIZE..];
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
        for byte in self.storage.as_mut()[HEADER_SIZE..].iter_mut() {
            *byte = 0;
        }
    }

    /// Set all of the bits in the filter, making it appear like every key is in the set.
    pub fn fill(&mut self) {
        for byte in self.storage.as_mut()[HEADER_SIZE..].iter_mut() {
            *byte = !0;
        }
    }
}

impl<T: ?Sized, S: Clone> Clone for Bloom<T, S> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            bitmap_bits: self.bitmap_bits,
            k_num: self.k_num,
            sips: self.sips,
            _phantom: PhantomData,
        }
    }
}

// --- Vec<u8> storage (heap-allocated) ---

impl<T: ?Sized> From<Bloom<T, Vec<u8>>> for Vec<u8> {
    fn from(bloom: Bloom<T, Vec<u8>>) -> Vec<u8> {
        bloom.storage
    }
}

impl<T: ?Sized> Bloom<T, Vec<u8>> {
    /// Create a new bloom filter structure.
    /// bitmap_size is the size in bytes (not bits) that will be allocated in
    /// memory. items_count is an estimation of the maximum number of items
    /// to store. seed is a random value used to generate the hash functions.
    pub fn new_with_seed(
        bitmap_size: NonZeroUsize,
        items_count: NonZeroUsize,
        seed: &[u8; 32],
    ) -> Self {
        let bitmap_bits =
            NonZeroU64::new(bitmap_size.get() as u64 * 8).expect("nonzero bitmap size");
        let bitmap_size = bitmap_size.get();
        let k_num = Self::optimal_k_num(bitmap_bits, items_count);
        let mut storage = vec![0; HEADER_SIZE + bitmap_size];
        let header = &mut storage[0..HEADER_SIZE];
        header::set_version(header, VERSION);
        header::set_len_bytes(header, bitmap_size as u64);
        header::set_k_num(header, k_num);
        header::set_seed(header, seed);
        Self {
            storage,
            bitmap_bits,
            k_num,
            sips: Self::sips_from_seed(seed),
            _phantom: PhantomData,
        }
    }

    /// Create a new bloom filter structure.
    /// bitmap_size is the size in bytes (not bits) that will be allocated in
    /// memory. items_count is an estimation of the maximum number of items
    /// to store.
    #[cfg(feature = "random")]
    pub fn new(bitmap_size: NonZeroUsize, items_count: NonZeroUsize) -> Result<Self, &'static str> {
        let mut seed = [0u8; 32];
        getrandom(&mut seed).map_err(|_| "Could not generate random seed")?;
        Ok(Self::new_with_seed(bitmap_size, items_count, &seed))
    }

    /// Create a new bloom filter structure.
    /// items_count is an estimation of the maximum number of items to store.
    /// fp_p is the wanted rate of false positives, in ]0.0, 1.0[
    #[cfg(feature = "random")]
    pub fn new_for_fp_rate(items_count: NonZeroUsize, fp_p: f64) -> Result<Self, &'static str> {
        let bitmap_size = Self::compute_bitmap_size(items_count, fp_p);
        Self::new(bitmap_size, items_count)
    }

    /// Create a new bloom filter structure.
    /// items_count is an estimation of the maximum number of items to store.
    /// fp_p is the wanted rate of false positives, in ]0.0, 1.0[
    pub fn new_for_fp_rate_with_seed(
        items_count: NonZeroUsize,
        fp_p: f64,
        seed: &[u8; 32],
    ) -> Self {
        let bitmap_size = Self::compute_bitmap_size(items_count, fp_p);
        Self::new_with_seed(bitmap_size, items_count, seed)
    }

    /// Create a bloom filter from a slice of bytes, previously generated with `as_slice`.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, &'static str> {
        Self::parse(bytes.to_vec())
    }

    /// Transform a byte vector into a bloom filter.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, &'static str> {
        Self::parse(bytes)
    }
}

#[cfg(feature = "serde")]
mod serde_impl;

#[cfg(feature = "serde")]
pub use serde_impl::*;
