use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use malus_wpe::widevine::{
    KnownCandidate, WidevineError, WidevineSource, discover_widevine_internal,
    validate_widevine_library,
};
use tempfile::tempdir;

fn create_mock_elf(dir: &Path, filename: &str) -> PathBuf {
    let path = dir.join(filename);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = File::create(&path).unwrap();
    let mut hdr = [0u8; 64];
    hdr[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
    hdr[4] = malus_wpe::widevine::EXPECTED_ELF_CLASS;
    hdr[5] = malus_wpe::widevine::EXPECTED_ELF_DATA;
    let machine_bytes = malus_wpe::widevine::EXPECTED_ELF_MACHINE.to_le_bytes();
    hdr[18] = machine_bytes[0];
    hdr[19] = machine_bytes[1];
    f.write_all(&hdr).unwrap();
    path.canonicalize().unwrap_or(path)
}

#[test]
fn test_explicit_env_path_file_wins() {
    let tmp = tempdir().unwrap();
    let mock_file = create_mock_elf(tmp.path(), "custom_widevine/libwidevinecdm.so");

    let res = discover_widevine_internal(Some(&mock_file), None, &[])
        .expect("Explicit env path to file must succeed");

    assert_eq!(res.source, WidevineSource::EnvOverride);
    assert_eq!(res.library_path, mock_file);
}

#[test]
fn test_explicit_env_path_dir_wins() {
    let tmp = tempdir().unwrap();
    let cdm_dir = tmp.path().join("ChromeWidevine/WidevineCdm");
    let mock_file = create_mock_elf(&cdm_dir, "_platform_specific/linux_x64/libwidevinecdm.so");

    let res = discover_widevine_internal(Some(&cdm_dir), None, &[])
        .expect("Explicit env path to directory must resolve library and succeed");

    assert_eq!(res.source, WidevineSource::EnvOverride);
    assert_eq!(res.library_path, mock_file);
    assert_eq!(res.directory, cdm_dir);
}

#[test]
fn test_invalid_env_path_returns_error_and_does_not_fall_back() {
    let tmp = tempdir().unwrap();
    let valid_candidate = create_mock_elf(tmp.path(), "known/libwidevinecdm.so");
    let candidates = vec![KnownCandidate {
        path: valid_candidate,
        source: WidevineSource::GoogleChrome,
    }];

    let nonexistent = Path::new("/definitely/missing/libwidevinecdm.so");
    let res = discover_widevine_internal(Some(nonexistent), None, &candidates);

    match res {
        Err(WidevineError::InvalidPath(msg)) => {
            assert!(
                msg.contains("MALUS_WIDEVINE_PATH is set to"),
                "Expected clear error message about invalid MALUS_WIDEVINE_PATH, got: {msg}"
            );
        }
        other => panic!("Expected InvalidPath error for invalid env path, got: {other:?}"),
    }
}

#[test]
fn test_persisted_config_wins_over_known_candidates() {
    let tmp = tempdir().unwrap();
    let persisted_file = create_mock_elf(tmp.path(), "persisted/libwidevinecdm.so");
    let known_file = create_mock_elf(tmp.path(), "known/libwidevinecdm.so");

    let candidates = vec![KnownCandidate {
        path: known_file,
        source: WidevineSource::Chromium,
    }];

    let res = discover_widevine_internal(None, Some(&persisted_file), &candidates)
        .expect("Persisted config must succeed");

    assert_eq!(res.source, WidevineSource::PersistedConfig);
    assert_eq!(res.library_path, persisted_file);
}

#[test]
fn test_stale_persisted_config_falls_back_to_known_candidates() {
    let tmp = tempdir().unwrap();
    let known_file = create_mock_elf(tmp.path(), "known/libwidevinecdm.so");

    let candidates = vec![KnownCandidate {
        path: known_file.clone(),
        source: WidevineSource::Brave,
    }];

    let stale_path = tmp.path().join("missing/libwidevinecdm.so");
    let res = discover_widevine_internal(None, Some(&stale_path), &candidates)
        .expect("Stale persisted config must fall back to known candidates");

    assert_eq!(res.source, WidevineSource::Brave);
    assert_eq!(res.library_path, known_file);
}

#[test]
fn test_known_candidate_selection_priority() {
    let tmp = tempdir().unwrap();
    let file1 = create_mock_elf(tmp.path(), "chrome/libwidevinecdm.so");
    let file2 = create_mock_elf(tmp.path(), "brave/libwidevinecdm.so");

    let candidates = vec![
        KnownCandidate {
            path: file1.clone(),
            source: WidevineSource::GoogleChrome,
        },
        KnownCandidate {
            path: file2,
            source: WidevineSource::Brave,
        },
    ];

    let res = discover_widevine_internal(None, None, &candidates)
        .expect("Known candidate discovery must succeed");

    assert_eq!(res.source, WidevineSource::GoogleChrome);
    assert_eq!(res.library_path, file1);
}

#[test]
fn test_no_candidates_returns_widevine_not_found() {
    let res = discover_widevine_internal(None, None, &[]);
    assert_eq!(res, Err(WidevineError::NotFound));
}

#[test]
fn test_validation_rejects_non_elf_and_directories() {
    let tmp = tempdir().unwrap();

    // 1. Text file (not an ELF)
    let txt_path = tmp.path().join("fake_widevine.so");
    fs::write(&txt_path, b"not an ELF binary").unwrap();
    let err = validate_widevine_library(&txt_path).unwrap_err();
    assert!(err.contains("not an ELF binary"));

    // 2. Directory
    let dir_path = tmp.path().join("directory.so");
    fs::create_dir(&dir_path).unwrap();
    let err_dir = validate_widevine_library(&dir_path).unwrap_err();
    assert!(err_dir.contains("not a regular file"));

    // 3. Nonexistent file
    let missing = tmp.path().join("does_not_exist.so");
    let err_missing = validate_widevine_library(&missing).unwrap_err();
    assert!(err_missing.contains("does not exist"));
}

#[test]
fn test_error_formatting_matches_spec() {
    let err = WidevineError::NotFound;
    let formatted = err.to_string();
    assert!(
        formatted.contains("Widevine CDM was not found. Apple Music playback requires Widevine."),
        "Error message must contain required preamble, got: {formatted}"
    );
    assert!(
        formatted.contains("Set MALUS_WIDEVINE_PATH to an existing libwidevinecdm.so"),
        "Error message must contain path guidance, got: {formatted}"
    );
}
