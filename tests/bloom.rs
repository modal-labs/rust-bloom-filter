#[cfg(feature = "random")]
use bloomfilter::reexports::getrandom::getrandom;
use bloomfilter::Bloom;
#[cfg(feature = "mmap")]
use bloomfilter::ReadOnlyBloom;
#[cfg(feature = "mmap")]
use std::fs;
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
    let cloned_bytes = cloned.to_bytes();
    assert_eq!(original_bytes, cloned_bytes);
    assert!(original.check(&k));
    assert!(cloned.check(&k));
}

#[test]
fn bloom_test_flush_owned_noop() {
    let seed = [42u8; 32];
    let bloom = Bloom::<[u8]>::new_with_seed(16, 80, &seed).unwrap();
    bloom.flush().unwrap();
}

#[test]
fn bloom_test_check_via_from_bytes() {
    let seed = [12u8; 32];
    let key = b"from-bytes-key";

    let mut bloom = Bloom::new_with_seed(64, 80, &seed).unwrap();
    bloom.set(key);

    let loaded = Bloom::from_bytes(bloom.to_bytes()).unwrap();
    assert!(loaded.check(key));
    assert!(!loaded.check(b"not-in-filter!"));
}

#[test]
#[cfg(feature = "mmap")]
fn bloom_test_mmap_persist_and_reload() {
    let path = unique_temp_path("persist-and-reload");
    let seed = [7u8; 32];
    let key = b"persistent-key";

    {
        let mut bloom = Bloom::new_mmap_with_seed(&path, 64, 80, &seed).unwrap();
        assert!(!bloom.check(key));
        bloom.set(key);
        assert!(bloom.check(key));
        bloom.flush().unwrap();
    }

    {
        let bloom = Bloom::from_mmap_path(&path).unwrap();
        assert!(bloom.check(key));
        let from_bytes = Bloom::from_bytes(bloom.to_bytes()).unwrap();
        assert!(from_bytes.check(key));
    }

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
    let serialized = bloom.to_bytes();
    fs::write(&path, &serialized).unwrap();

    let mapped = Bloom::from_mmap_path(&path).unwrap();
    assert!(mapped.check(key));
    assert_eq!(mapped.to_bytes(), serialized);

    fs::remove_file(path).unwrap();
}

#[test]
#[cfg(feature = "mmap")]
fn bloom_test_mmap_rejects_invalid_file() {
    let path = unique_temp_path("invalid");
    fs::write(&path, [1u8, 2u8, 3u8]).unwrap();

    let err = Bloom::<[u8]>::from_mmap_path(&path).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

    fs::remove_file(path).unwrap();
}

#[test]
#[cfg(feature = "mmap")]
fn readonly_bloom_mmap_check() {
    let path = unique_temp_path("readonly-check");
    let seed = [9u8; 32];
    let key = b"readonly-key";

    {
        let mut bloom = Bloom::new_mmap_with_seed(&path, 64, 80, &seed).unwrap();
        bloom.set(key);
        bloom.flush().unwrap();
    }

    let bloom = ReadOnlyBloom::from_mmap_path_readonly(&path).unwrap();
    assert!(bloom.check(key));
    assert!(!bloom.check(b"missing-key!"));

    fs::remove_file(path).unwrap();
}

#[test]
#[cfg(feature = "mmap")]
fn readonly_bloom_mmap_load_serialized() {
    let path = unique_temp_path("readonly-serialized");
    let seed = [11u8; 32];
    let key = b"serialized-ro";

    let mut bloom = Bloom::new_with_seed(64, 80, &seed).unwrap();
    bloom.set(key);
    let serialized = bloom.to_bytes();
    fs::write(&path, &serialized).unwrap();

    let ro = ReadOnlyBloom::from_mmap_path_readonly(&path).unwrap();
    assert!(ro.check(key));
    assert_eq!(ro.to_bytes(), serialized);

    fs::remove_file(path).unwrap();
}

#[test]
#[cfg(feature = "mmap")]
fn readonly_bloom_mmap_rejects_invalid_file() {
    let path = unique_temp_path("readonly-invalid");
    fs::write(&path, [1u8, 2u8, 3u8]).unwrap();

    let err = ReadOnlyBloom::<[u8]>::from_mmap_path_readonly(&path).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

    fs::remove_file(path).unwrap();
}

/// Parse /proc/self/maps to find the permission flags for a given file path.
/// Returns entries like "r--s" (read-only shared) or "rw-s" (read-write shared).
#[cfg(all(feature = "mmap", target_os = "linux"))]
fn mmap_perms_for_path(path: &std::path::Path) -> Vec<String> {
    let canonical = path.canonicalize().unwrap();
    let maps = fs::read_to_string("/proc/self/maps").unwrap();
    maps.lines()
        .filter(|line| line.ends_with(canonical.to_str().unwrap()))
        .map(|line| {
            // Format: "addr-addr perms offset dev inode pathname"
            line.split_whitespace().nth(1).unwrap().to_string()
        })
        .collect()
}

#[test]
#[cfg(all(feature = "mmap", target_os = "linux"))]
fn readonly_bloom_mmap_is_prot_read() {
    let path = unique_temp_path("prot-read-check");
    let seed = [13u8; 32];
    let key = b"prot-test-key";

    // Create and populate a filter.
    {
        let mut bloom = Bloom::new_mmap_with_seed(&path, 64, 80, &seed).unwrap();
        bloom.set(key);
        bloom.flush().unwrap();
    }

    // Open as ReadOnlyBloom and verify the mapping flags via /proc/self/maps.
    let ro = ReadOnlyBloom::from_mmap_path_readonly(&path).unwrap();
    assert!(ro.check(key));

    let perms = mmap_perms_for_path(&path);
    assert!(!perms.is_empty(), "expected at least one mapping for {:?}", path);
    for perm in &perms {
        assert_eq!(&perm[..2], "r-", "expected read-only mapping, got {perm}");
        assert_eq!(&perm[3..], "s", "expected shared mapping, got {perm}");
    }

    drop(ro);
    fs::remove_file(path).unwrap();
}
