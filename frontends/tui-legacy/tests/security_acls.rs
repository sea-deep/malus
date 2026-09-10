use malus::engine::profile::ProfileManager;
use std::os::unix::fs::PermissionsExt;

#[test]
fn test_profile_security_mode_0700() {
    let temp_dir = std::env::temp_dir().join(format!("malus_sec_test_{}", std::process::id()));
    let profile = ProfileManager::with_custom_path(&temp_dir);

    // Initializing profile directory must enforce 0700
    profile
        .prepare_profile_dir()
        .expect("Failed to initialize profile");

    let metadata = std::fs::metadata(&temp_dir).expect("Failed to get profile metadata");
    let mode = metadata.permissions().mode() & 0o777;

    assert_eq!(
        mode, 0o700,
        "OWASP A01 Violation: Profile directory must have strict 0700 permissions, got {:o}",
        mode
    );

    // Test wipe session
    profile.wipe_session().expect("Failed to wipe session");
    assert!(
        !temp_dir.exists(),
        "Profile directory must be erased after wipe_session"
    );
}
