use std::convert::TryFrom;
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
#[cfg(feature = "mmap")]
use std::io;
use std::marker::PhantomData;

#[cfg(feature = "mmap")]
use memmap2::{Mmap, MmapOptions};
#[cfg(feature = "mmap")]
use std::fs::{File, OpenOptions};
#[cfg(feature = "mmap")]
use std::path::Path;

use siphasher::sip::SipHasher13;

use crate::bitmap::{BitMap, BITMAP_HEADER_SIZE};

enum Storage {
    Owned(Vec<u8>),
    #[cfg(feature = "mmap")]
    Mapped(Mmap),
}

impl Storage {
    fn bytes(&self) -> &[u8] {
        match self {
            Storage::Owned(bytes) => bytes,
            #[cfg(feature = "mmap")]
            Storage::Mapped(mmap) => &mmap[..],
        }
    }
}

/// A read-only bloom filter for existence checks.
///
/// Unlike [`Bloom`](crate::Bloom), this type has no mutating methods.
/// When backed by mmap, the file is mapped with `PROT_READ` only,
/// so the kernel will prevent any writes to the underlying data.
pub struct ReadOnlyBloom<T: ?Sized> {
    storage: Storage,
    bitmap_bits: u64,
    k_num: u32,
    sips: [SipHasher13; 2],
    _phantom: PhantomData<T>,
}

impl<T: ?Sized> Clone for ReadOnlyBloom<T> {
    fn clone(&self) -> Self {
        Self {
            storage: Storage::Owned(self.storage.bytes().to_vec()),
            bitmap_bits: self.bitmap_bits,
            k_num: self.k_num,
            sips: self.sips,
            _phantom: PhantomData,
        }
    }
}

impl<T: ?Sized> Debug for ReadOnlyBloom<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ReadOnlyBloom filter with {} bits and {} hash functions",
            self.bitmap_bits, self.k_num,
        )
    }
}

impl<T: ?Sized> ReadOnlyBloom<T> {
    fn from_storage(storage: Storage) -> Result<Self, &'static str> {
        let bytes = storage.bytes();
        BitMap::validate_layout(bytes)?;
        let header = &bytes[0..BITMAP_HEADER_SIZE];
        let k_num = BitMap::get_k_num(header);
        let seed = BitMap::get_seed(header);
        let sips = Self::sips_from_seed(&seed);
        let bitmap_bits = u64::try_from(bytes.len() - BITMAP_HEADER_SIZE)
            .unwrap()
            .checked_mul(8)
            .unwrap();
        Ok(Self {
            storage,
            bitmap_bits,
            k_num,
            sips,
            _phantom: PhantomData,
        })
    }

    /// Create a read-only bloom filter from a byte slice.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, &'static str> {
        Self::from_storage(Storage::Owned(bytes.to_vec()))
    }

    /// Create a read-only bloom filter from a byte vector.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, &'static str> {
        Self::from_storage(Storage::Owned(bytes))
    }

    /// Create a read-only bloom filter from a memory-mapped file.
    ///
    /// The file is mapped with `PROT_READ` only.
    #[cfg(feature = "mmap")]
    #[allow(unsafe_code)]
    pub fn from_mmap_file(file: &File) -> io::Result<Self> {
        // SAFETY: The returned mapping owns its lifetime independently of `file`,
        // and we only expose it through safe slice APIs in this module.
        let mmap = unsafe { MmapOptions::new().map(file) }?;
        Self::from_storage(Storage::Mapped(mmap))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    /// Create a read-only bloom filter from a file path.
    ///
    /// The file is opened read-only and mapped with `PROT_READ` only.
    #[cfg(feature = "mmap")]
    pub fn from_mmap_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).open(path)?;
        Self::from_mmap_file(&file)
    }

    /// Check if an item is present in the set.
    /// There can be false positives, but no false negatives.
    pub fn check(&self, item: &T) -> bool
    where
        T: Hash,
    {
        let bits = &self.storage.bytes()[BITMAP_HEADER_SIZE..];
        let mut hashes = [0u64, 0u64];
        for k_i in 0..self.k_num {
            let bit_offset =
                (Self::bloom_hash(&self.sips, &mut hashes, item, k_i) % self.bitmap_bits) as usize;
            let byte_offset = bit_offset / 8;
            let bit_shift = bit_offset % 8;
            if (bits[byte_offset] & (1 << bit_shift)) == 0 {
                return false;
            }
        }
        true
    }

    /// View the bloom filter as an opaque slice of bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.storage.bytes()
    }

    /// Serialize the bloom filter to a byte vector.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.storage.bytes().to_vec()
    }

    /// Return the number of hash functions.
    pub fn number_of_hash_functions(&self) -> u32 {
        self.k_num
    }

    /// Return the number of bits in the filter.
    pub fn len(&self) -> u64 {
        self.bitmap_bits
    }

    /// Test if there are no elements in the set.
    pub fn is_empty(&self) -> bool {
        self.storage.bytes()[BITMAP_HEADER_SIZE..]
            .iter()
            .all(|&b| b == 0)
    }

    fn sips_from_seed(seed: &[u8; 32]) -> [SipHasher13; 2] {
        let mut k1 = [0u8; 16];
        let mut k2 = [0u8; 16];
        k1.copy_from_slice(&seed[0..16]);
        k2.copy_from_slice(&seed[16..32]);
        [
            SipHasher13::new_with_key(&k1),
            SipHasher13::new_with_key(&k2),
        ]
    }

    fn bloom_hash(sips: &[SipHasher13; 2], hashes: &mut [u64; 2], item: &T, k_i: u32) -> u64
    where
        T: Hash,
    {
        if k_i < 2 {
            let mut sip = sips[k_i as usize].clone();
            item.hash(&mut sip);
            let hash = sip.finish();
            hashes[k_i as usize] = hash;
            hash
        } else {
            (hashes[0]).wrapping_add((k_i as u64).wrapping_mul(hashes[1]))
                % 0xFFFF_FFFF_FFFF_FFC5u64
        }
    }
}
