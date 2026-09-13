use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

use malus_wpe::{ProfileManager, WebError};

#[test]
fn test_namespace_validation() {
    // Valid namespaces
    assert!(ProfileManager::for_namespace("apple").is_ok());
    assert!(ProfileManager::for_namespace("provider_1").is_ok());
    assert!(ProfileManager::for_namespace("test-profile").is_ok());

    // Invalid namespaces
    assert!(ProfileManager::for_namespace("").is_err());
    assert!(ProfileManager::for_namespace("../traversal").is_err());
    assert!(ProfileManager::for_namespace("sub/dir").is_err());
    assert!(ProfileManager::for_namespace("invalid.dots").is_err());
    assert!(ProfileManager::for_namespace("space name").is_err());
    assert!(ProfileManager::for_namespace(&"a".repeat(70)).is_err());
}

#[test]
fn test_profile_permissions_0700() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir");
    let profile_dir = tmp.path().join("malus_profile_test");
    let pm = ProfileManager::with_custom_path(&profile_dir);

    pm.prepare_profile_dir()
        .expect("Failed to prepare profile dir");

    let meta = fs::metadata(&profile_dir).expect("Failed to read metadata");
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o700,
        "Profile directory must have POSIX 0700 permissions"
    );
}

#[test]
fn test_symlink_profile_rejected() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir");
    let real_dir = tmp.path().join("real");
    fs::create_dir(&real_dir).unwrap();

    let link_path = tmp.path().join("symlinked_profile");
    symlink(&real_dir, &link_path).unwrap();

    let pm = ProfileManager::with_custom_path(&link_path);
    let err = pm.prepare_profile_dir().unwrap_err();

    match err {
        WebError::Profile(msg) => {
            assert!(
                msg.contains("symlink"),
                "Expected symlink rejection message, got: {}",
                msg
            );
        }
        other => panic!("Expected Profile error, got: {:?}", other),
    }
}

#[test]
fn test_unmanaged_wipe_prohibited() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir");
    let custom_dir = tmp.path().join("unmanaged");
    let pm = ProfileManager::with_custom_path(&custom_dir);

    let err = pm.wipe_managed().unwrap_err();
    match err {
        WebError::Profile(msg) => {
            assert!(
                msg.contains("unmanaged"),
                "Expected refusal to wipe unmanaged path, got: {}",
                msg
            );
        }
        other => panic!("Expected Profile error, got: {:?}", other),
    }
}

#[test]
fn test_stale_singleton_lock_cleanup() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir");
    let profile_dir = tmp.path().join("lock_test");
    let pm = ProfileManager::with_custom_path(&profile_dir);
    pm.prepare_profile_dir().unwrap();

    let lock_path = profile_dir.join("SingletonLock");
    // Point lock to dead PID 999999
    symlink("host-999999", &lock_path).unwrap();
    assert!(lock_path.exists() || fs::symlink_metadata(&lock_path).is_ok());

    pm.cleanup_stale_locks()
        .expect("Dead lock cleanup should succeed");
    assert!(
        fs::symlink_metadata(&lock_path).is_err(),
        "Stale lock should have been unlinked"
    );
}
