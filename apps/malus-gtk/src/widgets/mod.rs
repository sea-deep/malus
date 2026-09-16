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

pub mod interactive_scale;
pub mod player_controls;
pub mod square_artwork;
pub mod virtual_track_list;

pub use virtual_track_list::VirtualTrackList;
