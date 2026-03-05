use std::convert::TryInto;
use std::hash::{Hash, Hasher};

use siphasher::sip::SipHasher13;

use crate::bitmap::{BITMAP_HEADER_SIZE, VERSION};

/// Validate a serialized bloom filter and return its parameters.
///
/// On success returns `(bitmap_bits, k_num, sips)`.
pub(crate) fn parse_header(bytes: &[u8]) -> Result<(u64, u32, [SipHasher13; 2]), &'static str> {
    if bytes.len() < BITMAP_HEADER_SIZE {
        return Err("Invalid size");
    }
    let header = &bytes[0..BITMAP_HEADER_SIZE];
    let bits = &bytes[BITMAP_HEADER_SIZE..];

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

    let mut seed = [0u8; 32];
    seed.copy_from_slice(&header[13..][0..32]);
    let sips = sips_from_seed(&seed);
    let bitmap_bits = (bits.len() as u64).checked_mul(8).unwrap();

    Ok((bitmap_bits, k_num, sips))
}

pub(crate) fn sips_from_seed(seed: &[u8; 32]) -> [SipHasher13; 2] {
    let mut k1 = [0u8; 16];
    let mut k2 = [0u8; 16];
    k1.copy_from_slice(&seed[0..16]);
    k2.copy_from_slice(&seed[16..32]);
    [
        SipHasher13::new_with_key(&k1),
        SipHasher13::new_with_key(&k2),
    ]
}

pub(crate) fn bloom_hash<T: Hash + ?Sized>(
    sips: &[SipHasher13; 2],
    hashes: &mut [u64; 2],
    item: &T,
    k_i: u32,
) -> u64 {
    if k_i < 2 {
        let mut sip = sips[k_i as usize];
        item.hash(&mut sip);
        let hash = sip.finish();
        hashes[k_i as usize] = hash;
        hash
    } else {
        (hashes[0]).wrapping_add((k_i as u64).wrapping_mul(hashes[1]))
            % 0xFFFF_FFFF_FFFF_FFC5u64 // largest u64 prime
    }
}

pub(crate) fn check<T: Hash + ?Sized>(
    sips: &[SipHasher13; 2],
    bits: &[u8],
    bitmap_bits: u64,
    k_num: u32,
    item: &T,
) -> bool {
    let mut hashes = [0u64, 0u64];
    for k_i in 0..k_num {
        let bit_offset = (bloom_hash(sips, &mut hashes, item, k_i) % bitmap_bits) as usize;
        let byte_offset = bit_offset / 8;
        let bit_shift = bit_offset % 8;
        if (bits[byte_offset] & (1 << bit_shift)) == 0 {
            return false;
        }
    }
    true
}
