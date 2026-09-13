use std::time::Duration;

use malus_web_runtime::{BrowserEngine, EnginePreference, LaunchMode, RuntimeOptions, WebRuntime};
use serde_json::json;

#[tokio::test]
async fn test_chromium_headless_smoke_and_lifecycle() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("browser_profile");

    let initial_html = "data:text/html,<html><head><title>Malus Web Runtime</title></head><body><p>Running</p></body></html>";

    let options = RuntimeOptions {
        engine: Some(EnginePreference::Chromium),
        launch_mode: LaunchMode::Headless,
        initial_url: initial_html.to_string(),
        custom_profile_path: Some(profile_path),
        ..Default::default()
    };

    println!("Launching headless browser session with Chromium...");
    let runtime = WebRuntime::launch(options)
        .await
        .expect("Failed to launch WebRuntime with Chromium");

    assert_eq!(runtime.engine(), BrowserEngine::Chromium);

    // 1. Diagnostic health query
    let health = runtime
        .check_health()
        .await
        .expect("Health check should succeed");
    assert!(health.alive, "Runtime must report alive: true");
    assert_eq!(health.engine, BrowserEngine::Chromium);
    let pid = health.pid;
    assert!(pid > 0, "Valid PID expected");
    println!("Browser PID: {}, CDP port: {:?}", pid, health.port);

    // 2. Evaluate DOM property
    let page = runtime.page();
    let title_val = page
        .wait_for_expression("document.title", Duration::from_secs(5))
        .await
        .expect("Page title should become available");
    assert_eq!(
        title_val.as_str().unwrap_or_default(),
        "Malus Web Runtime",
        "Page title should match initial HTML"
    );

    // 3. Evaluate expression & function call
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

    // 4. Register event sink and verify push notification
    page.register_event_sink("__malus_smoke_event")
        .await
        .expect("Failed to register event sink");

    let mut event_rx = page.subscribe_events();

    // Trigger binding from JavaScript
    page.evaluate("window.__malus_smoke_event(JSON.stringify({ status: 'ok', num: 1234 }))")
        .await
        .expect("Failed to invoke event sink from JS");

    let event = tokio::time::timeout(Duration::from_secs(4), event_rx.recv())
        .await
        .expect("Timed out waiting for web event push")
        .expect("Event channel error");

    assert_eq!(event.name, "__malus_smoke_event");
    assert_eq!(event.payload["status"], "ok");
    assert_eq!(event.payload["num"], 1234);
    println!("Received push event successfully: {:?}", event);

    // 5. Graceful shutdown and verify no leaked owned processes
    runtime
        .shutdown()
        .await
        .expect("Shutdown should succeed cleanly");

    // Verify process is terminated
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
        "Browser process with PID {} should be fully terminated after shutdown",
        pid
    );
    println!(
        "Shutdown verified: PID {} is dead, no zombie processes.",
        pid
    );
}

#[tokio::test]
async fn test_chromium_windowless() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("browser_profile");

    let initial_html = "data:text/html,<html><head><title>Windowless Test</title></head><body><p>Testing windowless</p></body></html>";

    let options = RuntimeOptions {
        engine: Some(EnginePreference::Chromium),
        launch_mode: LaunchMode::Windowless,
        initial_url: initial_html.to_string(),
        custom_profile_path: Some(profile_path),
        ..Default::default()
    };

    println!("Launching windowless browser session...");
    let runtime = WebRuntime::launch(options)
        .await
        .expect("Failed to launch WebRuntime windowless");

    let page = runtime.page();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let title_val = page
        .wait_for_expression("document.title", Duration::from_secs(5))
        .await
        .expect("Page title should become available");
    assert_eq!(title_val.as_str().unwrap_or_default(), "Windowless Test");

    runtime.shutdown().await.expect("Shutdown failed");
}

#[tokio::test]
async fn test_chromium_load_document() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("browser_profile");

    let options = RuntimeOptions {
        engine: Some(EnginePreference::Chromium),
        launch_mode: LaunchMode::Headless,
        initial_url: "about:blank".to_string(),
        custom_profile_path: Some(profile_path),
        ..Default::default()
    };

    let runtime = WebRuntime::launch(options)
        .await
        .expect("Failed to launch WebRuntime for load_document test");

    let page = runtime.page();

    let intercepted_html = "<!DOCTYPE html><html><head><title>Intercepted Origin Test</title></head><body><h1>Hello Intercepted</h1></body></html>";

    page.load_document("https://example.com/subpath", intercepted_html)
        .await
        .expect("Failed to load document");

    let origin_val = page
        .wait_for_expression("window.location.origin", Duration::from_secs(5))
        .await
        .expect("Origin should be available");
    assert_eq!(origin_val.as_str(), Some("https://example.com"));

    let title_val = page
        .wait_for_expression("document.title", Duration::from_secs(5))
        .await
        .expect("Title should be available");
    assert_eq!(title_val.as_str(), Some("Intercepted Origin Test"));

    runtime.shutdown().await.expect("Shutdown failed");
}
