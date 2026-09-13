use std::{path::PathBuf, time::Duration};

use malus_wpe::{
    BrowserEngine, EnginePreference, LaunchMode, RuntimeOptions, WebError, WebRuntime,
};
use serde_json::json;

#[tokio::test]
async fn test_wpe_headless_smoke_and_lifecycle() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("wpe_profile");

    let initial_html = "data:text/html,<html><head><title>WPE Web Runtime</title></head><body><p>Running</p></body></html>";

    let options = RuntimeOptions {
        engine: Some(EnginePreference::Wpe),
        launch_mode: LaunchMode::Headless,
        initial_url: initial_html.to_string(),
        custom_profile_path: Some(profile_path),
        ..Default::default()
    };

    println!("Launching headless browser session with WPE WebKit...");
    let runtime = WebRuntime::launch(options)
        .await
        .expect("Failed to launch WebRuntime with WPE WebKit");

    assert_eq!(runtime.engine(), BrowserEngine::Wpe);

    // 1. Diagnostic health query
    let health = runtime
        .check_health()
        .await
        .expect("Health check should succeed");
    assert!(health.alive, "Runtime must report alive: true");
    assert_eq!(health.engine, BrowserEngine::Wpe);
    let pid = health.pid;
    assert!(pid > 0, "Valid PID expected");
    assert_eq!(health.port, None, "WPE does not use TCP devtools port");
    println!("WPE PID: {}", pid);

    // 2. Evaluate expression & function call
    let page = runtime.page();
    let calc = page
        .evaluate("6 * 7")
        .await
        .expect("Failed to evaluate 6 * 7");
    assert_eq!(calc.as_i64(), Some(42));

    let sum = page
        .call_function("function(a, b) { return a + b; }", &[json!(17), json!(25)])
        .await
        .expect("Failed to call function");
    assert_eq!(sum.as_i64(), Some(42));

    // 3. Register event sink and verify push notification
    page.register_event_sink("__malus_wpe_event")
        .await
        .expect("Failed to register event sink");

    let mut event_rx = page.subscribe_events();

    // Trigger binding from JavaScript
    page.evaluate("window.__malus_wpe_event({ status: 'ok', num: 1234 })")
        .await
        .expect("Failed to invoke event sink from JS");

    let event = tokio::time::timeout(Duration::from_secs(4), event_rx.recv())
        .await
        .expect("Timed out waiting for web event push")
        .expect("Event channel error");

    assert_eq!(event.name, "__malus_wpe_event");
    assert_eq!(event.payload["status"], "ok");
    assert_eq!(event.payload["num"], 1234);
    println!("Received push event from WPE successfully: {:?}", event);

    // 4. Graceful shutdown and verify process termination
    runtime
        .shutdown()
        .await
        .expect("Shutdown should succeed cleanly");

    let is_dead = unsafe {
        let res = libc::kill(pid as i32, 0);
        if res != 0 {
            let err = std::io::Error::last_os_error();
            err.raw_os_error() == Some(libc::ESRCH)
        } else {
            false
        }
    };

    assert!(
        is_dead,
        "WPE process with PID {} should be fully terminated after shutdown",
        pid
    );
    println!("WPE shutdown verified: PID {} is dead.", pid);
}

#[tokio::test]
async fn test_wpe_load_document() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("wpe_profile");

    let options = RuntimeOptions {
        engine: Some(EnginePreference::Wpe),
        launch_mode: LaunchMode::Headless,
        initial_url: "about:blank".to_string(),
        custom_profile_path: Some(profile_path),
        ..Default::default()
    };

    let runtime = WebRuntime::launch(options)
        .await
        .expect("Failed to launch WebRuntime for WPE load_document test");

    let page = runtime.page();

    let test_html = "<!DOCTYPE html><html><head><title>WPE Load Document Test</title></head><body><h1>Hello WPE</h1></body></html>";

    page.load_document("https://music.apple.com/subpath", test_html)
        .await
        .expect("Failed to load document in WPE");

    let origin_val = page
        .wait_for_expression("window.location.origin", Duration::from_secs(5))
        .await
        .expect("Origin should be available");
    assert_eq!(origin_val.as_str(), Some("https://music.apple.com"));

    let title_val = page
        .wait_for_expression("document.title", Duration::from_secs(5))
        .await
        .expect("Title should be available");
    assert_eq!(title_val.as_str(), Some("WPE Load Document Test"));

    runtime.shutdown().await.expect("Shutdown failed");
}

#[tokio::test]
async fn test_engine_selection_fallback_and_failfast() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("profile");

    // 1. Auto mode with invalid WPE dir falls back cleanly to Chromium
    let auto_options = RuntimeOptions {
        engine: Some(EnginePreference::Auto),
        wpe_dir: Some(PathBuf::from("/nonexistent/wpe/dir/malus_test")),
        launch_mode: LaunchMode::Headless,
        initial_url: "about:blank".to_string(),
        custom_profile_path: Some(profile_path.clone()),
        ..Default::default()
    };

    let auto_runtime = WebRuntime::launch(auto_options)
        .await
        .expect("Auto should cleanly fall back to Chromium when WPE is unavailable");
    assert_eq!(auto_runtime.engine(), BrowserEngine::Chromium);
    auto_runtime.shutdown().await.expect("Shutdown failed");

    // 2. Explicit WPE with invalid WPE dir fails fast with explicit error
    let wpe_options = RuntimeOptions {
        engine: Some(EnginePreference::Wpe),
        wpe_dir: Some(PathBuf::from("/nonexistent/wpe/dir/malus_test")),
        launch_mode: LaunchMode::Headless,
        initial_url: "about:blank".to_string(),
        custom_profile_path: Some(profile_path),
        ..Default::default()
    };

    let res = WebRuntime::launch(wpe_options).await;
    let err = match res {
        Err(e) => e,
        Ok(_) => panic!("Explicit WPE must fail fast when unavailable"),
    };

    match err {
        WebError::Initialization { engine, message } => {
            assert_eq!(engine, BrowserEngine::Wpe);
            assert!(message.contains("no WPE candidate was discovered"));
        }
        other => panic!("Expected Initialization error, got: {:?}", other),
    }
}
