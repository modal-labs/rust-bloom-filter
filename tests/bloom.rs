#[cfg(feature = "random")]
use bloomfilter::reexports::getrandom::getrandom;
use bloomfilter::Bloom;
#[cfg(feature = "mmap")]
use bloomfilter::MmapBloom;
#[cfg(feature = "mmap")]
use std::fs;
#[cfg(feature = "mmap")]
use std::io;
#[cfg(feature = "mmap")]
use std::path::PathBuf;
#[cfg(feature = "mmap")]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "mmap")]
fn unique_temp_path(test_name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!(
        "rust-bloom-filter-{test_name}-{}-{now}.bin",
        std::process::id()
    ));
    path
}

#[test]
#[cfg(feature = "random")]
fn bloom_test_set() {
    let mut bloom = Bloom::new(10, 80).unwrap();
    let mut k = vec![0u8, 16];
    getrandom(&mut k).unwrap();
    assert!(!bloom.check(&k));
    bloom.set(&k);
    assert!(bloom.check(&k));
}

#[test]
#[cfg(feature = "random")]
fn bloom_test_check_and_set() {
    let mut bloom = Bloom::new(10, 80).unwrap();
    let mut k = vec![0u8, 16];
    getrandom(&mut k).unwrap();
    assert!(!bloom.check_and_set(&k));
    assert!(bloom.check_and_set(&k));
}

#[test]
#[cfg(feature = "random")]
fn bloom_test_clear() {
    let mut bloom = Bloom::new(10, 80).unwrap();
    let mut k = vec![0u8, 16];
    getrandom(&mut k).unwrap();
    bloom.set(&k);
    assert!(bloom.check(&k));
    bloom.clear();
    assert!(!bloom.check(&k));
}

#[test]
#[cfg(feature = "random")]
fn bloom_test_load() {
    let mut original = Bloom::new(10, 80).unwrap();
    let mut k = vec![0u8, 16];
    getrandom(&mut k).unwrap();
    original.set(&k);
    assert!(original.check(&k));

    let original_bytes = original.as_slice();
    let cloned = Bloom::from_slice(original_bytes).unwrap();
    let cloned_bytes = cloned.as_slice().to_vec();
    assert_eq!(original_bytes, cloned_bytes);
    assert!(original.check(&k));
    assert!(cloned.check(&k));
}

#[test]
fn bloom_test_check_via_from_bytes() {
    let seed = [12u8; 32];
    let key = b"from-bytes-key";

    let mut bloom = Bloom::new_with_seed(64, 80, &seed).unwrap();
    bloom.set(key);

    let loaded = Bloom::from_bytes(bloom.as_slice().to_vec()).unwrap();
    assert!(loaded.check(key));
    assert!(!loaded.check(b"not-in-filter!"));
}

#[test]
fn bloom_test_rejects_zero_bitmap_size() {
    let seed = [1u8; 32];
    assert!(Bloom::<[u8], Vec<u8>>::new_with_seed(0, 80, &seed).is_err());
}

#[test]
fn bloom_test_rejects_zero_items_count() {
    let seed = [1u8; 32];
    assert!(Bloom::<[u8], Vec<u8>>::new_with_seed(64, 0, &seed).is_err());
}

#[test]
fn bloom_test_rejects_empty_bitmap_in_serialized_data() {
    // Craft a valid header with len_bytes=0 (just the 45-byte header, no bitmap).
    let mut bytes = vec![0u8; 45];
    bytes[0] = 1; // version
    // len_bytes = 0 (already zero)
    bytes[9..13].copy_from_slice(&1u32.to_le_bytes()); // k_num = 1
    // seed is all zeros (fine)
    assert!(Bloom::<[u8], Vec<u8>>::from_bytes(bytes).is_err());
}

#[test]
fn bloom_test_len_and_k_num() {
    let seed = [1u8; 32];
    let bloom = Bloom::<[u8], Vec<u8>>::new_with_seed(64, 80, &seed).unwrap();
    assert_eq!(bloom.len(), 64 * 8);
    assert!(bloom.number_of_hash_functions() >= 1);
}

#[test]
fn bloom_test_seed_roundtrip() {
    let seed = [99u8; 32];
    let bloom = Bloom::<[u8], Vec<u8>>::new_with_seed(64, 80, &seed).unwrap();
    assert_eq!(bloom.seed(), seed);
}

/// Golden bytes for the v1 binary format, produced with:
///   Bloom::<str>::new_with_seed(32, 100, &[42u8; 32])
///   .set("hello"), .set("world"), .set("bloom filter")
const GOLDEN_BYTES: [u8; 77] = [
    0x01, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x2a, 0x2a, 0x2a,
    0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a,
    0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00,
];

#[test]
fn bloom_test_golden_format_compatibility() {
    let bloom: Bloom<str, Vec<u8>> = Bloom::from_slice(&GOLDEN_BYTES).unwrap();
    assert_eq!(bloom.seed(), [42u8; 32]);
    assert_eq!(bloom.len(), 256);
    assert_eq!(bloom.number_of_hash_functions(), 2);
    assert!(bloom.check("hello"));
    assert!(bloom.check("world"));
    assert!(bloom.check("bloom filter"));
    assert!(!bloom.check("missing"));
    assert!(!bloom.check("nope"));
}

#[test]
fn bloom_test_golden_produces_identical_bytes() {
    let seed = [42u8; 32];
    let mut bloom: Bloom<str, Vec<u8>> = Bloom::new_with_seed(32, 100, &seed).unwrap();
    bloom.set("hello");
    bloom.set("world");
    bloom.set("bloom filter");
    assert_eq!(bloom.as_slice(), &GOLDEN_BYTES);
}

#[test]
fn bloom_test_is_empty_and_fill() {
    let seed = [2u8; 32];
    let mut bloom = Bloom::<[u8], Vec<u8>>::new_with_seed(64, 80, &seed).unwrap();
    assert!(bloom.is_empty());

    bloom.set(b"hello");
    assert!(!bloom.is_empty());

    bloom.fill();
    assert!(bloom.check(b"anything"));

    bloom.clear();
    assert!(bloom.is_empty());
}

#[test]
#[cfg(feature = "mmap")]
fn bloom_test_mmap_persist_and_reload() {
    let path = unique_temp_path("persist-and-reload");
    let seed = [7u8; 32];
    let key = b"persistent-key";

    let mut bloom = Bloom::new_with_seed(64, 80, &seed).unwrap();
    bloom.set(key);
    fs::write(&path, bloom.as_slice().to_vec()).unwrap();

    let ro: MmapBloom<[u8]> = Bloom::from_path(&path).unwrap();
    assert!(ro.check(key));
    let from_bytes = Bloom::from_slice(ro.as_slice()).unwrap();
    assert!(from_bytes.check(key));

    fs::remove_file(path).unwrap();
}

#[test]
#[cfg(feature = "mmap")]
fn bloom_test_mmap_load_serialized_filter() {
    let path = unique_temp_path("load-serialized");
    let seed = [5u8; 32];
    let key = b"serialized-key";

    let mut bloom = Bloom::new_with_seed(64, 80, &seed).unwrap();
    bloom.set(key);
    let serialized = bloom.as_slice().to_vec();
    fs::write(&path, &serialized).unwrap();

    let mapped: MmapBloom<[u8]> = Bloom::from_path(&path).unwrap();
    assert!(mapped.check(key));
    assert_eq!(mapped.as_slice(), serialized);

    fs::remove_file(path).unwrap();
}

#[test]
#[cfg(feature = "mmap")]
fn bloom_test_mmap_rejects_invalid_file() {
    let path = unique_temp_path("invalid");
    fs::write(&path, [1u8, 2u8, 3u8]).unwrap();

    let err = MmapBloom::<[u8]>::from_path(&path).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);

    fs::remove_file(path).unwrap();
}

#[test]
#[cfg(all(feature = "mmap", target_os = "linux"))]
fn bloom_test_mmap_is_prot_read() {
    let path = unique_temp_path("prot-read-check");
    let seed = [13u8; 32];
    let key = b"prot-test-key";

    // Create and populate a filter, write to file.
    let mut bloom = Bloom::new_with_seed(64, 80, &seed).unwrap();
    bloom.set(key);
    fs::write(&path, bloom.as_slice().to_vec()).unwrap();

    // Open as read-only mmap and verify the mapping flags via /proc/self/maps.
    let ro: MmapBloom<[u8]> = Bloom::from_path(&path).unwrap();
    assert!(ro.check(key));

    let canonical = path.canonicalize().unwrap();
    let maps = fs::read_to_string("/proc/self/maps").unwrap();
    // Format: "addr-addr perms offset dev inode pathname"
    let perm = maps
        .lines()
        .find(|line| line.ends_with(canonical.to_str().unwrap()))
        .expect("expected a mapping in /proc/self/maps")
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    assert_eq!(&perm[..2], "r-", "expected read-only mapping, got {perm}");
    assert_eq!(&perm[3..], "s", "expected shared mapping, got {perm}");

    drop(ro);
    fs::remove_file(path).unwrap();
}
