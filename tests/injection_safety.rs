use malus::engine::{
    cdp::CdpClient, discovery::select_best_browser, process::BrowserProcess,
    profile::ProfileManager,
};

#[tokio::test]
#[ignore = "requires Chromium; account tests also require malus --login"]
async fn test_injection_safety_with_malicious_inputs() {
    let candidate = select_best_browser(None).expect("No browser candidate found");
    let temp_profile =
        std::env::temp_dir().join(format!("malus_inject_test_{}", std::process::id()));
    let profile = ProfileManager::with_custom_path(&temp_profile);

    let browser = BrowserProcess::spawn(&candidate, profile, false, "about:blank")
        .expect("Failed to spawn browser");
    let cdp = CdpClient::connect_to_page(browser.port, "about:blank")
        .await
        .expect("Failed to connect CDP");

    // Attack payloads that would cause syntax errors or code execution if string interpolation were used
    let malicious_payloads = [
        "123'); alert(1); //",
        "\"; window.__pwned = true; \"",
        "'); return 'hacked'; ('",
        "`${7*7}`",
        "\\'; document.location='http://evil.com';",
        "<script>alert(1)</script>",
        "Track with 'single' and \"double\" quotes and \n newlines \r\n and \t tabs",
        "Unicode: \u{202E}reversed\u{202D} and \u{0000} null byte",
    ];

    for payload in malicious_payloads {
        // Echo function that simply returns the argument unmodified
        let res = cdp
            .call_function(
                "function(arg) { return arg; }",
                &[serde_json::json!(payload)],
            )
            .await
            .expect("call_function failed on attack payload");

        assert_eq!(
            res.as_str().unwrap(),
            payload,
            "Attack payload was modified or failed to roundtrip: {}",
            payload
        );
    }

    // Verify that window.__pwned was NOT created in the browser window
    let pwned_res = cdp
        .evaluate_js("typeof window.__pwned")
        .await
        .expect("Failed to check __pwned status");
    assert_eq!(pwned_res.as_str().unwrap(), "undefined");

    drop(browser);
    let _ = std::fs::remove_dir_all(&temp_profile);
}
