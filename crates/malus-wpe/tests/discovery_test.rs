use malus_wpe::{BrowserEngine, LaunchMechanism, WebError, discover_browsers, select_best_browser};
use std::path::Path;

#[test]
fn test_discover_finds_host_browsers() {
    let candidates = discover_browsers(None);
    assert!(
        !candidates.is_empty(),
        "Expected to discover at least one native Chromium-family browser on this host"
    );

    for c in &candidates {
        assert_eq!(c.engine, BrowserEngine::Chromium);
        assert_eq!(c.mechanism, LaunchMechanism::Native);
        assert!(c.path.exists(), "Candidate path must exist: {:?}", c.path);
        println!(
            "Discovered: {} ({:?}) at {:?}, version: {:?}",
            c.display_name, c.product, c.path, c.version
        );
    }
}

#[test]
fn test_select_best_browser_succeeds() {
    let best = select_best_browser(None).expect("Should select best browser");
    assert!(best.path.exists());
    assert!(!best.exec_cmd.is_empty());
}

#[test]
fn test_nonexistent_explicit_browser_fails() {
    let nonexistent = Path::new("/tmp/nonexistent_browser_binary_malus_test");
    let err = select_best_browser(Some(nonexistent)).unwrap_err();
    match err {
        WebError::ExplicitBrowserNotFound(p) => {
            assert_eq!(p, nonexistent);
        }
        other => panic!("Expected ExplicitBrowserNotFound, got: {:?}", other),
    }
}
