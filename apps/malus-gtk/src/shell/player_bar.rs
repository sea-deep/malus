//! Compact, full-width bottom transport. No independent playback model or timer.
use crate::{
    design::tokens::*,
    services::{DecodedImage, artwork_texture},
    state::{SharedPlayer, UtilityMode},
    widgets::{actions_menu::ActionMenuCommand, player_controls::*, square_artwork::SquareArtwork},
};
use malus_model::{MediaRef, PageRoute};
use relm4::gtk::{self, prelude::*};

pub struct PlayerBar {
    pub root: gtk::Box,
    artwork: SquareArtwork,
    title: gtk::Label,
    artist: gtk::Label,
    favorite: gtk::Button,
    more: gtk::MenuButton,
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
        root.set_valign(gtk::Align::Fill);
        let layout = gtk::CenterBox::new();
        layout.set_hexpand(true);
        root.append(&layout);

        let left = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        left.set_hexpand(false);
        left.set_valign(gtk::Align::Center);
        let identity = gtk::Button::new();
        identity.add_css_class("flat");
        identity.add_css_class("player-identity");
        identity.set_tooltip_text(Some("Open Now Playing"));
        identity.update_property(&[gtk::accessible::Property::Label("Open Now Playing")]);
        identity.set_focus_on_click(false);
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

        let p_art = player.clone();
        let cb_art = menu.clone();
        let art_gesture = gtk::GestureClick::new();
        art_gesture.connect_released(move |gesture, _n, _x, _y| {
            let state = p_art.borrow();
            if let Some(t) = &state.now.current_track
                && let Some(art_ref) = t.artists.first()
                && let Some(MediaRef::Artist(ref id)) = art_ref.id
            {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                cb_art(ActionMenuCommand::Navigate(PageRoute::Artist(id.clone())));
            }
        });
        artist.add_controller(art_gesture);

        metadata.append(&title);
        metadata.append(&artist);
        info.append(&metadata);
        identity.set_child(Some(&info));
        let open = std::rc::Rc::new(open);
        let cb = open.clone();
        identity.connect_clicked(move |_| cb());
        let favorite = favorite_button(player, menu);
        favorite.set_valign(gtk::Align::Center);
        let more = more_button(player, menu);
        more.set_valign(gtk::Align::Center);

        let right_click = gtk::GestureClick::new();
        right_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        let p_rc = player.clone();
        let cb_rc = menu.clone();
        let left_weak = left.downgrade();
        right_click.connect_pressed(move |_gesture, _n_press, x, y| {
            let Some(left_widget) = left_weak.upgrade() else {
                return;
            };
            popup_track_context_menu(&left_widget, x, y, &p_rc, &cb_rc);
        });
        left.add_controller(right_click);

        left.append(&identity);
        left.append(&favorite);
        left.append(&more);
        layout.set_start_widget(Some(&left));

        let center = gtk::Box::new(gtk::Orientation::Vertical, 0);
        center.set_valign(gtk::Align::Center);
        center.set_halign(gtk::Align::Center);
        center.set_hexpand(true);
        let transport = Transport::new(player, send, false);
        let seek = SeekControl::new(player, send, false);
        center.append(&transport.root);
        center.append(&seek.root);
        layout.set_center_widget(Some(&center));

        let right = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        right.set_hexpand(false);
        right.set_halign(gtk::Align::End);
        right.set_valign(gtk::Align::Center);
        let volume = VolumeControl::new_popover(send);
        let lyrics_button = icon_button(ICON_LYRICS, "Lyrics (Ctrl+L)", "player-icon-btn");
        lyrics_button.add_css_class("control-inactive");
        lyrics_button.set_focus_on_click(false);
        lyrics_button.connect_clicked(move |_| lyrics());
        let queue_button = icon_button(ICON_QUEUE, "Queue (Ctrl+Q)", "player-icon-btn");
        queue_button.add_css_class("control-inactive");
        queue_button.set_focus_on_click(false);
        queue_button.connect_clicked(move |_| queue());
        right.append(&lyrics_button);
        right.append(&queue_button);
        right.append(&volume.root);
        layout.set_end_widget(Some(&right));

        // Responsive width adjustments
        let favorite_button = favorite.clone();
        let more_button = more.clone();
        let title_label = title.clone();
        let artist_label = artist.clone();
        let center_box = center.clone();
        let volume_box = volume.root.clone();
        let lyrics_btn = lyrics_button.clone();
        let queue_btn = queue_button.clone();
        let shuffle_btn = transport.shuffle.clone();
        let repeat_btn = transport.repeat.clone();
        let last_width = std::cell::Cell::new(0);
        root.add_tick_callback(move |root, _| {
            let width = root.width();
            if width > 0 && last_width.replace(width) != width {
                let compact = width < 850;
                let narrow = width < 680;
                let very_narrow = width < 560;
                if very_narrow {
                    center_box.set_width_request(-1);
                } else if narrow {
                    let center_width = (width - 340).clamp(120, 260);
                    center_box.set_width_request(center_width);
                } else {
                    let center_width = (width - 340).clamp(160, if compact { 320 } else { 520 });
                    center_box.set_width_request(center_width);
                }
                let max_chars = if very_narrow {
                    6
                } else if narrow {
                    9
                } else if compact {
                    14
                } else {
                    24
                };
                title_label.set_max_width_chars(max_chars);
                artist_label.set_max_width_chars(max_chars);

                shuffle_btn.set_visible(width >= 620);
                repeat_btn.set_visible(width >= 620);
                lyrics_btn.set_visible(width >= 560);
                queue_btn.set_visible(width >= 500);
                favorite_button.set_visible(width >= 520);
                more_button.set_visible(width >= 460);
                volume_box.set_visible(width >= 380);
                metadata.set_visible(width >= 320);
            }
            gtk::glib::ControlFlow::Continue
        });
        Self {
            root,
            artwork,
            title,
            artist,
            favorite,
            more,
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
        let has_artist_id = state
            .now
            .current_track
            .as_ref()
            .and_then(|t| t.artists.first())
            .and_then(|a| a.id.as_ref())
            .is_some();
        if has_artist_id {
            self.artist.add_css_class("metadata-link");
            self.artist.set_cursor_from_name(Some("pointer"));
        } else {
            self.artist.remove_css_class("metadata-link");
            self.artist.set_cursor_from_name(None);
        }
        let has_track = state.now.current_track.is_some();
        drop(state);
        self.transport.refresh(player);
        refresh_favorite(&self.favorite, player);
        self.more.set_sensitive(has_track);
        self.tick(player);
        for (button, active) in [
            (&self.lyrics, mode.is_lyrics()),
            (&self.queue, mode.is_queue()),
        ] {
            if active {
                button.add_css_class("control-active");
                button.add_css_class("utility-active");
                button.remove_css_class("control-inactive");
            } else {
                button.remove_css_class("control-active");
                button.remove_css_class("utility-active");
                button.add_css_class("control-inactive");
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
