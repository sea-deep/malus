//! Right-hand utility panes and sheets for Malus GTK.

pub mod credits;
pub mod lyrics;
pub mod queue;

pub use credits::show_credits_dialog;
pub use lyrics::LyricsPane;
pub use queue::{QueueCmd, QueueInput, QueuePane};
