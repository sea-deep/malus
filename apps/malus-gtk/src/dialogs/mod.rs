//! Modal dialogs for playlist creation, editing, and deletion.

pub mod add_to_playlist;
pub mod edit_playlist;
pub mod new_playlist;

pub use add_to_playlist::show_add_to_playlist_dialog;
pub use edit_playlist::{show_delete_playlist_dialog, show_edit_playlist_dialog};
pub use new_playlist::show_new_playlist_dialog;
