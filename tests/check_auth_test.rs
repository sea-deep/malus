use malus::engine::{
    cdp::CdpClient, discovery::select_best_browser, process::BrowserProcess,
    profile::ProfileManager,
};

#[tokio::test]
#[ignore = "requires Chromium; account tests also require malus --login"]
async fn test_inspect_user_profile_auth() {
    let profile = ProfileManager::new().expect("Failed to get profile");
    println!("Checking profile path: {}", profile.profile_dir().display());

    let candidate = select_best_browser(None).expect("No browser candidate found");
    println!("Testing with browser: {}", candidate.display_name);

    // Spawn headless pointing to Apple Music
    let browser = BrowserProcess::spawn(&candidate, profile, false, "https://music.apple.com/")
        .expect("Failed to spawn browser process");
    let port = browser.port;
    println!("Spawned browser on port: {}", port);

    // Connect to page
    let cdp = CdpClient::connect_to_page(port, "music.apple.com")
        .await
        .expect("Failed to connect to page via CDP");

    // Wait up to 15 seconds for MusicKit to initialize and check isAuthorized
    let start = std::time::Instant::now();
    let mut is_authorized = false;
    let mut musickit_found = false;

    while start.elapsed() < std::time::Duration::from_secs(15) {
        let auth_res = cdp.evaluate_js(
            "Boolean(window.MusicKit && window.MusicKit.getInstance && window.MusicKit.getInstance().isAuthorized)"
        ).await;

        if let Ok(val) = auth_res {
            if val.as_bool() == Some(true) {
                is_authorized = true;
                musickit_found = true;
                break;
            }
        }

        let mk_res = cdp
            .evaluate_js("Boolean(window.MusicKit && window.MusicKit.getInstance)")
            .await;
        if let Ok(val) = mk_res {
            if val.as_bool() == Some(true) {
                musickit_found = true;
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    println!("MusicKit instance loaded: {}", musickit_found);
    println!("MusicKit isAuthorized: {}", is_authorized);

    // Also inspect cookies via CDP
    let cookies_res = cdp
        .send_command(
            "Network.getCookies",
            serde_json::json!({
                "urls": ["https://music.apple.com"]
            }),
        )
        .await;

    if let Ok(val) = cookies_res {
        if let Some(cookies) = val.get("cookies").and_then(|c| c.as_array()) {
            println!("Found {} cookies for music.apple.com:", cookies.len());
            for c in cookies {
                if let Some(name) = c.get("name").and_then(|n| n.as_str()) {
                    if name.contains("token")
                        || name.contains("session")
                        || name.contains("auth")
                        || name == "media-user-token"
                    {
                        println!("  - Auth Cookie: {}", name);
                    }
                }
            }
        }
    }

    drop(browser);
    assert!(is_authorized, "Apple Music session is not authorized yet!");
}
