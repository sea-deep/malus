use malus::engine::{
    BrowserProcess, CdpClient, MusicKitDriver, ProfileManager, musickit::parse_musickit_track,
    select_best_browser,
};
use std::{sync::Arc, time::Duration};
#[tokio::test]
#[ignore = "requires Chromium and a signed-in Malus profile"]
async fn test_musickit_api_search_and_library() {
    let candidate = select_best_browser(None).expect("browser required");
    let browser = BrowserProcess::spawn(
        &candidate,
        ProfileManager::new().unwrap(),
        false,
        "https://music.apple.com/",
    )
    .unwrap();
    let cdp = Arc::new(
        CdpClient::connect_to_page(browser.port, "music.apple.com")
            .await
            .unwrap(),
    );
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            if cdp
                .evaluate_js("Boolean(window.MusicKit && MusicKit.getInstance().isAuthorized)")
                .await
                .ok()
                .and_then(|v| v.as_bool())
                == Some(true)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .expect("Sign in with malus --login before running this test");
    let driver = MusicKitDriver::new(cdp);
    let tracks = driver.search_catalog("Get Lucky", 3).await.unwrap();
    assert!(!tracks.is_empty());
    assert!(
        tracks
            .iter()
            .all(|t| !t.id.is_empty() && !t.title.is_empty())
    );
    let library = driver.fetch_all("/v1/me/library/songs").await.unwrap();
    assert!(library.iter().all(|v| parse_musickit_track(v).is_some()));
    assert!(
        library
            .iter()
            .filter_map(parse_musickit_track)
            .all(|t| t.album_id.starts_with("l."))
    );
    println!(
        "Verified catalog search and {} library songs, including album relationships.",
        library.len()
    );
    drop(driver);
    drop(browser);
}
