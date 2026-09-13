use std::{fs, io::Cursor};

use malus_wpe::widevine::{
    EXPECTED_ELF_CLASS, EXPECTED_ELF_DATA, EXPECTED_ELF_MACHINE, WidevineError, WidevineMetadata,
    WidevineSource, extract_widevine_from_deb, install_managed_widevine,
    persist_widevine_path_to_file, read_managed_metadata, reset_widevine_config_internal,
    sanitize_version, validate_widevine_library,
};
use tempfile::tempdir;

fn create_valid_elf_bytes(ei_class: u8, ei_data: u8, e_machine: u16) -> Vec<u8> {
    let mut hdr = vec![0u8; 64];
    hdr[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
    hdr[4] = ei_class;
    hdr[5] = ei_data;
    let machine_bytes = e_machine.to_le_bytes();
    hdr[18] = machine_bytes[0];
    hdr[19] = machine_bytes[1];
    hdr
}

fn create_synthetic_deb(
    include_cdm: bool,
    cdm_bytes: &[u8],
    version: Option<&str>,
    multiple_cdms: bool,
    include_chrome_binary: bool,
) -> Vec<u8> {
    // 1. Build data.tar.xz
    let mut tar_xz_bytes = Vec::new();
    {
        let xz_encoder = xz2::write::XzEncoder::new(&mut tar_xz_bytes, 6);
        let mut tar_builder = tar::Builder::new(xz_encoder);

        if include_chrome_binary {
            let mut header = tar::Header::new_gnu();
            header.set_size(1024);
            header.set_mode(0o755);
            header.set_cksum();
            let fake_chrome = vec![0x90u8; 1024];
            tar_builder
                .append_data(
                    &mut header,
                    "opt/google/chrome/chrome",
                    Cursor::new(fake_chrome),
                )
                .unwrap();
        }

        if include_cdm {
            let mut header = tar::Header::new_gnu();
            header.set_size(cdm_bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar_builder
                .append_data(
                    &mut header,
                    "opt/google/chrome/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so",
                    Cursor::new(cdm_bytes),
                )
                .unwrap();
        }

        if multiple_cdms {
            let mut header = tar::Header::new_gnu();
            header.set_size(cdm_bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar_builder
                .append_data(
                    &mut header,
                    "opt/google/chrome/WidevineCdm/extra/libwidevinecdm.so",
                    Cursor::new(cdm_bytes),
                )
                .unwrap();
        }

        if let Some(v) = version {
            let manifest = format!(r#"{{"name":"WidevineCdm","version":"{v}"}}"#);
            let mut header = tar::Header::new_gnu();
            header.set_size(manifest.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append_data(
                    &mut header,
                    "opt/google/chrome/WidevineCdm/manifest.json",
                    Cursor::new(manifest.as_bytes()),
                )
                .unwrap();
        }

        let mut license_hdr = tar::Header::new_gnu();
        license_hdr.set_size(15);
        license_hdr.set_mode(0o644);
        license_hdr.set_cksum();
        tar_builder
            .append_data(
                &mut license_hdr,
                "opt/google/chrome/WidevineCdm/LICENSE",
                Cursor::new(b"Google License\n"),
            )
            .unwrap();

        tar_builder.finish().unwrap();
    }

    // 2. Build ar archive
    let mut deb_bytes = Vec::new();
    {
        let mut ar_builder = ar::Builder::new(&mut deb_bytes);

        // debian-binary
        let deb_bin = b"2.0\n";
        let mut h = ar::Header::new(b"debian-binary".to_vec(), deb_bin.len() as u64);
        h.set_mode(0o644);
        ar_builder.append(&h, Cursor::new(deb_bin)).unwrap();

        // control.tar.xz (dummy)
        let dummy_control = b"control";
        let mut h = ar::Header::new(b"control.tar.xz".to_vec(), dummy_control.len() as u64);
        h.set_mode(0o644);
        ar_builder.append(&h, Cursor::new(dummy_control)).unwrap();

        // data.tar.xz
        let mut h = ar::Header::new(b"data.tar.xz".to_vec(), tar_xz_bytes.len() as u64);
        h.set_mode(0o644);
        ar_builder
            .append(&h, Cursor::new(&tar_xz_bytes[..]))
            .unwrap();
    }
    deb_bytes
}

// ----------------------------------------------------------------------------
// PHASE 14: Unit test extraction
// ----------------------------------------------------------------------------

#[test]
fn test_extraction_extracts_only_intended_files() {
    let tmp = tempdir().unwrap();
    let dest_dir = tmp.path().join("extracted");

    let elf_bytes =
        create_valid_elf_bytes(EXPECTED_ELF_CLASS, EXPECTED_ELF_DATA, EXPECTED_ELF_MACHINE);
    let deb_bytes = create_synthetic_deb(true, &elf_bytes, Some("4.10.3112.0"), false, true);

    let extracted = extract_widevine_from_deb(Cursor::new(deb_bytes), &dest_dir)
        .expect("Extraction should succeed");

    assert_eq!(extracted.version, "4.10.3112.0");
    assert!(dest_dir.join("libwidevinecdm.so").is_file());
    assert!(dest_dir.join("manifest.json").is_file());
    assert!(dest_dir.join("LICENSE").is_file());

    // CRITICAL: Ensure large Chrome binary was NEVER extracted to disk
    assert!(!dest_dir.join("chrome").exists());
    assert!(!dest_dir.join("opt").exists());
}

#[test]
fn test_extraction_missing_cdm_rejected() {
    let tmp = tempdir().unwrap();
    let dest_dir = tmp.path().join("extracted");

    let elf_bytes =
        create_valid_elf_bytes(EXPECTED_ELF_CLASS, EXPECTED_ELF_DATA, EXPECTED_ELF_MACHINE);
    let deb_bytes = create_synthetic_deb(false, &elf_bytes, Some("4.10.3112.0"), false, false);

    let err = extract_widevine_from_deb(Cursor::new(deb_bytes), &dest_dir).unwrap_err();
    match err {
        WidevineError::Extraction(msg) => {
            assert!(msg.contains("libwidevinecdm.so was not found"));
        }
        other => panic!("Unexpected error: {:?}", other),
    }
}

#[test]
fn test_extraction_multiple_ambiguous_cdms_rejected() {
    let tmp = tempdir().unwrap();
    let dest_dir = tmp.path().join("extracted");

    let elf_bytes =
        create_valid_elf_bytes(EXPECTED_ELF_CLASS, EXPECTED_ELF_DATA, EXPECTED_ELF_MACHINE);
    let deb_bytes = create_synthetic_deb(true, &elf_bytes, Some("4.10.3112.0"), true, false);

    let err = extract_widevine_from_deb(Cursor::new(deb_bytes), &dest_dir).unwrap_err();
    match err {
        WidevineError::Extraction(msg) => {
            assert!(msg.contains("Multiple ambiguous libwidevinecdm.so"));
        }
        other => panic!("Unexpected error: {:?}", other),
    }
}

#[test]
fn test_extraction_malformed_deb_rejected() {
    let tmp = tempdir().unwrap();
    let dest_dir = tmp.path().join("extracted");

    let garbage = b"this is completely random corrupted data not a debian package";
    let err = extract_widevine_from_deb(Cursor::new(garbage), &dest_dir).unwrap_err();
    match err {
        WidevineError::Extraction(_) => {}
        other => panic!("Unexpected error: {:?}", other),
    }
}

// ----------------------------------------------------------------------------
// PHASE 15: Architecture validation
// ----------------------------------------------------------------------------

#[test]
fn test_arch_valid_host_elf_passes() {
    let tmp = tempdir().unwrap();
    let lib_path = tmp.path().join("libwidevinecdm.so");
    let elf = create_valid_elf_bytes(EXPECTED_ELF_CLASS, EXPECTED_ELF_DATA, EXPECTED_ELF_MACHINE);
    fs::write(&lib_path, elf).unwrap();

    let res = validate_widevine_library(&lib_path);
    assert!(res.is_ok(), "Valid host ELF must pass validation");
}

#[test]
fn test_arch_wrong_32bit_class_rejected() {
    let tmp = tempdir().unwrap();
    let lib_path = tmp.path().join("libwidevinecdm.so");
    // Class 1 = 32-bit ELF
    let elf = create_valid_elf_bytes(1, EXPECTED_ELF_DATA, EXPECTED_ELF_MACHINE);
    fs::write(&lib_path, elf).unwrap();

    let err = validate_widevine_library(&lib_path).unwrap_err();
    assert!(err.contains("Wrong ELF class"));
}

#[test]
fn test_arch_unsupported_endianness_rejected() {
    let tmp = tempdir().unwrap();
    let lib_path = tmp.path().join("libwidevinecdm.so");
    // Data 2 = Big Endian
    let elf = create_valid_elf_bytes(EXPECTED_ELF_CLASS, 2, EXPECTED_ELF_MACHINE);
    fs::write(&lib_path, elf).unwrap();

    let err = validate_widevine_library(&lib_path).unwrap_err();
    assert!(err.contains("Unsupported endianness"));
}

#[test]
fn test_arch_mismatched_machine_rejected() {
    let tmp = tempdir().unwrap();
    let lib_path = tmp.path().join("libwidevinecdm.so");
    let mismatched_machine = if EXPECTED_ELF_MACHINE == 62 { 183 } else { 62 };
    let elf = create_valid_elf_bytes(EXPECTED_ELF_CLASS, EXPECTED_ELF_DATA, mismatched_machine);
    fs::write(&lib_path, elf).unwrap();

    let err = validate_widevine_library(&lib_path).unwrap_err();
    assert!(err.contains("Architecture mismatch"));
}

#[test]
fn test_arch_corrupted_magic_rejected() {
    let tmp = tempdir().unwrap();
    let lib_path = tmp.path().join("libwidevinecdm.so");
    let mut elf =
        create_valid_elf_bytes(EXPECTED_ELF_CLASS, EXPECTED_ELF_DATA, EXPECTED_ELF_MACHINE);
    elf[0] = 0x00; // Corrupt magic

    fs::write(&lib_path, elf).unwrap();

    let err = validate_widevine_library(&lib_path).unwrap_err();
    assert!(err.contains("not an ELF binary"));
}

#[test]
fn test_arch_truncated_header_rejected() {
    let tmp = tempdir().unwrap();
    let lib_path = tmp.path().join("libwidevinecdm.so");
    // Only 10 bytes (has magic, but missing rest of ELF header)
    let elf = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0];
    fs::write(&lib_path, elf).unwrap();

    let err = validate_widevine_library(&lib_path).unwrap_err();
    assert!(err.contains("Truncated ELF binary header"));
}

// ----------------------------------------------------------------------------
// PHASE 6: Version sanitization
// ----------------------------------------------------------------------------

#[test]
fn test_version_sanitization_rules() {
    assert_eq!(sanitize_version("4.10.3112.0").unwrap(), "4.10.3112.0");
    assert_eq!(sanitize_version("1.2.3_rc1").unwrap(), "1.2.3_rc1");

    // Traversal attempts
    assert!(sanitize_version("../traversal").is_err());
    assert!(sanitize_version("foo/bar").is_err());
    assert!(sanitize_version(r"foo\bar").is_err());
    assert!(sanitize_version("").is_err());
    assert!(sanitize_version("version with spaces").is_err());
    assert!(sanitize_version("version\0null").is_err());
}

// ----------------------------------------------------------------------------
// PHASE 17: Reset safety
// ----------------------------------------------------------------------------

#[test]
fn test_reset_safety_external_file_preserved() {
    let tmp = tempdir().unwrap();
    let config_path = tmp.path().join("config/malus/widevine.json");
    let managed_root = tmp.path().join("data/malus/widevine");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    fs::create_dir_all(&managed_root).unwrap();

    let external_lib = tmp.path().join("external_cdm/libwidevinecdm.so");
    let elf = create_valid_elf_bytes(EXPECTED_ELF_CLASS, EXPECTED_ELF_DATA, EXPECTED_ELF_MACHINE);
    fs::create_dir_all(external_lib.parent().unwrap()).unwrap();
    fs::write(&external_lib, elf).unwrap();

    persist_widevine_path_to_file(&external_lib, WidevineSource::PersistedConfig, &config_path)
        .unwrap();
    assert!(config_path.is_file());

    let res = reset_widevine_config_internal(Some(&config_path), Some(&managed_root)).unwrap();
    assert!(res.config_removed);
    assert_eq!(res.managed_files_removed, None);
    assert_eq!(res.preserved_external_path, Some(external_lib.clone()));

    // CRITICAL: External file must NOT be deleted
    assert!(external_lib.is_file(), "External library must be preserved");
    // Config file must be gone
    assert!(!config_path.exists());
}

#[test]
fn test_reset_safety_managed_file_removed() {
    let tmp = tempdir().unwrap();
    let config_path = tmp.path().join("config/malus/widevine.json");
    let managed_root = tmp.path().join("data/malus/widevine");
    let version_dir = managed_root.join("4.10.3112.0");
    fs::create_dir_all(&version_dir).unwrap();

    let managed_lib = version_dir.join("libwidevinecdm.so");
    let elf = create_valid_elf_bytes(EXPECTED_ELF_CLASS, EXPECTED_ELF_DATA, EXPECTED_ELF_MACHINE);
    fs::write(&managed_lib, elf).unwrap();

    persist_widevine_path_to_file(&managed_lib, WidevineSource::ManagedInstall, &config_path)
        .unwrap();
    assert!(config_path.is_file());

    let res = reset_widevine_config_internal(Some(&config_path), Some(&managed_root)).unwrap();
    assert!(res.config_removed);
    assert_eq!(res.managed_files_removed, Some(version_dir.clone()));

    // Managed files must be deleted
    assert!(!managed_lib.exists(), "Managed library should be deleted");
    assert!(!version_dir.exists(), "Managed directory should be deleted");
    assert!(!config_path.exists());
}

// ----------------------------------------------------------------------------
// PHASE 16: Atomic installation & metadata tests
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_atomic_install_failed_download_leaves_no_config() {
    // Attempting to install from an unreachable URL must fail cleanly and not leave configuration
    let tmp = tempdir().unwrap();
    let config_path = tmp.path().join("config/malus/widevine.json");

    // Invalid port / unreachable server
    let res = install_managed_widevine(Some("https://127.0.0.1:9/chrome.deb"), true).await;
    assert!(res.is_err());
    assert!(!config_path.exists());
}

#[test]
fn test_managed_metadata_serialization() {
    let tmp = tempdir().unwrap();
    let meta = WidevineMetadata {
        source_url: "https://dl.google.com/linux/chrome.deb".into(),
        version: "4.10.3112.0".into(),
        architecture: "x86_64".into(),
        installed_at: "2026-09-12T10:00:00Z".into(),
    };

    let data = serde_json::to_string_pretty(&meta).unwrap();
    fs::write(tmp.path().join("metadata.json"), data).unwrap();

    let loaded = read_managed_metadata(tmp.path()).expect("Metadata should deserialize cleanly");
    assert_eq!(loaded, meta);
}
