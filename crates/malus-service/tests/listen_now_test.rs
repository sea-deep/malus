//! Tests for canonical Apple Music Listen Now (Home) feed adapter.

use malus_model::MediaRef;
use malus_service::pages::home::{
    AppleDisplayKind, build_listen_now_query, get_local_timezone_offset, get_locale_for_storefront,
    is_replay_home_shelf, map_listen_now_response,
};
use serde_json::Value;

#[test]
fn test_display_kind_parsing() {
    assert_eq!(
        AppleDisplayKind::parse("MusicCoverShelf"),
        AppleDisplayKind::CoverShelf
    );
    assert_eq!(
        AppleDisplayKind::parse("MusicCircleCoverShelf"),
        AppleDisplayKind::CircleCoverShelf
    );
    assert_eq!(
        AppleDisplayKind::parse("MusicNotesHeroShelf"),
        AppleDisplayKind::NotesHeroShelf
    );
    assert_eq!(
        AppleDisplayKind::parse("MusicSuperHeroShelf"),
        AppleDisplayKind::SuperHeroShelf
    );
    assert_eq!(
        AppleDisplayKind::parse("MusicCoverGrid"),
        AppleDisplayKind::CoverGrid
    );
    assert_eq!(
        AppleDisplayKind::parse("MusicSocialCardShelf"),
        AppleDisplayKind::SocialCardShelf
    );
    assert_eq!(
        AppleDisplayKind::parse("MusicConcertsEmptyShelf"),
        AppleDisplayKind::ConcertsEmptyShelf
    );
    assert_eq!(
        AppleDisplayKind::parse("SomeFutureUnseenShelf"),
        AppleDisplayKind::Unknown
    );
}

#[test]
fn test_timezone_offset_format() {
    let offset = get_local_timezone_offset();
    assert!(
        offset.starts_with('+') || offset.starts_with('-'),
        "Offset must start with + or -"
    );
    assert_eq!(offset.len(), 6, "Offset must be formatted as [+-]HH:MM");
    assert_eq!(&offset[3..4], ":", "Offset must contain colon separator");
}

#[test]
fn test_locale_derivation() {
    assert_eq!(get_locale_for_storefront("us"), "en-US");
    assert_eq!(get_locale_for_storefront("in"), "en-GB");
    assert_eq!(get_locale_for_storefront("gb"), "en-GB");
    assert_eq!(get_locale_for_storefront("jp"), "ja-JP");
}

#[test]
fn test_listen_now_query_builder() {
    let query = build_listen_now_query("+05:30", "en-GB");
    let map: std::collections::HashMap<&str, &str> =
        query.iter().map(|(k, v)| (*k, v.as_str())).collect();

    assert_eq!(map.get("name"), Some(&"listen-now"));
    assert_eq!(map.get("format[resources]"), Some(&"map"));
    assert_eq!(map.get("platform"), Some(&"web"));
    assert_eq!(map.get("timezone"), Some(&"+05:30"));
    assert_eq!(map.get("l"), Some(&"en-GB"));
    assert!(map.contains_key("displayFilter[kind]"));
    assert!(map.contains_key("types"));
}

#[test]
fn test_listen_now_fixture_structural_invariants() {
    let fixture_str = include_str!("fixtures/listen_now_fixture.json");
    let json: Value = serde_json::from_str(fixture_str).expect("Valid JSON fixture");

    let sections = map_listen_now_response(&json);

    // 1. Structural count verification
    // Input has 8 shelves:
    // - shelf-top-picks (included)
    // - shelf-recently-played (included)
    // - shelf-mood-stations (included)
    // - shelf-favourite-artists (included)
    // - shelf-empty-concerts (OMITTED - empty)
    // - shelf-replay (OMITTED - replay product exclusion)
    // - shelf-unknown-kind (included with fallback hint)
    // - shelf-dangling-ref (included with 1 resolved item)
    // Total should be 6 active sections.
    assert_eq!(
        sections.len(),
        6,
        "Expected exactly 6 active sections after exclusions"
    );

    // 2. Order preservation check
    let section_ids: Vec<&str> = sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        section_ids,
        vec![
            "shelf-top-picks",
            "shelf-recently-played",
            "shelf-mood-stations",
            "shelf-favourite-artists",
            "shelf-unknown-kind",
            "shelf-dangling-ref"
        ],
        "Shelves must preserve Apple's exact declared order"
    );

    // 3. Top Picks (NotesHeroShelf) checks
    let top_picks = &sections[0];
    assert_eq!(top_picks.title.as_deref(), Some("Top Picks"));
    assert_eq!(
        top_picks.presentation_hint.as_deref(),
        Some("top-picks-shelf")
    );
    assert_eq!(
        top_picks.items.len(),
        3,
        "Mixed shelf should resolve all 3 items"
    );

    // First item is playlist with editorial text
    let top_item = &top_picks.items[0];
    assert_eq!(top_item.title, "Weekly Discovery");
    assert_eq!(top_item.subtitle.as_deref(), Some("Apple Music Editorial"));
    assert_eq!(
        top_item.tertiary_text.as_deref(),
        Some("Handpicked fresh tracks curated every week for your taste."),
        "Hero shelf must populate tertiary_text with plainEditorialNotes"
    );
    assert_eq!(
        top_item.entity,
        Some(MediaRef::Playlist("pl-top-1".to_string()))
    );

    // Third item is a station
    assert_eq!(
        top_picks.items[2].entity,
        Some(MediaRef::Station("ra-top-station".to_string()))
    );

    // 4. Recently Played checks
    let recent = &sections[1];
    assert_eq!(recent.title.as_deref(), Some("Recently Played"));
    assert_eq!(recent.subtitle.as_deref(), Some("Based on recent activity"));
    assert_eq!(recent.presentation_hint.as_deref(), Some("shelf"));
    assert_eq!(recent.items.len(), 2);
    assert_eq!(
        recent.items[0].entity,
        Some(MediaRef::Album("alb-recent-1".to_string()))
    );
    assert_eq!(
        recent.items[1].entity,
        Some(MediaRef::Playlist("pl-recent-1".to_string()))
    );

    // 5. Mood stations shelf checks
    let mood = &sections[2];
    assert_eq!(mood.title.as_deref(), Some("Mood Stations"));
    assert_eq!(mood.presentation_hint.as_deref(), Some("stations-shelf"));
    assert_eq!(mood.items.len(), 2);
    assert_eq!(mood.items[0].title, "Relax");
    assert_eq!(
        mood.items[0].entity,
        Some(MediaRef::Station("ra.q-test-relax".to_string()))
    );
    assert_eq!(mood.items[1].title, "Energy");
    assert_eq!(
        mood.items[1].entity,
        Some(MediaRef::Station("ra.q-test-energy".to_string()))
    );

    // 6. Artists shelf checks
    let artists = &sections[3];
    assert_eq!(artists.title.as_deref(), Some("Artists You Follow"));
    assert_eq!(artists.presentation_hint.as_deref(), Some("artist-shelf"));
    assert_eq!(artists.items.len(), 1);
    assert_eq!(artists.items[0].title, "Featured Star");
    assert_eq!(
        artists.items[0].entity,
        Some(MediaRef::Artist("artist-1".to_string()))
    );

    // 7. Unknown display kind fallback checks
    let unknown_shelf = &sections[4];
    assert_eq!(unknown_shelf.title.as_deref(), Some("Experimental Feed"));
    assert_eq!(
        unknown_shelf.presentation_hint.as_deref(),
        Some("shelf"),
        "Unknown display kinds must fall back to generic shelf"
    );
    assert_eq!(unknown_shelf.items.len(), 1);

    // 8. Dangling reference handling checks
    let dangling_shelf = &sections[5];
    assert_eq!(dangling_shelf.title.as_deref(), Some("Partial Content"));
    assert_eq!(
        dangling_shelf.items.len(),
        1,
        "Missing dereference should not crash and valid items must be retained"
    );
    assert_eq!(dangling_shelf.items[0].title, "Fallback Album");
}

#[test]
fn test_is_replay_home_shelf_detection() {
    let fixture_str = include_str!("fixtures/listen_now_fixture.json");
    let json: Value = serde_json::from_str(fixture_str).expect("Valid JSON fixture");
    let resources = &json["resources"];

    let replay_shelf = &resources["personal-recommendation"]["shelf-replay"];
    assert!(
        is_replay_home_shelf(replay_shelf, resources),
        "Must identify Replay shelf via semantic pl.rp-* id"
    );

    let top_picks = &resources["personal-recommendation"]["shelf-top-picks"];
    assert!(
        !is_replay_home_shelf(top_picks, resources),
        "Must not classify top picks as Replay"
    );
}
