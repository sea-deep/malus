//! Compact, full-width bottom transport. No independent playback model or timer.
use crate::{
    design::tokens::*,
    services::{DecodedImage, artwork_texture},
    state::{SharedPlayer, UtilityMode},
    widgets::{player_controls::*, square_artwork::SquareArtwork},
};
use relm4::gtk::{self, prelude::*};

pub struct PlayerBar {
    pub root: gtk::Box,
    artwork: SquareArtwork,
    title: gtk::Label,
    artist: gtk::Label,
    favorite: gtk::Button,
    transport: Transport,
    seek: SeekControl,
    volume: VolumeControl,
    lyrics: gtk::Button,
    queue: gtk::Button,
}
impl PlayerBar {
    pub fn new(
        player: &SharedPlayer,
        send: &CommandHandler,
        menu: &MenuHandler,
        open: impl Fn() + 'static,
        lyrics: impl Fn() + 'static,
        queue: impl Fn() + 'static,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("player-bar");
        root.set_height_request(PLAYER_TOTAL_HEIGHT);
        root.set_vexpand(false);
        root.set_valign(gtk::Align::End);
        let layout = gtk::CenterBox::new();
        layout.set_hexpand(true);
        root.append(&layout);

        let left = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        left.set_hexpand(false);
        left.set_width_request(160);
        let identity = gtk::Button::new();
        identity.add_css_class("flat");
        identity.add_css_class("player-identity");
        identity.set_tooltip_text(Some("Open Now Playing"));
        identity.update_property(&[gtk::accessible::Property::Label("Open Now Playing")]);
        let info = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let artwork = SquareArtwork::new(PLAYER_ARTWORK_SIZE, "player-artwork");
        info.append(&artwork);
        let metadata = gtk::Box::new(gtk::Orientation::Vertical, 2);
        metadata.set_valign(gtk::Align::Center);
        metadata.set_hexpand(false);
        let title = gtk::Label::new(Some("Not Playing"));
        let artist = gtk::Label::new(Some("Choose something to listen to"));
        for label in [&title, &artist] {
            label.set_xalign(0.0);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(16);
            label.set_width_chars(1);
        }
        title.add_css_class("player-track-title");
        artist.add_css_class("player-track-artist");
        metadata.append(&title);
        metadata.append(&artist);
        info.append(&metadata);
        identity.set_child(Some(&info));
        let open = std::rc::Rc::new(open);
        let cb = open.clone();
        identity.connect_clicked(move |_| cb());
        let favorite = favorite_button(player, menu);
        favorite.set_valign(gtk::Align::Center);
        left.append(&identity);
        left.append(&favorite);
        layout.set_start_widget(Some(&left));

        let center = gtk::Box::new(gtk::Orientation::Vertical, 0);
        center.set_valign(gtk::Align::Center);
        center.set_halign(gtk::Align::Center);
        center.set_hexpand(false);
        let transport = Transport::new(player, send, false);
        let seek = SeekControl::new(player, send, false);
        center.append(&transport.root);
        center.append(&seek.root);
        layout.set_center_widget(Some(&center));

        let right = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        right.set_hexpand(false);
        right.set_halign(gtk::Align::End);
        right.set_valign(gtk::Align::Center);
        let volume = VolumeControl::new(send, 64);
        let lyrics_button = icon_button(ICON_LYRICS, "Lyrics (Ctrl+L)", "player-icon-btn");
        lyrics_button.connect_clicked(move |_| lyrics());
        let queue_button = icon_button(ICON_QUEUE, "Playing Next (Ctrl+U)", "player-icon-btn");
        queue_button.connect_clicked(move |_| queue());
        let expand = icon_button(
            "view-fullscreen-symbolic",
            "Expand Now Playing",
            "player-icon-btn",
        );
        expand.connect_clicked(move |_| open());
        right.append(&volume.root);
        right.append(&lyrics_button);
        right.append(&queue_button);
        right.append(&expand);
        layout.set_end_widget(Some(&right));

        // Responsive width adjustments
        let favorite_button = favorite.clone();
        let title_label = title.clone();
        let artist_label = artist.clone();
        let center_box = center.clone();
        let left_box = left.clone();
        let volume_box = volume.root.clone();
        let last_width = std::cell::Cell::new(0);
        root.add_tick_callback(move |root, _| {
            let width = root.width();
            if last_width.replace(width) != width {
                let compact = width < 850;
                let narrow = width < 680;
                left_box.set_width_request(if narrow {
                    110
                } else if compact {
                    140
                } else {
                    180
                });
                let center_width = (width - 440).clamp(120, if compact { 220 } else { 320 });
                center_box.set_width_request(center_width);
                let max_chars = if narrow {
                    10
                } else if compact {
                    14
                } else {
                    20
                };
                title_label.set_max_width_chars(max_chars);
                artist_label.set_max_width_chars(max_chars);
                favorite_button.set_visible(width >= 480);
                metadata.set_visible(width >= 340);
                volume_box.set_visible(width >= 560);
            }
            gtk::glib::ControlFlow::Continue
        });
        Self {
            root,
            artwork,
            title,
            artist,
            favorite,
            transport,
            seek,
            volume,
            lyrics: lyrics_button,
            queue: queue_button,
        }
    }
    pub fn refresh(&self, player: &SharedPlayer, mode: UtilityMode) {
        let state = player.borrow();
        self.title.set_text(
            state
                .now
                .current_track
                .as_ref()
                .map(|t| t.title.as_str())
                .unwrap_or("Not Playing"),
        );
        self.artist.set_text(
            &state
                .now
                .current_track
                .as_ref()
                .map(|t| t.artist_display())
                .unwrap_or_default(),
        );
        drop(state);
        self.transport.refresh(player);
        refresh_favorite(&self.favorite, player);
        self.tick(player);
        for (button, active) in [
            (&self.lyrics, mode.is_lyrics()),
            (&self.queue, mode.is_queue()),
        ] {
            if active {
                button.add_css_class("utility-active");
            } else {
                button.remove_css_class("utility-active");
            }
        }
    }
    pub fn tick(&self, player: &SharedPlayer) {
        self.seek.refresh(player);
        self.volume.refresh(player);
    }
    pub fn set_artwork(&self, image: Option<&DecodedImage>) {
        self.artwork
            .picture()
            .set_paintable(image.map(artwork_texture).as_ref());
    }
    pub fn cancel_interactions(&self) {
        self.seek.slider.cancel();
        self.volume.slider.cancel();
    }
}
