#[cfg(feature = "random")]
use bloomfilter::reexports::getrandom::getrandom;
use bloomfilter::Bloom;
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
fn bloom_test_mmap_clone_preserves_mapping() {
    let path = unique_temp_path("clone-preserves-mapping");
    let seed = [9u8; 32];
    let key_from_original = b"original-key".to_vec();
    let key_from_clone = b"clone-key".to_vec();

    {
        let mut original = Bloom::new_mmap_with_seed(&path, 64, 80, &seed).unwrap();
        original.set(&key_from_original);
        original.flush().unwrap();

        let mut cloned = original.clone();
        assert!(cloned.check(&key_from_original));
        cloned.set(&key_from_clone);
        cloned.flush().unwrap();
    }

    {
        let reopened = Bloom::from_mmap_path(&path).unwrap();
        assert!(reopened.check(&key_from_original));
        assert!(reopened.check(&key_from_clone));
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
