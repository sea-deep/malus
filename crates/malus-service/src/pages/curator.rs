//! Apple Curator Page Resolver.
//!
//! Builds structured `PageWire` feeds for Apple Curators (`curator:<id>`).

use malus_ipc::wire::{PageHeaderWire, PageWire};

use crate::api::error::AppleApiError;
use crate::api::official::OfficialAppleMusicApi;
use crate::api::parse::parse_apple_artwork;
use crate::pages::groupings::map_groupings_response;

/// Build a curator product page.
pub async fn build_curator_page(
    curator_id: &str,
    api: &OfficialAppleMusicApi,
) -> Result<PageWire, AppleApiError> {
    let resp = api.get_curator_grouping(curator_id).await?;

    let res = resp.get("resources");
    let curators = res
        .and_then(|r| r.get("apple-curators").or_else(|| r.get("curators")))
        .and_then(|c| c.as_object());

    let curator_obj = curators
        .and_then(|c| c.get(curator_id))
        .or_else(|| curators.and_then(|c| c.values().next()));

    let attrs = curator_obj.and_then(|c| c.get("attributes"));
    let title = attrs
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("Curator")
        .to_string();

    let subtitle = attrs
        .and_then(|a| a.get("editorialNotes"))
        .and_then(|e| e.get("standard").or_else(|| e.get("short")))
        .and_then(|s| s.as_str())
        .map(str::to_string);

    let artwork = attrs
        .and_then(|a| a.get("artwork"))
        .and_then(parse_apple_artwork);

    let banner_artwork = attrs
        .and_then(|a| a.get("editorialArtwork"))
        .and_then(|ea| {
            ea.get("bannerSuperHero")
                .or_else(|| ea.get("bannerUber"))
                .or_else(|| ea.get("staticBannerUber"))
                .or_else(|| ea.get("original"))
        })
        .and_then(parse_apple_artwork);

    let header = Some(PageHeaderWire {
        title: title.clone(),
        subtitle: subtitle.clone(),
        subtitle_route: None,
        artwork,
        banner_artwork,
        badges: Vec::new(),
        description: subtitle.clone(),
        metadata: Vec::new(),
        actions: Vec::new(),
        can_edit: false,
        can_delete: false,
    });

    let sections = map_groupings_response(&resp, "curator");

    Ok(PageWire {
        id: format!("curator:{curator_id}"),
        title,
        subtitle,
        header,
        actions: Vec::new(),
        sections,
        continuation: None,
    })
}
