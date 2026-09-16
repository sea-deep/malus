use super::*;

impl FeedPage {
    pub(super) fn build_hero_header(
        &mut self,
        header: &PageHeaderWire,
        sender: ComponentSender<Self>,
    ) -> gtk::Box {
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
        if let Some(ref sub) = header.subtitle {
            let sub_label = gtk::Label::builder()
                .label(sub)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .css_classes(vec!["hero-subtitle".to_string()])
                .build();
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
        let last = std::cell::Cell::new(false);
        root.add_tick_callback(move |header, _| {
            let narrow = header.width() < 580;
            if last.replace(narrow) != narrow {
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
                move |command| {
                    if let crate::widgets::actions_menu::ActionMenuCommand::Action(action) = command
                    {
                        let _ = s.output(FeedOutput::Action(action));
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

        match section.presentation_hint.as_deref().unwrap_or("shelf") {
            "track-list" => {
                let track_list = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(2)
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

                for (idx, item) in items.iter().enumerate() {
                    let track = self.item_to_track(item);
                    let coll_clone = collection_ref.clone();
                    let row = TrackRow::builder()
                        .launch(TrackRowInit {
                            track,
                            index: Some(idx),
                            show_artwork: false,
                            is_favorite: item.is_favorite(),
                            in_library: item.in_library(),
                            actions: item.actions.clone(),
                            artwork_service: self.artwork_service.clone(),
                            playlist_context: playlist_ctx.as_ref().map(|p| (p.clone(), idx)),
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
                    let card = MediaCard::builder()
                        .launch(MediaCardInit {
                            id: item.id.clone(),
                            title: item.title.clone(),
                            subtitle: item.subtitle.clone(),
                            overline: item.tertiary_text.clone(),
                            artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
                            entity: item.entity.clone(),
                            open_route: item.open_route.clone(),
                            size: ARTWORK_TOP_PICKS_SIZE,
                            is_circular: false,
                            artwork_service: self.artwork_service.clone(),
                            eager,
                        })
                        .forward(sender.output_sender(), |out| match out {
                            MediaCardOutput::Navigate(r) => FeedOutput::Navigate(r),
                            MediaCardOutput::Play(r) => FeedOutput::Play(r),
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

                let senders_ref = hook_shelf_artwork_trigger(
                    &scrolled,
                    ARTWORK_TOP_PICKS_SIZE as f64,
                    shelf_senders,
                );

                self.mounted_sections.insert(
                    section.id.clone(),
                    MountedSection::Shelf {
                        shelf_box: shelf_box.clone(),
                        card_size: ARTWORK_TOP_PICKS_SIZE,
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
            _ => {
                // Shelf (horizontal scroll)
                let (scrolled, shelf_box) = create_shelf_container();
                let mut shelf_senders = Vec::new();
                let mut vert_senders = Vec::new();
                let is_artist = items
                    .first()
                    .and_then(|i| i.open_route.as_ref())
                    .map(|r| matches!(r, PageRoute::Artist(_)))
                    .unwrap_or(false);

                for (idx, item) in items.iter().enumerate() {
                    let card_is_artist = item
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
}

fn page_action_button(label: &str, icon: &str) -> gtk::Button {
    let button = gtk::Button::new();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.set_halign(gtk::Align::Center);
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(label)));
    button.set_child(Some(&content));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}
