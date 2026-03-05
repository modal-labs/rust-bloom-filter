use crate::bitmap::{BITMAP_HEADER_SIZE, VERSION};
use super::{Sealed, Storage};

/// Heap-allocated storage. This is the default storage for [`Bloom`](crate::Bloom).
pub struct OwnedStorage(pub(crate) Vec<u8>);

impl Sealed for OwnedStorage {}

impl Storage for OwnedStorage {
    fn bytes(&self) -> &[u8] {
        &self.0
    }
    fn into_bytes(self) -> Vec<u8> {
        self.0
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

    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}
