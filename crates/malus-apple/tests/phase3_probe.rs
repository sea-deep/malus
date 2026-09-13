//! Stage 3: Live endpoint probing for Phase 3 Apple Surface Implementation.
//! Run: cargo test -p malus-provider-apple --test phase3_probe -- --ignored --nocapture

use malus_apple::{
    AppleCredentials, OfficialAppleMusicApi, ProductionAppleWebSession, ProfileTokenProvider,
};
use malus_web_runtime::ProfileManager;
use std::sync::Arc;

async fn make_api() -> Arc<OfficialAppleMusicApi> {
    let pm = ProfileManager::for_namespace("apple").expect("profile manager");
    let token_path = pm.profile_dir().join("tokens.json");
    let _creds = AppleCredentials::load_from_file(&token_path).expect("cached tokens");
    let session = Arc::new(ProductionAppleWebSession::new());
    let token_provider = Arc::new(ProfileTokenProvider::new(session));
    Arc::new(OfficialAppleMusicApi::new(token_provider))
}

fn summarize(label: &str, val: &serde_json::Value) {
    let sep = "=".repeat(60);
    println!("\n{sep}");
    println!("PROBE: {label}");
    println!("{sep}");
    if let Some(obj) = val.as_object() {
        println!("  top-level keys: {:?}", obj.keys().collect::<Vec<_>>());
        if let Some(data) = obj.get("data").and_then(|d| d.as_array()) {
            println!("  data[].len = {}", data.len());
            if let Some(first) = data.first() {
                println!("  data[0].type = {:?}", first.get("type"));
                println!("  data[0].id = {:?}", first.get("id"));
                if let Some(attrs) = first.get("attributes").and_then(|a| a.as_object()) {
                    println!(
                        "  data[0].attributes keys: {:?}",
                        attrs.keys().collect::<Vec<_>>()
                    );
                }
                if let Some(rels) = first.get("relationships").and_then(|r| r.as_object()) {
                    println!(
                        "  data[0].relationships keys: {:?}",
                        rels.keys().collect::<Vec<_>>()
                    );
                }
            }
        }
        if let Some(results) = obj.get("results").and_then(|r| r.as_object()) {
            println!("  results keys: {:?}", results.keys().collect::<Vec<_>>());
            for (k, v) in results.iter() {
                if let Some(arr) = v.get("data").and_then(|d| d.as_array()) {
                    println!("  results.{k}.data.len = {}", arr.len());
                } else if let Some(arr) = v.as_array() {
                    println!("  results.{k}.len = {}", arr.len());
                }
            }
        }
        if let Some(next) = obj.get("next") {
            println!("  next = {:?}", next);
        }
    }
}

fn summarize_deep(label: &str, val: &serde_json::Value, max_items: usize) {
    let sep = "=".repeat(70);
    println!("\n{sep}");
    println!("DEEP PROBE: {label}");
    println!("{sep}");
    if let Some(data) = val.get("data").and_then(|d| d.as_array()) {
        for (i, item) in data.iter().enumerate().take(max_items) {
            let typ = item.get("type").and_then(|t| t.as_str()).unwrap_or("?");
            let id = item.get("id").and_then(|t| t.as_str()).unwrap_or("?");
            let attrs = item.get("attributes");
            let name = attrs
                .and_then(|a| a.get("name").or(a.get("title")))
                .and_then(|n| n.as_str())
                .unwrap_or("?");
            println!("  [{i}] type={typ} id={id} name={name:?}");
            if let Some(a) = attrs.and_then(|a| a.as_object()) {
                for key in &[
                    "artistName",
                    "albumName",
                    "curatorName",
                    "description",
                    "isLive",
                    "stationProviderName",
                    "genreNames",
                    "releaseDate",
                    "trackCount",
                    "contentRating",
                    "url",
                    "reason",
                ] {
                    if let Some(v) = a.get(*key) {
                        let s = format!("{v}");
                        println!("      {key}: {}", &s[..s.len().min(120)]);
                    }
                }
                if let Some(art) = a.get("artwork").and_then(|a| a.as_object()) {
                    let url = art.get("url").and_then(|u| u.as_str()).unwrap_or("?");
                    println!("      artwork.url: {}...", &url[..url.len().min(80)]);
                }
            }
            if let Some(rels) = item.get("relationships").and_then(|r| r.as_object()) {
                let rel_keys: Vec<_> = rels.keys().collect();
                if !rel_keys.is_empty() {
                    println!("      relationships: {:?}", rel_keys);
                    for rk in &rel_keys {
                        if let Some(rd) = rels
                            .get(*rk)
                            .and_then(|r| r.get("data"))
                            .and_then(|d| d.as_array())
                        {
                            println!("        {rk}.data.len = {}", rd.len());
                        }
                    }
                }
            }
        }
        if data.len() > max_items {
            println!("  ... ({} more)", data.len() - max_items);
        }
    }
    if let Some(next) = val.get("next").and_then(|n| n.as_str()) {
        println!("  NEXT: {}", &next[..next.len().min(120)]);
    }
}

#[tokio::test]
#[ignore = "live probe requiring authenticated profile"]
async fn probe_all_phase3_endpoints() {
    let api = make_api().await;

    // ========== HOME ==========
    println!("\n\n########## HOME ##########");

    // 1. Recommendations
    match api
        .send_request("/v1/me/recommendations", &[("limit", "10")])
        .await
    {
        Ok(v) => {
            summarize("GET /v1/me/recommendations", &v);
            // Check recommendation group structure
            if let Some(data) = v.get("data").and_then(|d| d.as_array()) {
                for (i, group) in data.iter().enumerate().take(5) {
                    let title = group
                        .get("attributes")
                        .and_then(|a| {
                            a.get("title")
                                .and_then(|t| t.get("stringForDisplay"))
                                .and_then(|s| s.as_str())
                        })
                        .unwrap_or("?");
                    let reason = group.get("attributes").and_then(|a| {
                        a.get("reason")
                            .and_then(|r| r.get("stringForDisplay"))
                            .and_then(|s| s.as_str())
                    });
                    let group_type = group.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                    let next_update = group
                        .get("attributes")
                        .and_then(|a| a.get("nextUpdateDate").and_then(|n| n.as_str()));
                    let is_group_rec = group
                        .get("attributes")
                        .and_then(|a| a.get("isGroupRecommendation"));
                    println!(
                        "  GROUP[{i}]: type={group_type} title={title:?} reason={reason:?} nextUpdate={next_update:?} isGroup={is_group_rec:?}"
                    );
                    // Print attribute keys
                    if let Some(attrs) = group.get("attributes").and_then(|a| a.as_object()) {
                        println!("    attribute keys: {:?}", attrs.keys().collect::<Vec<_>>());
                    }
                    // relationships.contents
                    if let Some(contents) = group
                        .get("relationships")
                        .and_then(|r| r.get("contents"))
                        .and_then(|c| c.get("data"))
                        .and_then(|d| d.as_array())
                    {
                        for (j, item) in contents.iter().enumerate().take(3) {
                            let typ = item.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                            let id = item.get("id").and_then(|i| i.as_str()).unwrap_or("?");
                            let name = item
                                .get("attributes")
                                .and_then(|a| a.get("name").and_then(|n| n.as_str()))
                                .unwrap_or("?");
                            println!("    CONTENT[{j}]: type={typ} id={id} name={name:?}");
                        }
                        println!("    ... {} total content items", contents.len());
                    }
                }
            }
            if let Some(next) = v.get("next").and_then(|n| n.as_str()) {
                println!("  RECOMMENDATIONS NEXT: {next}");
            }
        }
        Err(e) => println!("ERROR recommendations: {e}"),
    }

    // 2. Recently Played
    match api
        .send_request("/v1/me/recent/played", &[("limit", "10")])
        .await
    {
        Ok(v) => {
            summarize("GET /v1/me/recent/played", &v);
            summarize_deep("/v1/me/recent/played", &v, 5);
        }
        Err(e) => println!("ERROR recent/played: {e}"),
    }

    // 3. Heavy Rotation
    match api
        .send_request("/v1/me/history/heavy-rotation", &[("limit", "10")])
        .await
    {
        Ok(v) => {
            summarize("GET /v1/me/history/heavy-rotation", &v);
            summarize_deep("/v1/me/history/heavy-rotation", &v, 5);
        }
        Err(e) => println!("ERROR heavy-rotation: {e}"),
    }

    // ========== NEW (CHARTS) ==========
    println!("\n\n########## NEW (CHARTS) ##########");

    match api
        .send_request(
            "/v1/catalog/{storefront}/charts",
            &[("types", "songs,albums,playlists"), ("limit", "5")],
        )
        .await
    {
        Ok(v) => {
            summarize("GET /v1/catalog/{sf}/charts", &v);
            if let Some(results) = v.get("results").and_then(|r| r.as_object()) {
                for (chart_type, chart_data) in results.iter() {
                    println!("  CHART TYPE: {chart_type}");
                    if let Some(arr) = chart_data.as_array() {
                        for (ci, chart) in arr.iter().enumerate() {
                            let chart_name =
                                chart.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                            let chart_id =
                                chart.get("chart").and_then(|c| c.as_str()).unwrap_or("?");
                            let href = chart.get("href").and_then(|h| h.as_str()).unwrap_or("?");
                            println!("    [{ci}] chart={chart_id} name={chart_name}");
                            println!("       href: {}", &href[..href.len().min(100)]);
                            if let Some(data) = chart.get("data").and_then(|d| d.as_array()) {
                                println!("       data.len = {}", data.len());
                                if let Some(first) = data.first() {
                                    let typ =
                                        first.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                                    let id =
                                        first.get("id").and_then(|i| i.as_str()).unwrap_or("?");
                                    let name = first
                                        .get("attributes")
                                        .and_then(|a| a.get("name").and_then(|n| n.as_str()))
                                        .unwrap_or("?");
                                    println!("       [0] type={typ} id={id} name={name}");
                                }
                            }
                            if let Some(next) = chart.get("next").and_then(|n| n.as_str()) {
                                println!("       next: {}", &next[..next.len().min(100)]);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => println!("ERROR charts: {e}"),
    }

    // ========== RADIO ==========
    println!("\n\n########## RADIO ##########");

    // Recent Radio Stations
    match api
        .send_request("/v1/me/recent/radio-stations", &[("limit", "10")])
        .await
    {
        Ok(v) => {
            summarize("GET /v1/me/recent/radio-stations", &v);
            summarize_deep("/v1/me/recent/radio-stations", &v, 5);
        }
        Err(e) => println!("ERROR recent/radio-stations: {e}"),
    }

    // Search for stations
    match api
        .send_request(
            "/v1/catalog/{storefront}/search",
            &[
                ("term", "Apple Music"),
                ("types", "stations"),
                ("limit", "5"),
            ],
        )
        .await
    {
        Ok(v) => {
            println!("\n--- Apple Music station search ---");
            if let Some(stations) = v
                .get("results")
                .and_then(|r| r.get("stations"))
                .and_then(|s| s.get("data"))
                .and_then(|d| d.as_array())
            {
                for (i, s) in stations.iter().enumerate() {
                    let name = s
                        .get("attributes")
                        .and_then(|a| a.get("name").and_then(|n| n.as_str()))
                        .unwrap_or("?");
                    let id = s.get("id").and_then(|i| i.as_str()).unwrap_or("?");
                    let is_live = s.get("attributes").and_then(|a| a.get("isLive"));
                    let provider = s
                        .get("attributes")
                        .and_then(|a| a.get("stationProviderName").and_then(|n| n.as_str()));
                    println!(
                        "  [{i}] id={id} name={name:?} isLive={is_live:?} provider={provider:?}"
                    );
                    if let Some(attrs) = s.get("attributes").and_then(|a| a.as_object()) {
                        println!(
                            "       attribute keys: {:?}",
                            attrs.keys().collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
        Err(e) => println!("ERROR station search: {e}"),
    }

    // ========== LIBRARY ==========
    println!("\n\n########## LIBRARY ##########");

    // Recently Added
    match api
        .send_request("/v1/me/library/recently-added", &[("limit", "5")])
        .await
    {
        Ok(v) => {
            summarize("GET /v1/me/library/recently-added", &v);
            summarize_deep("/v1/me/library/recently-added", &v, 5);
        }
        Err(e) => println!("ERROR recently-added: {e}"),
    }

    // Library Artists
    match api
        .send_request("/v1/me/library/artists", &[("limit", "5")])
        .await
    {
        Ok(v) => {
            summarize("GET /v1/me/library/artists", &v);
            summarize_deep("/v1/me/library/artists", &v, 5);
        }
        Err(e) => println!("ERROR library/artists: {e}"),
    }

    // ========== DETAILS ==========
    println!("\n\n########## DETAILS ##########");

    // Album detail with tracks — using a well-known album ID
    match api
        .send_request(
            "/v1/catalog/{storefront}/albums/1440857780",
            &[("include", "tracks")],
        )
        .await
    {
        Ok(v) => {
            summarize("GET album/1440857780 (RAM)", &v);
            summarize_deep("Album detail", &v, 1);
            if let Some(data) = v
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
            {
                if let Some(tracks) = data
                    .get("relationships")
                    .and_then(|r| r.get("tracks"))
                    .and_then(|t| t.get("data"))
                    .and_then(|d| d.as_array())
                {
                    println!("  TRACKS ({} total):", tracks.len());
                    for (i, t) in tracks.iter().enumerate().take(3) {
                        let name = t
                            .get("attributes")
                            .and_then(|a| a.get("name").and_then(|n| n.as_str()))
                            .unwrap_or("?");
                        let dur = t.get("attributes").and_then(|a| a.get("durationInMillis"));
                        let tn = t.get("attributes").and_then(|a| a.get("trackNumber"));
                        let disc = t.get("attributes").and_then(|a| a.get("discNumber"));
                        let cr = t.get("attributes").and_then(|a| a.get("contentRating"));
                        println!(
                            "    [{i}] name={name:?} dur={dur:?} track#={tn:?} disc#={disc:?} contentRating={cr:?}"
                        );
                    }
                }
                if let Some(tracks_next) = data
                    .get("relationships")
                    .and_then(|r| r.get("tracks"))
                    .and_then(|t| t.get("next"))
                    .and_then(|n| n.as_str())
                {
                    println!("  TRACKS NEXT: {tracks_next}");
                }
            }
        }
        Err(e) => println!("ERROR album detail: {e}"),
    }

    // Artist detail with views
    match api
        .send_request(
            "/v1/catalog/{storefront}/artists/5468295",
            &[(
                "views",
                "top-songs,latest-release,full-albums,featured-albums,singles,similar-artists",
            )],
        )
        .await
    {
        Ok(v) => {
            summarize("GET artist/5468295 (Daft Punk) with views", &v);
            if let Some(data) = v
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
                && let Some(views) = data.get("views").and_then(|v| v.as_object())
            {
                println!("  VIEWS present: {:?}", views.keys().collect::<Vec<_>>());
                for (vk, vv) in views.iter() {
                    let title = vv
                        .get("attributes")
                        .and_then(|a| a.get("title").and_then(|t| t.as_str()));
                    let data_len = vv.get("data").and_then(|d| d.as_array()).map(|a| a.len());
                    let next = vv.get("next").and_then(|n| n.as_str());
                    println!(
                        "    VIEW {vk}: title={title:?} data.len={data_len:?} next={:?}",
                        next.map(|s| &s[..s.len().min(80)])
                    );
                    // Show first item in each view
                    if let Some(arr) = vv.get("data").and_then(|d| d.as_array())
                        && let Some(first) = arr.first()
                    {
                        let typ = first.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                        let id = first.get("id").and_then(|t| t.as_str()).unwrap_or("?");
                        let name = first
                            .get("attributes")
                            .and_then(|a| a.get("name").and_then(|n| n.as_str()))
                            .unwrap_or("?");
                        println!("      [0] type={typ} id={id} name={name:?}");
                    }
                }
            }
        }
        Err(e) => println!("ERROR artist views: {e}"),
    }

    // Playlist detail
    match api
        .send_request(
            "/v1/catalog/{storefront}/playlists/pl.74657640b88c4587a426160f7441de46",
            &[("include", "tracks")],
        )
        .await
    {
        Ok(v) => {
            summarize("GET playlist (Daft Punk Essentials)", &v);
            summarize_deep("Playlist detail", &v, 1);
            if let Some(data) = v
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
            {
                if let Some(tracks) = data
                    .get("relationships")
                    .and_then(|r| r.get("tracks"))
                    .and_then(|t| t.get("data"))
                    .and_then(|d| d.as_array())
                {
                    println!("  TRACKS: {} total", tracks.len());
                    if let Some(t) = tracks.first() {
                        let name = t
                            .get("attributes")
                            .and_then(|a| a.get("name").and_then(|n| n.as_str()))
                            .unwrap_or("?");
                        let id = t.get("id").and_then(|i| i.as_str()).unwrap_or("?");
                        println!("    [0] id={id} name={name:?}");
                    }
                }
                if let Some(next) = data
                    .get("relationships")
                    .and_then(|r| r.get("tracks"))
                    .and_then(|t| t.get("next"))
                    .and_then(|n| n.as_str())
                {
                    println!("  TRACKS NEXT: {next}");
                }
            }
        }
        Err(e) => println!("ERROR playlist: {e}"),
    }

    // ========== REPLAY ==========
    println!("\n\n########## REPLAY ##########");

    match api
        .send_request(
            "/v1/me/music-summaries",
            &[
                ("filter[year]", "latest"),
                ("views", "top-artists,top-albums,top-songs"),
            ],
        )
        .await
    {
        Ok(v) => {
            summarize("GET /v1/me/music-summaries", &v);
            if let Some(data) = v
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
            {
                let year = data.get("attributes").and_then(|a| a.get("year"));
                println!("  Year: {year:?}");
                if let Some(attrs) = data.get("attributes").and_then(|a| a.as_object()) {
                    println!("  Attribute keys: {:?}", attrs.keys().collect::<Vec<_>>());
                }
                if let Some(views) = data.get("views").and_then(|v| v.as_object()) {
                    println!("  VIEWS present: {:?}", views.keys().collect::<Vec<_>>());
                    for (vk, vv) in views.iter() {
                        let data_len = vv.get("data").and_then(|d| d.as_array()).map(|a| a.len());
                        println!("    VIEW {vk}: data.len={data_len:?}");
                        if let Some(arr) = vv.get("data").and_then(|d| d.as_array()) {
                            for (i, item) in arr.iter().enumerate().take(3) {
                                let typ = item.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                                let id = item.get("id").and_then(|i| i.as_str()).unwrap_or("?");
                                let listen_count =
                                    item.get("attributes").and_then(|a| a.get("listenCount"));
                                let play_count =
                                    item.get("attributes").and_then(|a| a.get("playCount"));
                                println!(
                                    "      [{i}] type={typ} id={id} listenCount={listen_count:?} playCount={play_count:?}"
                                );
                                if let Some(attrs) =
                                    item.get("attributes").and_then(|a| a.as_object())
                                {
                                    println!(
                                        "        attribute keys: {:?}",
                                        attrs.keys().collect::<Vec<_>>()
                                    );
                                }
                                if let Some(rels) =
                                    item.get("relationships").and_then(|r| r.as_object())
                                {
                                    for (rk, rv) in rels.iter() {
                                        if let Some(rd) = rv.get("data").and_then(|d| d.as_array())
                                            && let Some(first) = rd.first()
                                        {
                                            let rt = first
                                                .get("type")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("?");
                                            let ri = first
                                                .get("id")
                                                .and_then(|i| i.as_str())
                                                .unwrap_or("?");
                                            let rn = first
                                                .get("attributes")
                                                .and_then(|a| {
                                                    a.get("name").and_then(|n| n.as_str())
                                                })
                                                .unwrap_or("?");
                                            println!(
                                                "        {rk} -> type={rt} id={ri} name={rn:?}"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => println!("ERROR music-summaries: {e}"),
    }

    println!("\n\n########## ALL PROBES COMPLETE ##########");
}
