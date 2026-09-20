//! Reusable presentation widgets for Malus GTK.

pub mod actions_menu;
pub mod empty_state;
pub mod loading;
pub mod media_card;
pub mod media_shelf;
pub mod section_header;
pub mod track_row;

pub use actions_menu::{
    ActionMenuCommand, build_action_popover, build_action_popover_with_actions,
};
pub use empty_state::create_empty_state;
pub use loading::create_loading_spinner;
pub use media_card::{MediaCard, MediaCardInit, MediaCardInput, MediaCardOutput};
pub use media_shelf::{create_shelf_container, hook_shelf_artwork_trigger};
pub use section_header::{SectionHeader, SectionHeaderInit, SectionHeaderInput};
pub use track_row::{TrackRow, TrackRowInit, TrackRowInput, TrackRowOutput};

pub mod category_card;
pub mod featured_banner_card;
pub mod interactive_scale;
pub mod live_station_pill;
pub mod multirow_episode_row;
pub mod multirow_track_row;
pub mod player_controls;
pub mod square_artwork;
pub mod station_card;
pub mod virtual_track_list;

pub use category_card::{CategoryCard, CategoryCardInit, CategoryCardInput, CategoryCardOutput};

pub use featured_banner_card::{
    FeaturedBannerCard, FeaturedBannerCardInit, FeaturedBannerCardInput, FeaturedBannerCardOutput,
};
pub use live_station_pill::{
    LiveStationPill, LiveStationPillInit, LiveStationPillInput, LiveStationPillOutput,
};
pub use multirow_episode_row::{
    MultiRowEpisodeRow, MultiRowEpisodeRowInit, MultiRowEpisodeRowInput, MultiRowEpisodeRowOutput,
};
pub use multirow_track_row::{
    MultiRowTrackRow, MultiRowTrackRowInit, MultiRowTrackRowInput, MultiRowTrackRowOutput,
};
pub use station_card::{StationCard, StationCardInit, StationCardInput, StationCardOutput};
pub use virtual_track_list::VirtualTrackList;
