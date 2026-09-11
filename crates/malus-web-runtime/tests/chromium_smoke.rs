use std::time::Duration;

use malus_web_runtime::{LaunchMode, RuntimeOptions, WebRuntime};
use serde_json::json;

#[tokio::test]
async fn test_chromium_headless_smoke_and_lifecycle() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("browser_profile");

    let initial_html = "data:text/html,<html><head><title>Malus Web Runtime</title></head><body><p>Running</p></body></html>";

    let options = RuntimeOptions {
        launch_mode: LaunchMode::Headless,
        initial_url: initial_html.to_string(),
        custom_profile_path: Some(profile_path),
        ..Default::default()
    };

    println!("Launching headless browser session...");
    let runtime = WebRuntime::launch(options)
        .await
        .expect("Failed to launch WebRuntime with installed browser");

    let candidate = runtime.candidate();
    println!(
        "Active browser candidate: {} at {:?}",
        candidate.display_name, candidate.path
    );

    // 1. Diagnostic health query
    let health = runtime
        .check_health()
        .await
        .expect("Health check should succeed");
    assert!(health.alive, "Runtime must report alive: true");
    let pid = health.pid;
    assert!(pid > 0, "Valid PID expected");
    println!("Browser PID: {}, CDP port: {}", pid, health.port);

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
async fn test_chromium_headed_minimize() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("browser_profile");

    let initial_html = "data:text/html,<html><head><title>Minimize Test</title></head><body><p>Testing minimize</p></body></html>";

    let options = RuntimeOptions {
        launch_mode: LaunchMode::Headed,
        initial_url: initial_html.to_string(),
        custom_profile_path: Some(profile_path),
        ..Default::default()
    };

    println!("Launching headed browser session for minimize test...");
    let runtime = WebRuntime::launch(options)
        .await
        .expect("Failed to launch WebRuntime");

    let page = runtime.page();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check Hyprland clients before minimize
    let before_output = std::process::Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .expect("hyprctl failed");
    let before_str = String::from_utf8_lossy(&before_output.stdout);
    let before_json: serde_json::Value = serde_json::from_str(&before_str).unwrap();
    let before_chrome: Vec<_> = before_json
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["class"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("chrome")
        })
        .collect();
    println!("Chrome windows before minimize: {}", before_chrome.len());

    println!("Calling page.minimize_window()...");
    let res = page.minimize_window().await;
    println!("Minimize result: {:?}", res);

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check Hyprland clients after minimize
    let after_output = std::process::Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .expect("hyprctl failed");
    let after_str = String::from_utf8_lossy(&after_output.stdout);
    let after_json: serde_json::Value = serde_json::from_str(&after_str).unwrap();
    let after_chrome: Vec<_> = after_json
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["class"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("chrome")
        })
        .collect();
    println!("Chrome windows after minimize: {}", after_chrome.len());
    if let Some(w) = after_chrome.first() {
        println!(
            "Window after minimize details: hidden={}, mapped={}, workspace={:?}",
            w["hidden"], w["mapped"], w["workspace"]
        );
    }

    runtime.shutdown().await.expect("Shutdown failed");
}

#[tokio::test]
async fn test_chromium_windowless() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("browser_profile");

    let initial_html = "data:text/html,<html><head><title>Windowless Test</title></head><body><p>Testing windowless</p></body></html>";

    let options = RuntimeOptions {
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

    // Check Hyprland clients
    let output = std::process::Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .expect("hyprctl failed");
    let json_str = String::from_utf8_lossy(&output.stdout);
    let clients: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let chrome_clients: Vec<_> = clients
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["class"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("chrome")
        })
        .collect();
    println!(
        "Chrome windows on Hyprland in windowless mode: {}",
        chrome_clients.len()
    );

    let title_val = page
        .wait_for_expression("document.title", Duration::from_secs(5))
        .await
        .expect("Page title should become available");
    assert_eq!(title_val.as_str().unwrap_or_default(), "Windowless Test");

    runtime.shutdown().await.expect("Shutdown failed");
}

#[tokio::test]
async fn test_chromium_main_document_interception() {
    let tmp = tempfile::tempdir().expect("Failed to create tempdir for test profile");
    let profile_path = tmp.path().join("browser_profile");

    let options = RuntimeOptions {
        launch_mode: LaunchMode::Headless,
        initial_url: "about:blank".to_string(),
        custom_profile_path: Some(profile_path),
        ..Default::default()
    };

    let runtime = WebRuntime::launch(options)
        .await
        .expect("Failed to launch WebRuntime for interception test");

    let page = runtime.page();

    let intercepted_html = "<!DOCTYPE html><html><head><title>Intercepted Origin Test</title></head><body><h1>Hello Intercepted</h1></body></html>";

    let rx = page
        .intercept_next_main_document("example.com", "text/html; charset=utf-8", intercepted_html)
        .await
        .expect("Failed to arm document interceptor");

    page.navigate("https://example.com/subpath")
        .await
        .expect("Failed to navigate to target URL");

    tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .expect("Timed out waiting for document fulfillment")
        .expect("Fulfillment channel closed without notification");

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
