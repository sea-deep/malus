use malus::engine::{
    cdp::CdpClient, discovery::select_best_browser, process::BrowserProcess,
    profile::ProfileManager,
};

#[tokio::test]
#[ignore = "requires Chromium; account tests also require malus --login"]
async fn test_live_browser_spawn_and_cdp() {
    let candidate = select_best_browser(None).expect("No browser candidate found");
    println!("Testing with browser: {}", candidate.display_name);

    let temp_profile = std::env::temp_dir().join(format!("malus_live_test_{}", std::process::id()));
    let profile = ProfileManager::with_custom_path(&temp_profile);

    let browser = BrowserProcess::spawn(&candidate, profile, false, "about:blank")
        .expect("Failed to spawn browser process");
    let port = browser.port;
    println!("Spawned browser on port: {}", port);

    let cdp = CdpClient::connect_to_page(port, "about:blank")
        .await
        .expect("Failed to connect to page via CDP");

    // Ping
    cdp.ping().await.expect("CDP ping failed");
    println!("CDP ping succeeded!");

    // Evaluate JS
    let res = cdp
        .evaluate_js("21 + 21")
        .await
        .expect("evaluate_js failed");
    assert_eq!(res, serde_json::json!(42));
    println!("CDP evaluate_js 21+21 = {}!", res);

    // Call function
    let res_fn = cdp
        .call_function(
            "function(a, b) { return a * b; }",
            &[serde_json::json!(6), serde_json::json!(7)],
        )
        .await
        .expect("call_function failed");
    assert_eq!(res_fn, serde_json::json!(42));
    println!("CDP call_function 6*7 = {}!", res_fn);

    // Cleanup
    drop(browser);
    let _ = std::fs::remove_dir_all(&temp_profile);
    println!("Live test completed cleanly!");
}
