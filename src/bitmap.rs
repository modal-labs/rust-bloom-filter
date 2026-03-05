use std::convert::TryFrom;
use std::fmt::Debug;
use std::io;

#[cfg(feature = "mmap")]
use memmap2::{MmapMut, MmapOptions};
#[cfg(feature = "mmap")]
use std::fs::File;

pub const VERSION: u8 = 1;
pub const BITMAP_HEADER_SIZE: usize = 1 + 8 + 4 + 32;

#[derive(Debug)]
enum BitMapStorage {
    Owned(Vec<u8>),
    #[cfg(feature = "mmap")]
    Mapped(MmapMut),
}

#[derive(Debug)]
pub(crate) struct BitMap {
    header_and_bits: BitMapStorage,
}

impl Clone for BitMap {
    fn clone(&self) -> Self {
        Self {
            header_and_bits: BitMapStorage::Owned(self.as_slice().to_vec()),
        }
    }
}

impl BitMap {
    pub fn new(len_bytes: usize) -> Self {
        let mut header_and_bits = vec![0; BITMAP_HEADER_SIZE + len_bytes];
        let header = &mut header_and_bits[0..BITMAP_HEADER_SIZE];
        Self::set_version(header, VERSION);
        Self::set_len_bytes(header, len_bytes as u64);
        Self::set_k_num(header, 0);
        Self {
            header_and_bits: BitMapStorage::Owned(header_and_bits),
        }
    }

    #[inline]
    fn bytes(&self) -> &[u8] {
        match &self.header_and_bits {
            BitMapStorage::Owned(bytes) => bytes,
            #[cfg(feature = "mmap")]
            BitMapStorage::Mapped(mmap) => &mmap[..],
        }
    }

    #[inline]
    fn bytes_mut(&mut self) -> &mut [u8] {
        match &mut self.header_and_bits {
            BitMapStorage::Owned(bytes) => bytes,
            #[cfg(feature = "mmap")]
            BitMapStorage::Mapped(mmap) => &mut mmap[..],
        }
    }

    #[inline]
    pub(crate) fn bits(&self) -> &[u8] {
        &self.bytes()[BITMAP_HEADER_SIZE..]
    }

    #[inline]
    fn bits_mut(&mut self) -> &mut [u8] {
        &mut self.bytes_mut()[BITMAP_HEADER_SIZE..]
    }

    #[inline]
    pub fn header_mut(&mut self) -> &mut [u8] {
        &mut self.bytes_mut()[0..BITMAP_HEADER_SIZE]
    }

    #[inline]
    fn get_version(header: &[u8]) -> u8 {
        header[0]
    }

    #[inline]
    fn set_version(header: &mut [u8], version: u8) {
        header[0] = version;
    }

    #[inline]
    fn set_len_bytes(header: &mut [u8], len_bytes: u64) {
        header[1..][0..8].copy_from_slice(&len_bytes.to_le_bytes());
    }

    #[inline]
    pub fn set_k_num(header: &mut [u8], k_num: u32) {
        header[9..][0..4].copy_from_slice(&k_num.to_le_bytes());
    }

    #[inline]
    pub fn set_seed(header: &mut [u8], seed: &[u8; 32]) {
        header[13..][0..32].copy_from_slice(seed);
    }

    fn validate_layout(bytes: &[u8]) -> Result<(), &'static str> {
        crate::hash::parse_header(bytes)?;
        Ok(())
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, &'static str> {
        Self::validate_layout(&bytes)?;
        Ok(Self {
            header_and_bits: BitMapStorage::Owned(bytes),
        })
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, &'static str> {
        Self::validate_layout(bytes)?;
        Ok(Self {
            header_and_bits: BitMapStorage::Owned(bytes.to_vec()),
        })
    }

    #[cfg(feature = "mmap")]
    pub fn from_mmap_mut(mmap: MmapMut) -> Result<Self, &'static str> {
        Self::validate_layout(&mmap)?;
        Ok(Self {
            header_and_bits: BitMapStorage::Mapped(mmap),
        })
    }

    #[cfg(feature = "mmap")]
    pub fn create_mmap_in_file(file: &File, len_bytes: usize) -> io::Result<Self> {
        let total_len = BITMAP_HEADER_SIZE
            .checked_add(len_bytes)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Too big"))?;
        let total_len_u64 = u64::try_from(total_len)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Too big"))?;
        let len_bytes_u64 = u64::try_from(len_bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Too big"))?;
        file.set_len(total_len_u64)?;

        let mut mmap = Self::map_file_mut(file)?;
        mmap.fill(0);
        let header = &mut mmap[0..BITMAP_HEADER_SIZE];
        Self::set_version(header, VERSION);
        Self::set_len_bytes(header, len_bytes_u64);
        Self::set_k_num(header, 0);

        Ok(Self {
            header_and_bits: BitMapStorage::Mapped(mmap),
        })
    }

    #[cfg(feature = "mmap")]
    #[allow(unsafe_code)]
    pub fn map_file_mut(file: &File) -> io::Result<MmapMut> {
        // SAFETY: The returned mapping owns its lifetime independently of `file`,
        // and we only expose it through safe slice APIs in this module.
        unsafe { MmapOptions::new().map_mut(file) }
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        self.bytes()
    }

    #[inline]
    pub fn into_bytes(self) -> Vec<u8> {
        match self.header_and_bits {
            BitMapStorage::Owned(bytes) => bytes,
            #[cfg(feature = "mmap")]
            BitMapStorage::Mapped(mmap) => mmap[..].to_vec(),
        }
    }

    #[inline]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.as_slice().to_vec()
    }

    pub fn get(&self, bit_offset: usize) -> bool {
        let byte_offset = bit_offset / 8;
        let bit_shift = bit_offset % 8;
        (self.bits()[byte_offset] & (1 << bit_shift)) != 0
    }

    pub fn set(&mut self, bit_offset: usize) {
        let byte_offset = bit_offset / 8;
        let bit_shift = bit_offset % 8;
        self.bits_mut()[byte_offset] |= 1 << bit_shift;
    }

    pub fn clear(&mut self) {
        for byte in self.bits_mut().iter_mut() {
            *byte = 0;
        }
    }

    pub fn set_all(&mut self) {
        for byte in self.bits_mut().iter_mut() {
            *byte = !0;
        }
    }

    pub fn any(&self) -> bool {
        self.bits().iter().any(|&byte| byte != 0)
    }

    pub fn len_bits(&self) -> u64 {
        u64::try_from(self.bits().len())
            .unwrap()
            .checked_mul(8)
            .unwrap()
    }

    #[doc(hidden)]
    pub fn realloc_large_heap_allocated_objects(self, f: fn(Vec<u8>) -> Vec<u8>) -> Self {
        let previous_bytes = self.into_bytes();
        let previous_len = previous_bytes.len();
        let header_and_bits = f(previous_bytes);
        assert_eq!(previous_len, header_and_bits.len());
        assert_eq!(
            Self::get_version(&header_and_bits[0..BITMAP_HEADER_SIZE]),
            VERSION
        );
        Self {
            header_and_bits: BitMapStorage::Owned(header_and_bits),
        }
    }

    pub fn flush(&self) -> io::Result<()> {
        match &self.header_and_bits {
            BitMapStorage::Owned(_) => Ok(()),
            #[cfg(feature = "mmap")]
            BitMapStorage::Mapped(mmap) => mmap.flush(),
        }
    }
}
