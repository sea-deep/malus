//! Tests for WPE runtime version manifest and patch integrity.

use std::fs;
use std::path::Path;

#[test]
fn test_wpe_versions_manifest_integrity() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/wpe-runtime/versions.env");

    assert!(manifest_path.is_file(), "versions.env must exist");
    let content = fs::read_to_string(&manifest_path).expect("Failed to read versions.env");

    // Required variables
    let required_keys = [
        "WEBKIT_VERSION",
        "WEBKIT_URL",
        "WEBKIT_SHA256",
        "LIBWPE_VERSION",
        "LIBWPE_URL",
        "LIBWPE_SHA256",
        "WPEBACKEND_FDO_VERSION",
        "WPEBACKEND_FDO_URL",
        "WPEBACKEND_FDO_SHA256",
        "GPERF_SHA256",
        "UNIFDEF_SHA256",
        "RUBY_ERB_SHA256",
        "RUBY_GETOPTLONG_SHA256",
        "RUBY_BASE64_SHA256",
        "RUBY_MUTEX_M_SHA256",
    ];

    for key in required_keys {
        assert!(
            content.contains(&format!("{key}=")),
            "versions.env missing key: {key}"
        );
    }
}

#[test]
fn test_wpe_patches_exist_and_valid() {
    let patches_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("native/webkit/patches");

    assert!(patches_dir.is_dir(), "patches directory must exist");

    let required_patches = [
        "0001-glib-process-executable-path.patch",
        "0002-coordinated-scrolling-guards.patch",
        "0003-tooling-backends-build.patch",
    ];

    for patch_name in required_patches {
        let p = patches_dir.join(patch_name);
        assert!(p.is_file(), "Patch file missing: {}", p.display());
        let meta = fs::metadata(&p).unwrap();
        assert!(meta.len() > 100, "Patch file is suspiciously small");
    }

    let readme = patches_dir.join("README.md");
    assert!(readme.is_file(), "patches/README.md must exist");
}
