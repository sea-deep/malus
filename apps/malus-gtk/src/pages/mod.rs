//! Page components for Malus GTK.

pub mod feed;
pub mod now_playing;
pub mod search;
pub mod settings;

pub use feed::{FeedCmd, FeedInput, FeedOutput, FeedPage};
pub use now_playing::NowPlayingPage;
pub use search::{SearchCmd, SearchInput, SearchOutput, SearchPage};
pub use settings::{SettingsCmd, SettingsInput, SettingsPage};
