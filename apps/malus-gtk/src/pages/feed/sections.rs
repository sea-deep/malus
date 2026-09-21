use super::*;
use crate::widgets::square_artwork::SquareArtwork;

impl FeedPage {
    pub(super) fn build_hero_header(
        &mut self,
        header: &PageHeaderWire,
        sender: ComponentSender<Self>,
    ) -> gtk::Box {
        // Only catalog artists (not library artists, albums, or playlists) render a wide banner hero.
        // Albums and playlists always keep their classic, proper square artwork hero.
        let is_catalog_artist = matches!(
            self.route,
            PageRoute::Artist(ref id) if !id.starts_with("r.") && !id.starts_with("l.")
        );
        if is_catalog_artist && let Some(ref banner) = header.banner_artwork {
            let root = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(SPACING_MD)
                .margin_bottom(SPACING_LG)
                .build();

            let overlay = gtk::Overlay::builder()
                .css_classes(vec!["hero-banner-container".to_string()])
                .height_request(BANNER_HERO_HEIGHT)
                .build();

            let pic = gtk::Picture::builder()
                .can_shrink(true)
                .content_fit(gtk::ContentFit::Cover)
                .height_request(BANNER_HERO_HEIGHT)
                .css_classes(vec!["hero-banner-picture".to_string()])
                .build();
            bind_artwork(&pic, &self.artwork_service, Some(banner.url.clone()), 1400);
            overlay.set_child(Some(&pic));

            let scrim = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(SPACING_XS)
                .valign(gtk::Align::End)
                .hexpand(true)
                .css_classes(vec!["hero-banner-scrim".to_string()])
                .build();

            let title_label = gtk::Label::builder()
                .label(&header.title)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .css_classes(vec!["hero-banner-title".to_string()])
                .build();
            scrim.append(&title_label);

            if let Some(ref sub) = header.subtitle {
                let sub_label = gtk::Label::builder()
                    .label(sub)
                    .xalign(0.0)
                    .wrap(true)
                    .wrap_mode(gtk::pango::WrapMode::WordChar)
                    .css_classes(vec!["hero-banner-metadata".to_string()])
                    .build();
                if let Some(ref route) = header.subtitle_route {
                    sub_label.add_css_class("metadata-link");
                    sub_label.set_cursor_from_name(Some("pointer"));
                    let s = sender.clone();
                    let rt = route.clone();
                    let g = gtk::GestureClick::new();
                    g.connect_released(move |g, n, _, _| {
                        if n == 1 {
                            g.set_state(gtk::EventSequenceState::Claimed);
                            let _ = s.output(FeedOutput::Navigate(rt.clone()));
                        }
                    });
                    sub_label.add_controller(g);
                }
                scrim.append(&sub_label);
            }

            if !header.metadata.is_empty() {
                let meta_str = header
                    .metadata
                    .iter()
                    .filter(|value| !value.starts_with("catalog_id:"))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" • ");
                if !meta_str.is_empty() {
                    let meta_label = gtk::Label::builder()
                        .label(&meta_str)
                        .xalign(0.0)
                        .wrap(true)
                        .css_classes(vec!["hero-banner-metadata".to_string()])
                        .build();
                    scrim.append(&meta_label);
                }
            }

            overlay.add_overlay(&scrim);
            root.append(&overlay);

            let actions_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
            actions_host.append(&self.build_header_actions(header, sender));
            root.append(&actions_host);
            self.header_actions_host = Some(actions_host);

            return root;
        }

        let is_artist = matches!(self.route, PageRoute::Artist(_));
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(28)
            .margin_bottom(24)
            .css_classes(vec!["hero-header".to_string()])
            .build();

        let art = header.artwork.as_ref().map(|artwork| {
            let art = crate::widgets::square_artwork::SquareArtwork::new(
                ARTWORK_HERO_SIZE,
                "hero-artwork",
            );
            if is_artist {
                art.add_css_class("card-artwork-circular");
            }
            bind_artwork(
                art.picture(),
                &self.artwork_service,
                Some(artwork.url.clone()),
                (ARTWORK_HERO_SIZE * 2) as u32,
            );
            root.append(&art);
            art
        });

        // Right details column
        let details = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .valign(gtk::Align::End)
            .hexpand(true)
            .build();

        // Title
        let title_label = gtk::Label::builder()
            .label(&header.title)
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .max_width_chars(24)
            .css_classes(vec!["hero-title".to_string()])
            .build();
        details.append(&title_label);

        // Subtitle (Artist / Curator)
        if !header.subtitle_links.is_empty() {
            let sub_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            sub_box.add_css_class("hero-subtitle-box");
            sub_box.set_halign(gtk::Align::Start);
            for (i, link) in header.subtitle_links.iter().enumerate() {
                if i > 0 {
                    let sep_str = if header.subtitle_links.len() == 2
                        || i == header.subtitle_links.len() - 1
                    {
                        " & "
                    } else {
                        ", "
                    };
                    let sep = gtk::Label::new(Some(sep_str));
                    sep.add_css_class("hero-subtitle");
                    sub_box.append(&sep);
                }
                let sub_label = gtk::Label::builder()
                    .label(&link.text)
                    .xalign(0.0)
                    .wrap(true)
                    .wrap_mode(gtk::pango::WrapMode::WordChar)
                    .css_classes(vec!["hero-subtitle".to_string()])
                    .build();
                if let Some(ref route) = link.route {
                    sub_label.add_css_class("metadata-link");
                    sub_label.set_cursor_from_name(Some("pointer"));
                    let s = sender.clone();
                    let rt = route.clone();
                    let g = gtk::GestureClick::new();
                    g.connect_released(move |g, n, _, _| {
                        if n == 1 {
                            g.set_state(gtk::EventSequenceState::Claimed);
                            let _ = s.output(FeedOutput::Navigate(rt.clone()));
                        }
                    });
                    sub_label.add_controller(g);
                }
                sub_box.append(&sub_label);
            }
            details.append(&sub_box);
        } else if let Some(ref sub) = header.subtitle {
            let sub_label = gtk::Label::builder()
                .label(sub)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .css_classes(vec!["hero-subtitle".to_string()])
                .build();
            if let Some(ref route) = header.subtitle_route {
                sub_label.add_css_class("metadata-link");
                sub_label.set_cursor_from_name(Some("pointer"));
                let s = sender.clone();
                let rt = route.clone();
                let g = gtk::GestureClick::new();
                g.connect_released(move |g, n, _, _| {
                    if n == 1 {
                        g.set_state(gtk::EventSequenceState::Claimed);
                        let _ = s.output(FeedOutput::Navigate(rt.clone()));
                    }
                });
                sub_label.add_controller(g);
            }
            details.append(&sub_label);
        }

        // Metadata chips (release, genres, etc.)
        if !header.metadata.is_empty() {
            let meta_str = header
                .metadata
                .iter()
                .filter(|value| !value.starts_with("catalog_id:"))
                .cloned()
                .collect::<Vec<_>>()
                .join(" • ");
            let meta_label = gtk::Label::builder()
                .label(&meta_str)
                .xalign(0.0)
                .wrap(true)
                .css_classes(vec!["hero-metadata".to_string()])
                .build();
            details.append(&meta_label);
        }

        let actions_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        actions_host.append(&self.build_header_actions(header, sender));
        details.append(&actions_host);
        self.header_actions_host = Some(actions_host);
        root.append(&details);
        let last = std::cell::Cell::new(None::<bool>);
        root.add_tick_callback(move |header, _| {
            let w = header.width();
            if w > 0 {
                let narrow = w < 580;
                if last.get() != Some(narrow) {
                    last.set(Some(narrow));
                    header.set_orientation(if narrow {
                        gtk::Orientation::Vertical
                    } else {
                        gtk::Orientation::Horizontal
                    });
                    if let Some(art) = &art {
                        art.set_side(if narrow { 176 } else { ARTWORK_HERO_SIZE });
                    }
                    details.set_valign(if narrow {
                        gtk::Align::Start
                    } else {
                        gtk::Align::End
                    });
                }
            }
            glib::ControlFlow::Continue
        });

        root
    }

    pub(super) fn build_header_actions(
        &self,
        header: &PageHeaderWire,
        sender: ComponentSender<Self>,
    ) -> gtk::FlowBox {
        // Action Buttons Row (Play, Shuffle, Favorite, Add)
        let actions_row = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(false)
            .min_children_per_line(1)
            .max_children_per_line(6)
            .column_spacing(8)
            .row_spacing(8)
            .margin_top(12)
            .halign(gtk::Align::Start)
            .build();
        for action in &header.actions {
            match action {
                PageActionWire::Play(reference) => {
                    for (label, icon, shuffle) in
                        [("Play", ICON_PLAY, false), ("Shuffle", ICON_SHUFFLE, true)]
                    {
                        let button = page_action_button(label, icon);
                        button.add_css_class(if shuffle { "flat" } else { "suggested-action" });
                        button.add_css_class("hero-play-btn");
                        button.set_focus_on_click(false);
                        let reference = reference.clone();
                        let sender = sender.clone();
                        button.connect_clicked(move |_| {
                            let _ = sender.output(FeedOutput::PlayCollection {
                                reference: reference.clone(),
                                shuffle,
                            });
                        });
                        actions_row.append(&button);
                    }
                }
                PageActionWire::Favorite(_)
                | PageActionWire::Unfavorite(_)
                | PageActionWire::AddToLibrary(_) => {
                    let (icon, label) = match action {
                        PageActionWire::Favorite(_) => (ICON_FAVORITE_OUTLINE, "Favorite"),
                        PageActionWire::Unfavorite(_) => (ICON_FAVORITE, "Unfavorite"),
                        _ => (ICON_ADD, "Add to Library"),
                    };
                    let button = gtk::Button::from_icon_name(icon);
                    button.set_tooltip_text(Some(label));
                    button.update_property(&[gtk::accessible::Property::Label(label)]);
                    button.add_css_class("flat");
                    button.add_css_class("hero-action-btn");
                    button.set_focus_on_click(false);
                    if matches!(action, PageActionWire::Unfavorite(_)) {
                        button.add_css_class("favorite-active");
                    }
                    let action = action.clone();
                    let sender = sender.clone();
                    button.connect_clicked(move |_| {
                        let _ = sender.output(FeedOutput::Action(action.clone()));
                    });
                    actions_row.append(&button);
                }
                _ => {}
            }
        }
        if let Some(reference) = header.actions.iter().find_map(|action| {
            if let PageActionWire::Play(reference) = action {
                Some(reference)
            } else {
                None
            }
        }) {
            let button = gtk::MenuButton::new();
            button.set_icon_name(ICON_MORE);
            button.set_tooltip_text(Some("More actions"));
            button.add_css_class("flat");
            button.add_css_class("hero-action-btn");
            button.set_focus_on_click(false);
            let s = sender.clone();
            let popover = crate::widgets::actions_menu::build_action_popover_with_actions(
                reference,
                header
                    .actions
                    .iter()
                    .any(|a| matches!(a, PageActionWire::Unfavorite(_))),
                !header
                    .actions
                    .iter()
                    .any(|a| matches!(a, PageActionWire::AddToLibrary(_))),
                false,
                &header.actions,
                move |command| match command {
                    crate::widgets::actions_menu::ActionMenuCommand::Action(action) => {
                        let _ = s.output(FeedOutput::Action(action));
                    }
                    crate::widgets::actions_menu::ActionMenuCommand::ViewCredits(reference) => {
                        let _ = s.output(FeedOutput::ViewCredits(reference));
                    }
                    crate::widgets::actions_menu::ActionMenuCommand::AddToPlaylist(reference) => {
                        let _ = s.output(FeedOutput::ShowAddToPlaylist(reference));
                    }
                    crate::widgets::actions_menu::ActionMenuCommand::RemoveFromPlaylist {
                        playlist,
                        track_index,
                        expected_track,
                    } => {
                        let _ = s.output(FeedOutput::RemoveTrackFromPlaylist {
                            playlist,
                            track_index,
                            expected_track,
                        });
                    }
                    crate::widgets::actions_menu::ActionMenuCommand::Navigate(route) => {
                        let _ = s.output(FeedOutput::Navigate(route));
                    }
                    crate::widgets::actions_menu::ActionMenuCommand::CopyLink(url) => {
                        let _ = s.output(FeedOutput::CopyLink(url));
                    }
                },
            );
            button.set_popover(Some(&popover));
            actions_row.append(&button);
        }

        if header.can_edit {
            let btn_edit = gtk::Button::builder()
                .icon_name("document-edit-symbolic")
                .css_classes(vec!["flat".to_string(), "hero-action-btn".to_string()])
                .tooltip_text("Edit Playlist")
                .focus_on_click(false)
                .build();
            let s = sender.clone();
            let pl_ref = match self.route {
                PageRoute::Playlist(ref id) => MediaRef::Playlist(id.clone()),
                _ => MediaRef::Playlist(header.title.clone()),
            };
            let title = header.title.clone();
            let desc = header.metadata.first().cloned();
            btn_edit.connect_clicked(move |_| {
                let _ = s.output(FeedOutput::ShowEditPlaylistDialog {
                    playlist_ref: pl_ref.clone(),
                    current_name: title.clone(),
                    current_desc: desc.clone(),
                });
            });
            actions_row.append(&btn_edit);
        }

        if header.can_delete {
            let btn_del = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .css_classes(vec!["flat".to_string(), "hero-action-btn".to_string()])
                .tooltip_text("Delete Playlist")
                .focus_on_click(false)
                .build();
            let s = sender.clone();
            let pl_ref = match self.route {
                PageRoute::Playlist(ref id) => MediaRef::Playlist(id.clone()),
                _ => MediaRef::Playlist(header.title.clone()),
            };
            let title = header.title.clone();
            btn_del.connect_clicked(move |_| {
                let _ = s.output(FeedOutput::ShowDeletePlaylistDialog {
                    playlist_ref: pl_ref.clone(),
                    playlist_title: title.clone(),
                });
            });
            actions_row.append(&btn_del);
        }

        actions_row
    }

    pub(super) fn build_section(
        &mut self,
        section: &PageSectionWire,
        section_index: usize,
        sender: ComponentSender<Self>,
    ) -> gtk::Box {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();

        // Section Title & Subtitle
        if let Some(ref title) = section.title {
            let header_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(2)
                .margin_top(8)
                .margin_bottom(4)
                .build();

            let title_lbl = gtk::Label::builder()
                .label(title)
                .xalign(0.0)
                .css_classes(vec!["section-title".to_string()])
                .build();
            header_box.append(&title_lbl);

            if let Some(ref sub) = section.subtitle {
                let sub_lbl = gtk::Label::builder()
                    .label(sub)
                    .wrap(true)
                    .xalign(0.0)
                    .css_classes(vec!["section-subtitle".to_string()])
                    .build();
                header_box.append(&sub_lbl);
            }

            root.append(&header_box);
        }

        let mut items = section.items.clone();
        if matches!(
            self.route,
            PageRoute::LibrarySongs
                | PageRoute::LibraryAlbums
                | PageRoute::LibraryArtists
                | PageRoute::LibraryPlaylists
                | PageRoute::LibraryRecentlyAdded
        ) {
            match self.current_sort {
                LibrarySortMethod::RecentlyAdded => {}
                LibrarySortMethod::Title => items.sort_by(|a, b| a.title.cmp(&b.title)),
                LibrarySortMethod::Artist => items.sort_by(|a, b| {
                    let a_sub = a.subtitle.as_deref().unwrap_or("");
                    let b_sub = b.subtitle.as_deref().unwrap_or("");
                    a_sub.cmp(b_sub).then_with(|| a.title.cmp(&b.title))
                }),
            }
        }

        if section.id == "view:latest-release" && items.len() == 1 {
            let item = &items[0];
            let card = self.build_latest_release_card(item, sender);
            root.append(&card);
            return root;
        }

        match section.presentation_hint.as_deref().unwrap_or("shelf") {
            "track-list" => {
                let track_list = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(2)
                    .valign(gtk::Align::Start)
                    .build();

                let playlist_ctx = match (
                    &self.route,
                    self.page_data.as_ref().and_then(|p| p.header.as_ref()),
                ) {
                    (PageRoute::Playlist(id), Some(h)) if h.can_edit => {
                        Some(MediaRef::Playlist(id.clone()))
                    }
                    _ => None,
                };

                // Determine collection context for canonical playback routing.
                let collection_ref = match &self.route {
                    PageRoute::Album(id) => Some(MediaRef::Album(id.clone())),
                    PageRoute::Playlist(id) => Some(MediaRef::Playlist(id.clone())),
                    _ => None,
                };

                let show_artwork = !matches!(self.route, PageRoute::Album(_));

                for (idx, item) in items.iter().enumerate() {
                    let track = self.item_to_track(item);
                    let coll_clone = collection_ref.clone();
                    let row = TrackRow::builder()
                        .launch(TrackRowInit {
                            track,
                            index: Some(idx),
                            show_artwork,
                            is_favorite: item.is_favorite(),
                            in_library: item.in_library(),
                            actions: item.actions.clone(),
                            artwork_service: self.artwork_service.clone(),
                            playlist_context: playlist_ctx.as_ref().map(|p| (p.clone(), idx)),
                            album_route: item.album_route.clone(),
                            artist_route: item.artist_route.clone(),
                        })
                        .forward(sender.output_sender(), move |out| match out {
                            TrackRowOutput::Play(r) => FeedOutput::PlayTrack {
                                track: r,
                                collection: coll_clone.clone(),
                                index: Some(idx),
                            },
                            TrackRowOutput::Action(a) => FeedOutput::Action(a),
                            TrackRowOutput::ViewCredits(r) => FeedOutput::ViewCredits(r),
                            TrackRowOutput::AddToPlaylist(r) => FeedOutput::ShowAddToPlaylist(r),
                            TrackRowOutput::RemoveFromPlaylist {
                                playlist,
                                track_index,
                                expected_track,
                            } => FeedOutput::RemoveTrackFromPlaylist {
                                playlist,
                                track_index,
                                expected_track,
                            },
                            TrackRowOutput::Navigate(r) => FeedOutput::Navigate(r),
                            TrackRowOutput::CopyLink(u) => FeedOutput::CopyLink(u),
                        });
                    track_list.append(row.widget());
                    self.track_controllers.push(row);
                }

                let spinner = if section.continuation.is_some() {
                    let sp = Self::create_continuation_spinner();
                    track_list.append(&sp);
                    Some(sp)
                } else {
                    None
                };

                self.mounted_sections.insert(
                    section.id.clone(),
                    MountedSection::TrackList {
                        track_list: track_list.clone(),
                        spinner: std::rc::Rc::new(std::cell::RefCell::new(spinner)),
                        playlist_ctx,
                        collection_ref,
                        show_artwork,
                    },
                );
                root.append(&track_list);
            }
            "grid" => {
                let flow = gtk::FlowBox::builder()
                    .valign(gtk::Align::Start)
                    .max_children_per_line(12)
                    .min_children_per_line(1)
                    .selection_mode(gtk::SelectionMode::None)
                    .homogeneous(true)
                    .column_spacing(GRID_GAP)
                    .row_spacing(GRID_GAP + 6)
                    .build();

                let mut grid_deferred = Vec::new();
                for (idx, item) in items.iter().enumerate() {
                    let is_artist = item
                        .open_route
                        .as_ref()
                        .map(|r| matches!(r, PageRoute::Artist(_)))
                        .unwrap_or(false);
                    let eager = idx < 12;
                    let card = MediaCard::builder()
                        .launch(MediaCardInit {
                            id: item.id.clone(),
                            title: item.title.clone(),
                            subtitle: item.subtitle.clone(),
                            subtitle_route: item.artist_route.clone(),
                            overline: item.tertiary_text.clone(),
                            artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                            entity: item.entity.clone(),
                            open_route: item.open_route.clone(),
                            size: ARTWORK_GRID_SIZE,
                            is_circular: is_artist,
                            artwork_service: self.artwork_service.clone(),
                            eager,
                        })
                        .forward(sender.output_sender(), |out| match out {
                            MediaCardOutput::Navigate(r) => FeedOutput::Navigate(r),
                            MediaCardOutput::Play(r) => FeedOutput::Play(r),
                            MediaCardOutput::CopyLink(u) => FeedOutput::CopyLink(u),
                        });
                    flow.append(card.widget());
                    if !eager {
                        grid_deferred.push(card.sender().clone());
                    }
                    self.card_controllers.push(card);
                }

                root.append(&flow);

                let spinner = if section.continuation.is_some() {
                    let sp = Self::create_continuation_spinner();
                    root.append(&sp);
                    Some(sp)
                } else {
                    None
                };

                self.mounted_sections.insert(
                    section.id.clone(),
                    MountedSection::Grid {
                        flow: flow.clone(),
                        spinner_parent: root.clone(),
                        spinner: std::rc::Rc::new(std::cell::RefCell::new(spinner)),
                    },
                );

                if !grid_deferred.is_empty() {
                    self.deferred_sections
                        .borrow_mut()
                        .push((root.clone().upcast(), grid_deferred));
                }
            }
            "top-picks-shelf" => {
                let (scrolled, shelf_box) = create_shelf_container();
                let mut shelf_senders = Vec::new();
                let mut vert_senders = Vec::new();

                for (idx, item) in items.iter().enumerate() {
                    let eager = section_index < 3 && idx < 6;
                    let card_is_artist = item
                        .open_route
                        .as_ref()
                        .map(|r| matches!(r, PageRoute::Artist(_)))
                        .unwrap_or(false);
                    let card = MediaCard::builder()
                        .launch(MediaCardInit {
                            id: item.id.clone(),
                            title: item.title.clone(),
                            subtitle: item.subtitle.clone(),
                            subtitle_route: item.artist_route.clone(),
                            overline: item.tertiary_text.clone(),
                            artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                            entity: item.entity.clone(),
                            open_route: item.open_route.clone(),
                            size: ARTWORK_SHELF_SIZE,
                            is_circular: card_is_artist,
                            artwork_service: self.artwork_service.clone(),
                            eager,
                        })
                        .forward(sender.output_sender(), |out| match out {
                            MediaCardOutput::Navigate(r) => FeedOutput::Navigate(r),
                            MediaCardOutput::Play(r) => FeedOutput::Play(r),
                            MediaCardOutput::CopyLink(u) => FeedOutput::CopyLink(u),
                        });
                    shelf_box.append(card.widget());
                    if !eager {
                        shelf_senders.push((idx, card.sender().clone()));
                        if idx < 6 {
                            vert_senders.push(card.sender().clone());
                        }
                    }
                    self.card_controllers.push(card);
                }

                let senders_ref =
                    hook_shelf_artwork_trigger(&scrolled, ARTWORK_SHELF_SIZE as f64, shelf_senders);

                self.mounted_sections.insert(
                    section.id.clone(),
                    MountedSection::Shelf {
                        shelf_box: shelf_box.clone(),
                        card_size: ARTWORK_SHELF_SIZE,
                        is_artist: false,
                        shelf_senders: senders_ref,
                    },
                );

                root.append(&scrolled);

                if !vert_senders.is_empty() {
                    self.deferred_sections
                        .borrow_mut()
                        .push((root.clone().upcast(), vert_senders));
                }
            }
            "live-stations-shelf" => {
                let (scrolled, shelf_box) = create_shelf_container();
                for item in items.iter() {
                    let pill = LiveStationPill::builder()
                        .launch(LiveStationPillInit {
                            id: item.id.clone(),
                            title: item.title.clone(),
                            artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                            entity: item.entity.clone(),
                            artwork_service: self.artwork_service.clone(),
                        })
                        .forward(sender.output_sender(), |out| match out {
                            LiveStationPillOutput::Play(r) => FeedOutput::Play(r),
                        });
                    shelf_box.append(pill.widget());
                    self.live_station_controllers.push(pill);
                }
                root.append(&scrolled);
            }
            "gradient-stations-shelf" => {
                let (scrolled, shelf_box) = create_shelf_container();
                for item in items.iter() {
                    let genre_label = item.tertiary_text.clone().unwrap_or_else(|| {
                        item.title
                            .strip_suffix(" Station")
                            .unwrap_or(&item.title)
                            .to_string()
                    });
                    let card = StationCard::builder()
                        .launch(StationCardInit {
                            id: item.id.clone(),
                            title: item.title.clone(),
                            genre_label,
                            subtitle: item.subtitle.clone(),
                            bg_color: item.bg_color.clone(),
                            entity: item.entity.clone(),
                            artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                            artwork_service: Some(self.artwork_service.clone()),
                        })
                        .forward(sender.output_sender(), |out| match out {
                            StationCardOutput::Play(r) => FeedOutput::Play(r),
                            StationCardOutput::Navigate(r) => FeedOutput::Navigate(r),
                        });
                    shelf_box.append(card.widget());
                    self.station_card_controllers.push(card);
                }
                root.append(&scrolled);
            }
            "featured-banner-shelf" => {
                let (scrolled, shelf_box) = create_shelf_container();
                for (idx, item) in items.iter().enumerate() {
                    let eager = idx < 4;
                    let card = FeaturedBannerCard::builder()
                        .launch(FeaturedBannerCardInit {
                            id: item.id.clone(),
                            title: item.title.clone(),
                            subtitle: item.subtitle.clone(),
                            eyebrow: item.tertiary_text.clone(),
                            artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                            entity: item.entity.clone(),
                            open_route: item.open_route.clone(),
                            artwork_service: self.artwork_service.clone(),
                            eager,
                        })
                        .forward(sender.output_sender(), |out| match out {
                            FeaturedBannerCardOutput::Navigate(r) => FeedOutput::Navigate(r),
                            FeaturedBannerCardOutput::Play(r) => FeedOutput::Play(r),
                        });
                    shelf_box.append(card.widget());
                    self.featured_banner_controllers.push(card);
                }
                root.append(&scrolled);
            }
            "multi-row-track-shelf" => {
                let (scrolled, shelf_box) = create_shelf_container();
                let rows_per_col = 4;
                for chunk in items.chunks(rows_per_col) {
                    let col = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(SPACING_XXS)
                        .css_classes(vec!["multirow-column".to_string()])
                        .build();

                    for item in chunk {
                        let is_explicit = item
                            .badges
                            .iter()
                            .any(|b| b.label == "Explicit" || b.label == "E");
                        let row = MultiRowTrackRow::builder()
                            .launch(MultiRowTrackRowInit {
                                id: item.id.clone(),
                                title: item.title.clone(),
                                artist: item.subtitle.clone(),
                                artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                                entity: item.entity.clone(),
                                is_explicit,
                                is_favorite: item.is_favorite(),
                                in_library: item.in_library(),
                                actions: item.actions.clone(),
                                artwork_service: self.artwork_service.clone(),
                                album_route: item.album_route.clone(),
                                artist_route: item.artist_route.clone(),
                            })
                            .forward(sender.output_sender(), |out| match out {
                                MultiRowTrackRowOutput::Play(r) => FeedOutput::Play(r),
                                MultiRowTrackRowOutput::Action(a) => FeedOutput::Action(a),
                                MultiRowTrackRowOutput::Navigate(r) => FeedOutput::Navigate(r),
                                MultiRowTrackRowOutput::CopyLink(u) => FeedOutput::CopyLink(u),
                            });
                        col.append(row.widget());
                        self.compact_track_controllers.push(row);
                    }
                    shelf_box.append(&col);
                }
                root.append(&scrolled);
            }
            "multi-row-episode-shelf" => {
                let (scrolled, shelf_box) = create_shelf_container();
                let rows_per_col = 3;
                for chunk in items.chunks(rows_per_col) {
                    let col = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(SPACING_XXS)
                        .css_classes(vec!["multirow-column".to_string()])
                        .build();

                    for item in chunk {
                        let row = MultiRowEpisodeRow::builder()
                            .launch(MultiRowEpisodeRowInit {
                                id: item.id.clone(),
                                title: item.title.clone(),
                                subtitle: item.subtitle.clone(),
                                artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                                entity: item.entity.clone(),
                                artwork_service: self.artwork_service.clone(),
                            })
                            .forward(sender.output_sender(), |out| match out {
                                MultiRowEpisodeRowOutput::Play(r) => FeedOutput::Play(r),
                            });
                        col.append(row.widget());
                        self.compact_episode_controllers.push(row);
                    }
                    shelf_box.append(&col);
                }
                root.append(&scrolled);
            }
            "explore-pills" => {
                let (scrolled, shelf_box) = create_shelf_container();
                shelf_box.add_css_class("explore-pills-container");
                for item in items.iter() {
                    let btn = gtk::Button::builder()
                        .label(&item.title)
                        .css_classes(vec!["flat".to_string(), "explore-pill-btn".to_string()])
                        .focus_on_click(false)
                        .build();
                    btn.set_cursor_from_name(Some("pointer"));
                    let s = sender.clone();
                    btn.connect_clicked(move |_| {
                        let _ = s.output(FeedOutput::Navigate(PageRoute::New));
                    });
                    shelf_box.append(&btn);
                }
                root.append(&scrolled);
            }
            _ => {
                // If section consists of songs and has at least 4 items, render as multi-row track shelf
                let all_songs = items
                    .iter()
                    .all(|it| matches!(it.entity, Some(MediaRef::Song(_))));
                if all_songs && items.len() >= 4 {
                    let (scrolled, shelf_box) = create_shelf_container();
                    let rows_per_col = 4;
                    for chunk in items.chunks(rows_per_col) {
                        let col = gtk::Box::builder()
                            .orientation(gtk::Orientation::Vertical)
                            .spacing(SPACING_XXS)
                            .css_classes(vec!["multirow-column".to_string()])
                            .build();

                        for item in chunk {
                            let is_explicit = item
                                .badges
                                .iter()
                                .any(|b| b.label == "Explicit" || b.label == "E");
                            let row = MultiRowTrackRow::builder()
                                .launch(MultiRowTrackRowInit {
                                    id: item.id.clone(),
                                    title: item.title.clone(),
                                    artist: item.subtitle.clone(),
                                    artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                                    entity: item.entity.clone(),
                                    is_explicit,
                                    is_favorite: item.is_favorite(),
                                    in_library: item.in_library(),
                                    actions: item.actions.clone(),
                                    artwork_service: self.artwork_service.clone(),
                                    album_route: item.album_route.clone(),
                                    artist_route: item.artist_route.clone(),
                                })
                                .forward(sender.output_sender(), |out| match out {
                                    MultiRowTrackRowOutput::Play(r) => FeedOutput::Play(r),
                                    MultiRowTrackRowOutput::Action(a) => FeedOutput::Action(a),
                                    MultiRowTrackRowOutput::Navigate(r) => FeedOutput::Navigate(r),
                                    MultiRowTrackRowOutput::CopyLink(u) => FeedOutput::CopyLink(u),
                                });
                            col.append(row.widget());
                            self.compact_track_controllers.push(row);
                        }
                        shelf_box.append(&col);
                    }
                    root.append(&scrolled);
                    return root;
                }
                // Shelf (horizontal scroll)
                let (scrolled, shelf_box) = create_shelf_container();
                let mut shelf_senders = Vec::new();
                let mut vert_senders = Vec::new();
                let is_artist = section.presentation_hint.as_deref() == Some("artist-shelf")
                    || items
                        .first()
                        .and_then(|i| i.open_route.as_ref())
                        .map(|r| matches!(r, PageRoute::Artist(_)))
                        .unwrap_or(false);

                for (idx, item) in items.iter().enumerate() {
                    let card_is_artist = section.presentation_hint.as_deref()
                        == Some("artist-shelf")
                        || item
                            .open_route
                            .as_ref()
                            .map(|r| matches!(r, PageRoute::Artist(_)))
                            .unwrap_or(is_artist);
                    let eager = section_index < 3 && idx < 6;
                    let card = MediaCard::builder()
                        .launch(MediaCardInit {
                            id: item.id.clone(),
                            title: item.title.clone(),
                            subtitle: item.subtitle.clone(),
                            subtitle_route: item.artist_route.clone(),
                            overline: item.tertiary_text.clone(),
                            artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                            entity: item.entity.clone(),
                            open_route: item.open_route.clone(),
                            size: ARTWORK_SHELF_SIZE,
                            is_circular: card_is_artist,
                            artwork_service: self.artwork_service.clone(),
                            eager,
                        })
                        .forward(sender.output_sender(), |out| match out {
                            MediaCardOutput::Navigate(r) => FeedOutput::Navigate(r),
                            MediaCardOutput::Play(r) => FeedOutput::Play(r),
                            MediaCardOutput::CopyLink(u) => FeedOutput::CopyLink(u),
                        });
                    shelf_box.append(card.widget());
                    if !eager {
                        shelf_senders.push((idx, card.sender().clone()));
                        if idx < 6 {
                            vert_senders.push(card.sender().clone());
                        }
                    }
                    self.card_controllers.push(card);
                }

                let senders_ref =
                    hook_shelf_artwork_trigger(&scrolled, ARTWORK_SHELF_SIZE as f64, shelf_senders);

                self.mounted_sections.insert(
                    section.id.clone(),
                    MountedSection::Shelf {
                        shelf_box: shelf_box.clone(),
                        card_size: ARTWORK_SHELF_SIZE,
                        is_artist,
                        shelf_senders: senders_ref,
                    },
                );

                root.append(&scrolled);

                if !vert_senders.is_empty() {
                    self.deferred_sections
                        .borrow_mut()
                        .push((root.clone().upcast(), vert_senders));
                }
            }
        }

        root
    }

    fn build_latest_release_card(
        &self,
        item: &PageItemWire,
        sender: ComponentSender<Self>,
    ) -> gtk::Box {
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_MD)
            .css_classes(vec!["latest-release-card".to_string()])
            .build();
        card.set_cursor_from_name(Some("pointer"));

        let pic = SquareArtwork::new(ARTWORK_LATEST_RELEASE_SIZE, "card-artwork");
        bind_artwork(
            pic.picture(),
            &self.artwork_service,
            item.artwork.as_ref().map(|a| a.url.clone()),
            (ARTWORK_LATEST_RELEASE_SIZE * 2) as u32,
        );
        card.append(&pic);

        let details = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_XXS)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();

        if let Some(ref overline) = item.tertiary_text {
            let overline_lbl = gtk::Label::builder()
                .label(overline)
                .xalign(0.0)
                .css_classes(vec!["card-overline".to_string()])
                .build();
            details.append(&overline_lbl);
        }

        let title_lbl = gtk::Label::builder()
            .label(&item.title)
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .css_classes(vec!["latest-release-title".to_string()])
            .build();
        details.append(&title_lbl);

        if let Some(ref sub) = item.subtitle {
            let sub_lbl = gtk::Label::builder()
                .label(sub)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .css_classes(vec!["latest-release-subtitle".to_string()])
                .build();
            if let Some(ref route) = item.artist_route {
                sub_lbl.add_css_class("metadata-link");
                sub_lbl.set_cursor_from_name(Some("pointer"));
                let s = sender.clone();
                let rt = route.clone();
                let g = gtk::GestureClick::new();
                g.connect_released(move |g, n, _, _| {
                    if n == 1 {
                        g.set_state(gtk::EventSequenceState::Claimed);
                        let _ = s.output(FeedOutput::Navigate(rt.clone()));
                    }
                });
                sub_lbl.add_controller(g);
            }
            details.append(&sub_lbl);
        }

        if !item.metadata.is_empty() {
            let meta_lbl = gtk::Label::builder()
                .label(item.metadata.join(" • "))
                .xalign(0.0)
                .css_classes(vec!["latest-release-metadata".to_string()])
                .build();
            details.append(&meta_lbl);
        }

        card.append(&details);

        if let Some(ref entity) = item.entity {
            let play_btn = gtk::Button::from_icon_name(ICON_PLAY);
            play_btn.set_tooltip_text(Some("Play"));
            play_btn.add_css_class("suggested-action");
            play_btn.add_css_class("circular");
            play_btn.set_focus_on_click(false);
            play_btn.set_valign(gtk::Align::Center);
            play_btn.set_halign(gtk::Align::End);
            let ent = entity.clone();
            let s_play = sender.clone();
            play_btn.connect_clicked(move |_| {
                let _ = s_play.output(FeedOutput::Play(ent.clone()));
            });
            card.append(&play_btn);
        }

        if let Some(ref open_route) = item.open_route {
            let r = open_route.clone();
            let s = sender;
            let gesture = gtk::GestureClick::new();
            gesture.connect_released(move |_, _, _, _| {
                let _ = s.output(FeedOutput::Navigate(r.clone()));
            });
            card.add_controller(gesture);
        }

        card
    }
}

fn page_action_button(label: &str, icon: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_focus_on_click(false);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.set_halign(gtk::Align::Center);
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(label)));
    button.set_child(Some(&content));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}
