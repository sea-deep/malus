use malus_model::MediaRef;
use malus_service::{OfficialAppleMusicApi, ProductionAppleWebSession, ProfileTokenProvider};
use malus_wpe::ProfileManager;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires real authenticated Apple Music profile"]
async fn test_live_item_types_probe() {
    let pm = ProfileManager::for_namespace("apple").expect("profile manager");
    let token_path = pm.profile_dir().join("tokens.json");
    if !token_path.exists() {
        eprintln!("No tokens.json found, skipping");
        return;
    }

    let session = Arc::new(ProductionAppleWebSession::new());
    let token_provider = Arc::new(ProfileTokenProvider::new(session.clone()));
    let api = Arc::new(OfficialAppleMusicApi::new(token_provider));

    // 1. Song (Catalog)
    let song = MediaRef::Song("617154362".to_string()); // Instant Crush
    let state = api
        .get_account_media_state(&song)
        .await
        .expect("song state");
    println!("Catalog Song state: {:?}", state);

    // 2. Album (Catalog)
    let album = MediaRef::Album("617154241".to_string()); // Random Access Memories
    let album_state = api
        .get_account_media_state(&album)
        .await
        .expect("album state");
    println!("Catalog Album state: {:?}", album_state);

    // 3. Catalog Playlist
    let cat_pl = MediaRef::Playlist("pl.u-8aAVZAaFmvZPE7Y".to_string());
    let cat_pl_state = api
        .get_account_media_state(&cat_pl)
        .await
        .expect("cat pl state");
    println!("Catalog Playlist state: {:?}", cat_pl_state);

    // 4. Personal/Smart Playlist
    let smart_pl = MediaRef::Playlist("pl.pm-7cd15d345a6ed1efbb6370fcaf27bd3f".to_string());
    let smart_pl_state = api
        .get_account_media_state(&smart_pl)
        .await
        .expect("smart pl state");
    println!("Smart Playlist state: {:?}", smart_pl_state);

    // 5. Library Playlist
    let lib_pl = MediaRef::Playlist("p.VRU64LvNXP".to_string());
    let lib_pl_state = api
        .get_account_media_state(&lib_pl)
        .await
        .expect("lib pl state");
    println!("Library Playlist state: {:?}", lib_pl_state);

    // 6. Test playlist ID resolution
    let resolved_cat = api
        .resolve_library_playlist_id("pl.u-8aAVZAaFmvZPE7Y")
        .await
        .expect("resolve catalog pl");
    println!(
        "Resolved catalog playlist pl.u-8aAVZAaFmvZPE7Y -> {}",
        resolved_cat
    );
    assert!(resolved_cat.starts_with("p."));

    let resolved_smart = api
        .resolve_library_playlist_id("pl.pm-7cd15d345a6ed1efbb6370fcaf27bd3f")
        .await;
    println!("Resolved smart playlist -> {:?}", resolved_smart);

    let resolved_lib = api
        .resolve_library_playlist_id("p.VRU64LvNXP")
        .await
        .expect("resolve lib pl");
    println!("Resolved lib playlist -> {}", resolved_lib);
    assert_eq!(resolved_lib, "p.VRU64LvNXP");

    // 7. Test favorite / unfavorite on library playlist
    api.favorite(&lib_pl).await.expect("favorite lib pl");
    api.unfavorite(&lib_pl).await.expect("unfavorite lib pl");

    // 8. Test favorite / unfavorite on song
    api.favorite(&song).await.expect("favorite song");
    api.unfavorite(&song).await.expect("unfavorite song");

    // 9. Test add_to_library on library playlist and catalog song
    api.add_to_library(&lib_pl).await.expect("add lib pl");
    api.add_to_library(&song).await.expect("add song");

    println!("ALL ITEM TYPE PROBES PASSED SUCCESSFULLY!");
}
