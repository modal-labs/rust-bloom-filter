// (C)opyleft 2013-2024 Frank Denis
// Licensed under the ICS license (https://opensource.org/licenses/ISC)

#![doc = include_str!("../README.md")]
#![warn(non_camel_case_types, non_upper_case_globals, unused_qualifications)]
#![deny(unsafe_code)]
#![allow(clippy::unreadable_literal, clippy::bool_comparison)]

mod bitmap;
use bitmap::*;

mod hash;
mod readonly;
pub use readonly::ReadOnlyBloom;

use std::cmp;
use std::convert::TryFrom;
use std::f64;
use std::fmt::{self, Debug};
#[cfg(feature = "mmap")]
use std::fs::{File, OpenOptions};
use std::hash::Hash;
use std::io;
use std::marker::PhantomData;
#[cfg(feature = "mmap")]
use std::path::Path;

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

/// Bloom filter structure
#[derive(Clone)]
pub struct Bloom<T: ?Sized> {
    bitmap: BitMap,
    bitmap_bits: u64,
    k_num: u32,
    sips: [SipHasher13; 2],

    _phantom: PhantomData<T>,
}

impl<T: ?Sized> Debug for Bloom<T> {
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

impl<T: ?Sized> Bloom<T> {
    /// Create a new bloom filter structure.
    /// bitmap_size is the size in bytes (not bits) that will be allocated in
    /// memory items_count is an estimation of the maximum number of items
    /// to store. seed is a random value used to generate the hash
    /// functions.
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
        let bitmap = BitMap::new(bitmap_size);
        let sips = Self::sips_from_seed(seed);
        let mut res = Self {
            bitmap,
            bitmap_bits,
            k_num,
            sips,
            _phantom: PhantomData,
        };
        res.sync();
        Ok(res)
    }

    /// Create a new memory-mapped bloom filter structure backed by a file.
    /// `path` is truncated if it already exists.
    #[cfg(feature = "mmap")]
    pub fn new_mmap_with_seed<P: AsRef<Path>>(
        path: P,
        bitmap_size: usize,
        items_count: usize,
        seed: &[u8; 32],
    ) -> io::Result<Self> {
        assert!(bitmap_size > 0 && items_count > 0);
        let bitmap_bits = u64::try_from(bitmap_size)
            .unwrap()
            .checked_mul(8u64)
            .unwrap();
        let k_num = Self::optimal_k_num(bitmap_bits, items_count);
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(path)?;
        let bitmap = BitMap::create_mmap_in_file(&file, bitmap_size)?;
        let sips = Self::sips_from_seed(seed);
        let mut res = Self {
            bitmap,
            bitmap_bits,
            k_num,
            sips,
            _phantom: PhantomData,
        };
        res.sync();
        res.flush()?;
        Ok(res)
    }

    /// Create a new bloom filter structure.
    /// bitmap_size is the size in bytes (not bits) that will be allocated in
    /// memory items_count is an estimation of the maximum number of items
    /// to store.
    #[cfg(feature = "random")]
    pub fn new(bitmap_size: usize, items_count: usize) -> Result<Self, &'static str> {
        let mut seed = [0u8; 32];
        getrandom(&mut seed).map_err(|_| "Could not generate random seed")?;
        let res = Self::new_with_seed(bitmap_size, items_count, &seed)?;
        Ok(res)
    }

    /// Create a new memory-mapped bloom filter structure backed by a file.
    /// `path` is truncated if it already exists.
    #[cfg(all(feature = "mmap", feature = "random"))]
    pub fn new_mmap<P: AsRef<Path>>(
        path: P,
        bitmap_size: usize,
        items_count: usize,
    ) -> io::Result<Self> {
        let mut seed = [0u8; 32];
        getrandom(&mut seed)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Could not generate random seed"))?;
        Self::new_mmap_with_seed(path, bitmap_size, items_count, &seed)
    }

    /// Create a new bloom filter structure.
    /// items_count is an estimation of the maximum number of items to store.
    /// fp_p is the wanted rate of false positives, in ]0.0, 1.0[
    #[cfg(feature = "random")]
    pub fn new_for_fp_rate(items_count: usize, fp_p: f64) -> Result<Self, &'static str> {
        let bitmap_size = Self::compute_bitmap_size(items_count, fp_p);
        Bloom::new(bitmap_size, items_count)
    }

    /// Create a new memory-mapped bloom filter structure backed by a file.
    /// `path` is truncated if it already exists.
    #[cfg(all(feature = "mmap", feature = "random"))]
    pub fn new_mmap_for_fp_rate<P: AsRef<Path>>(
        path: P,
        items_count: usize,
        fp_p: f64,
    ) -> io::Result<Self> {
        let bitmap_size = Self::compute_bitmap_size(items_count, fp_p);
        Self::new_mmap(path, bitmap_size, items_count)
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

    /// Create a new memory-mapped bloom filter structure backed by a file.
    /// `path` is truncated if it already exists.
    #[cfg(feature = "mmap")]
    pub fn new_mmap_for_fp_rate_with_seed<P: AsRef<Path>>(
        path: P,
        items_count: usize,
        fp_p: f64,
        seed: &[u8; 32],
    ) -> io::Result<Self> {
        let bitmap_size = Self::compute_bitmap_size(items_count, fp_p);
        Self::new_mmap_with_seed(path, bitmap_size, items_count, seed)
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

    /// Return the number of bits in the filter.
    pub fn len(&self) -> u64 {
        self.bitmap.len_bits()
    }

    /// Record the presence of an item.
    pub fn set(&mut self, item: &T)
    where
        T: Hash,
    {
        let mut hashes = [0u64, 0u64];
        for k_i in 0..self.k_num {
            let bit_offset = (self.bloom_hash(&mut hashes, item, k_i) % self.bitmap_bits) as usize;
            self.bitmap.set(bit_offset);
        }
    }

    /// Check if an item is present in the set.
    /// There can be false positives, but no false negatives.
    pub fn check(&self, item: &T) -> bool
    where
        T: Hash,
    {
        let mut hashes = [0u64, 0u64];
        for k_i in 0..self.k_num {
            let bit_offset = (self.bloom_hash(&mut hashes, item, k_i) % self.bitmap_bits) as usize;
            if self.bitmap.get(bit_offset) == false {
                return false;
            }
        }
        true
    }

    /// Record the presence of an item in the set, and return the previous state of this item.
    pub fn check_and_set(&mut self, item: &T) -> bool
    where
        T: Hash,
    {
        let mut hashes = [0u64, 0u64];
        let mut found = true;
        for k_i in 0..self.k_num {
            let bit_offset = (self.bloom_hash(&mut hashes, item, k_i) % self.bitmap_bits) as usize;
            if self.bitmap.get(bit_offset) == false {
                found = false;
                self.bitmap.set(bit_offset);
            }
        }
        found
    }

    /// View the bloom filter as an opaque slice of bytes.
    /// This can be used to save the bloom filter to a file.
    pub fn as_slice(&self) -> &[u8] {
        self.bitmap.as_slice()
    }

    /// Create a bloom filter from a slice of bytes, previously generated with `as_slice`.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, &'static str> {
        let bitmap = BitMap::from_slice(bytes)?;
        Self::from_bitmap(bitmap)
    }

    /// Create a bloom filter from a read-write memory-mapped file.
    #[cfg(feature = "mmap")]
    pub fn from_mmap_file(file: &File) -> io::Result<Self> {
        let mmap = BitMap::map_file_mut(file)?;
        let bitmap = BitMap::from_mmap_mut(mmap)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        Self::from_bitmap(bitmap).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    /// Create a bloom filter from a read-write memory-mapped file path.
    #[cfg(feature = "mmap")]
    pub fn from_mmap_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Self::from_mmap_file(&file)
    }

    /// Serialize the bloom filter to an opaque byte vector.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bitmap.to_bytes()
    }

    /// Transform the bloom filter into a byte vector.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bitmap.into_bytes()
    }

    /// Transform a byte vector into a bloom filter.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, &'static str> {
        let bitmap = BitMap::from_bytes(bytes)?;
        Self::from_bitmap(bitmap)
    }

    /// Return the number of hash functions used for `check` and `set`
    pub fn number_of_hash_functions(&self) -> u32 {
        self.k_num
    }

    /// Clear all of the bits in the filter, removing all keys from the set
    pub fn clear(&mut self) {
        self.bitmap.clear()
    }

    /// Set all of the bits in the filter, making it appear like every key is in the set
    pub fn fill(&mut self) {
        self.bitmap.set_all()
    }

    /// Flush pending writes if the filter is memory-mapped.
    /// For in-memory filters, this is a no-op.
    pub fn flush(&self) -> io::Result<()> {
        self.bitmap.flush()
    }

    /// Test if there are no elements in the set
    pub fn is_empty(&self) -> bool {
        !self.bitmap.any()
    }

    /// Return the seed used to generate the hash functions
    pub fn seed(&self) -> [u8; 32] {
        let mut seed = [0u8; 32];
        seed[0..16].copy_from_slice(&self.sips[0].key());
        seed[16..32].copy_from_slice(&self.sips[1].key());
        seed
    }

    #[doc(hidden)]
    /// Reallocate large heap allocated objects in the bitmap using the provided function.
    /// The function is expected to return a vector of the same length as the input vector,
    /// with the same content, but possibly allocated at a different location.
    /// Most applications should not need to call this function.
    pub fn realloc_large_heap_allocated_objects(mut self, f: fn(Vec<u8>) -> Vec<u8>) -> Self {
        self.bitmap = self.bitmap.realloc_large_heap_allocated_objects(f);
        self
    }

    fn from_bitmap(bitmap: BitMap) -> Result<Self, &'static str> {
        let header = bitmap.header();
        let k_num = BitMap::get_k_num(header);
        let seed = BitMap::get_seed(header);
        let sips = Self::sips_from_seed(&seed);
        let bitmap_bits = bitmap.len_bits();
        Ok(Self {
            bitmap,
            bitmap_bits,
            k_num,
            sips,
            _phantom: PhantomData,
        })
    }

    fn sips_from_seed(seed: &[u8; 32]) -> [SipHasher13; 2] {
        hash::sips_from_seed(seed)
    }

    fn sync(&mut self) {
        let seed = self.seed();
        let header = self.bitmap.header_mut();
        BitMap::set_k_num(header, self.k_num);
        BitMap::set_seed(header, &seed);
    }

    #[allow(dead_code)]
    fn optimal_k_num(bitmap_bits: u64, items_count: usize) -> u32 {
        let m = bitmap_bits as f64;
        let n = items_count as f64;
        let k_num = (m / n * f64::ln(2.0f64)).round() as u32;
        cmp::max(k_num, 1)
    }

    fn bloom_hash(&self, hashes: &mut [u64; 2], item: &T, k_i: u32) -> u64
    where
        T: Hash,
    {
        hash::bloom_hash(&self.sips, hashes, item, k_i)
    }
}

#[cfg(feature = "serde")]
mod serde_extensions {
    use super::*;

    use reexports::serde;

    use serde::{
        de::{Error as DeError, Visitor},
        Deserializer, Serializer,
    };

    pub fn serialize<S: Serializer, T: ?Sized>(
        bloom: &Bloom<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bloom.as_slice())
    }

    struct BloomVisitor<T: ?Sized> {
        _phantom: PhantomData<T>,
    }

    impl<'de, T: ?Sized> Visitor<'de> for BloomVisitor<T> {
        type Value = Bloom<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("Blom filter")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Bloom::from_slice(v).map_err(E::custom)
        }

        fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Bloom::from_bytes(v).map_err(E::custom)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: ?Sized>(
        deserializer: D,
    ) -> Result<Bloom<T>, D::Error> {
        deserializer.deserialize_bytes(BloomVisitor {
            _phantom: PhantomData,
        })
    }
}

#[cfg(feature = "serde")]
pub use serde_extensions::*;
