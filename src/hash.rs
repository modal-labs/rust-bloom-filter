use std::hash::{Hash, Hasher};

use siphasher::sip::SipHasher13;

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
