//! Browsing utility pane. Lyrics are a view of the shared player state.
use crate::{
    panes::{
        lyrics::LyricsPane,
        queue::{QueueInput, QueuePane},
    },
    services::ArtworkService,
    state::{SharedPlayer, UtilityMode},
    widgets::player_controls::CommandHandler,
};
use malus_client::MalusClient;
use malus_model::Queue;
use relm4::{
    gtk::{self, prelude::*},
    prelude::*,
};

pub struct UtilityPane {
    pub root: gtk::Box,
    stack: gtk::Stack,
    queue: Controller<QueuePane>,
    pub lyrics: LyricsPane,
}
impl UtilityPane {
    pub fn new(
        client: MalusClient,
        artwork: ArtworkService,
        player: &SharedPlayer,
        send: &CommandHandler,
        expand: impl Fn() + 'static,
        close: impl Fn() + 'static,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("utility-pane-container");
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_vexpand(true);
        let close_rc = std::rc::Rc::new(close);
        let c1 = close_rc.clone();
        let queue = QueuePane::builder()
            .launch((client, artwork, c1, false))
            .detach();
        let c2 = close_rc.clone();
        let lyrics = LyricsPane::new(player, send, expand, move || c2());
        stack.add_named(queue.widget(), Some("queue"));
        stack.add_named(&lyrics.root, Some("lyrics"));
        root.append(&stack);
        Self {
            root,
            stack,
            queue,
            lyrics,
        }
    }
    pub fn set_mode(&self, mode: UtilityMode) {
        match mode {
            UtilityMode::Queue => {
                self.stack.set_visible_child_name("queue");
                self.queue.emit(QueueInput::ScrollToNowPlaying);
            }
            UtilityMode::Lyrics => {
                self.stack.set_visible_child_name("lyrics");
                self.lyrics.view.reveal_current();
            }
            UtilityMode::Closed => {}
        }
    }
    pub fn scroll_to_now_playing(&self) {
        self.queue.emit(QueueInput::ScrollToNowPlaying);
    }
    pub fn set_queue(&self, queue: Queue) {
        self.queue.emit(QueueInput::SetQueue(queue));
    }
    pub fn set_playback_state(&self, state: malus_model::PlaybackState) {
        self.queue.emit(QueueInput::SetPlaybackState(state));
    }
    pub fn set_autoplay(&self, autoplay: bool) {
        self.queue.emit(QueueInput::SetAutoplay(autoplay));
    }
    pub fn reload_queue(&self) {
        self.queue.emit(QueueInput::Reload);
    }
}
