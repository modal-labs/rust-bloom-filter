// (C)opyleft 2013-2024 Frank Denis
// Licensed under the ICS license (https://opensource.org/licenses/ISC)

use std::convert::TryInto;
use std::num::NonZeroU64;

pub const VERSION: u8 = 1;
pub const HEADER_SIZE: usize = 1 + 8 + 4 + 32;

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

/// Validate a serialized bloom filter header and return its parameters.
///
/// On success returns `(bitmap_bits, k_num, seed)`.
pub(crate) fn parse(bytes: &[u8]) -> Result<(NonZeroU64, u32, [u8; 32]), &'static str> {
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

    let mut seed = [0u8; 32];
    seed.copy_from_slice(&header[13..][0..32]);
    let bitmap_bits =
        NonZeroU64::new((bits.len() as u64).checked_mul(8).unwrap()).unwrap();

    Ok((bitmap_bits, k_num, seed))
}
