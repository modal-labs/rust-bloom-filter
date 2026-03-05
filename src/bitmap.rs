pub const VERSION: u8 = 1;
pub const BITMAP_HEADER_SIZE: usize = 1 + 8 + 4 + 32;

#[inline]
pub(crate) fn set_version(header: &mut [u8], version: u8) {
    header[0] = version;
}

#[inline]
pub(crate) fn set_len_bytes(header: &mut [u8], len_bytes: u64) {
    header[1..][0..8].copy_from_slice(&len_bytes.to_le_bytes());
}

#[inline]
pub(crate) fn set_k_num(header: &mut [u8], k_num: u32) {
    header[9..][0..4].copy_from_slice(&k_num.to_le_bytes());
}

#[inline]
pub(crate) fn set_seed(header: &mut [u8], seed: &[u8; 32]) {
    header[13..][0..32].copy_from_slice(seed);
}

#[inline]
pub(crate) fn get_version(header: &[u8]) -> u8 {
    header[0]
}
