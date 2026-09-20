use super::*;
use crate::widgets::square_artwork::SquareArtwork;

impl FeedPage {
    pub(super) fn create_continuation_spinner() -> gtk::Box {
        let spinner_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk::Align::Center)
            .margin_top(16)
            .margin_bottom(24)
            .build();
        let spinner = gtk::Spinner::new();
        spinner.start();
        let label = gtk::Label::new(Some("Loading more…"));
        label.add_css_class("dim-label");
        spinner_box.append(&spinner);
        spinner_box.append(&label);
        spinner_box
    }

    fn build_artist_row(&self, item: &PageItemWire, sender: ComponentSender<Self>) -> gtk::Box {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(vec!["artist-row".to_string()])
            .build();
        let artist_id = artist_route_id(item);
        let id_str = artist_id.as_deref().unwrap_or("");
        row.set_widget_name(&format!("{id_str}:{}", item.title));
        row.set_cursor_from_name(Some("pointer"));
        let is_selected = self.selected_artist_id == artist_id && artist_id.is_some();
        if is_selected {
            row.add_css_class("artist-row-active");
        }

        let pic = SquareArtwork::new(36, "card-artwork-circular");
        bind_artwork(
            pic.picture(),
            &self.artwork_service,
            item.artwork.as_ref().map(|a| a.url.clone()),
            96,
        );
        row.append(&pic);

        let name_lbl = gtk::Label::builder()
            .label(&item.title)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .css_classes(vec!["card-title".to_string()])
            .build();
        row.append(&name_lbl);

        let item_id = artist_id;
        let gesture = gtk::GestureClick::new();
        gesture.connect_released(move |_, _, _, _| {
            if let Some(id) = &item_id {
                sender.input(FeedInput::SelectArtist(id.clone()));
            }
        });
        row.add_controller(gesture);
        row
    }

    pub(super) fn append_continuation_items(
        &mut self,
        widgets: &mut FeedPageWidgets,
        sender: ComponentSender<Self>,
        cont: &PageContinuationWire,
    ) {
        if self.route == PageRoute::LibraryGenres {
            self.render_content(widgets, sender);
            return;
        }

        if let PageContinuationWire::Section {
            section_id,
            items,
            continuation,
        } = cont
        {
            if self.route == PageRoute::LibraryArtists {
                if let Some(list) = &self.active_artist_list {
                    if let Some(sp) = self.active_artist_spinner.take()
                        && sp.parent().is_some()
                    {
                        list.remove(&sp);
                    }
                    for item in items {
                        list.append(&self.build_artist_row(item, sender.clone()));
                    }
                    if continuation.is_some() {
                        let sp = Self::create_continuation_spinner();
                        list.append(&sp);
                        self.active_artist_spinner = Some(sp);
                    }
                }
                if continuation.is_none() {
                    self.stop_continuation_spinner();
                }
                return;
            }

            if let Some(ref vtl) = self.virtual_track_list {
                if self.current_sort == LibrarySortMethod::RecentlyAdded {
                    vtl.append_items(items);
                } else if let Some(page) = &self.page_data {
                    let mut all_items = Vec::new();
                    for sec in &page.sections {
                        all_items.extend(sec.items.clone());
                    }
                    match self.current_sort {
                        LibrarySortMethod::RecentlyAdded => {}
                        LibrarySortMethod::Title => {
                            all_items.sort_by_key(|a| a.title.to_lowercase());
                        }
                        LibrarySortMethod::Artist => {
                            all_items.sort_by(|a, b| {
                                let a_sub = a.subtitle.as_deref().unwrap_or("").to_lowercase();
                                let b_sub = b.subtitle.as_deref().unwrap_or("").to_lowercase();
                                a_sub.cmp(&b_sub)
                            });
                        }
                    }
                    vtl.set_items(&all_items);
                }
                vtl.set_loading(continuation.is_some());

                if let Some(ref lbl) = self.library_subtitle_lbl
                    && let Some(page) = &self.page_data
                {
                    let total: usize = page.sections.iter().map(|s| s.items.len()).sum();
                    let txt = if continuation.is_some() {
                        format!("{total}+ songs")
                    } else {
                        format!("{total} songs")
                    };
                    lbl.set_text(&txt);
                }
                if continuation.is_none() {
                    self.stop_continuation_spinner();
                }
                return;
            }

            if let Some(mounted) = self.mounted_sections.get(section_id).cloned() {
                match mounted {
                    MountedSection::TrackList {
                        track_list,
                        spinner,
                        playlist_ctx,
                        collection_ref,
                        show_artwork,
                    } => {
                        if let Some(sp) = spinner.borrow_mut().take()
                            && sp.parent().is_some()
                        {
                            track_list.remove(&sp);
                        }

                        let start_idx = self.track_controllers.len();
                        for (i, item) in items.iter().enumerate() {
                            let idx = start_idx + i;
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
                                    playlist_context: playlist_ctx
                                        .as_ref()
                                        .map(|p| (p.clone(), idx)),
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
                                    TrackRowOutput::AddToPlaylist(r) => {
                                        FeedOutput::ShowAddToPlaylist(r)
                                    }
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

                        if continuation.is_some() {
                            let new_sp = Self::create_continuation_spinner();
                            track_list.append(&new_sp);
                            *spinner.borrow_mut() = Some(new_sp);
                        }
                    }
                    MountedSection::Grid {
                        flow,
                        spinner_parent,
                        spinner,
                    } => {
                        if let Some(sp) = spinner.borrow_mut().take()
                            && sp.parent().is_some()
                        {
                            spinner_parent.remove(&sp);
                        }

                        for item in items {
                            let is_artist = item
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
                                    size: ARTWORK_GRID_SIZE,
                                    is_circular: is_artist,
                                    artwork_service: self.artwork_service.clone(),
                                    eager: true,
                                })
                                .forward(sender.output_sender(), |out| match out {
                                    MediaCardOutput::Navigate(r) => FeedOutput::Navigate(r),
                                    MediaCardOutput::Play(r) => FeedOutput::Play(r),
                                    MediaCardOutput::CopyLink(u) => FeedOutput::CopyLink(u),
                                });
                            flow.append(card.widget());
                            self.card_controllers.push(card);
                        }

                        if continuation.is_some() {
                            let new_sp = Self::create_continuation_spinner();
                            spinner_parent.append(&new_sp);
                            *spinner.borrow_mut() = Some(new_sp);
                        }
                    }
                    MountedSection::Shelf {
                        shelf_box,
                        card_size,
                        is_artist: shelf_is_artist,
                        shelf_senders,
                    } => {
                        let current_count = shelf_senders.borrow().len();
                        for (idx_offset, item) in items.iter().enumerate() {
                            let is_art = shelf_is_artist
                                || item
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
                                    size: card_size,
                                    is_circular: is_art,
                                    artwork_service: self.artwork_service.clone(),
                                    eager: false,
                                })
                                .forward(sender.output_sender(), |out| match out {
                                    MediaCardOutput::Navigate(r) => FeedOutput::Navigate(r),
                                    MediaCardOutput::Play(r) => FeedOutput::Play(r),
                                    MediaCardOutput::CopyLink(u) => FeedOutput::CopyLink(u),
                                });
                            shelf_box.append(card.widget());
                            let new_idx = current_count + idx_offset;
                            shelf_senders
                                .borrow_mut()
                                .push((new_idx, card.sender().clone()));
                            self.card_controllers.push(card);
                        }
                    }
                }
                if continuation.is_none() {
                    self.stop_continuation_spinner();
                }
                return;
            }
        }

        if let PageContinuationWire::Sections {
            sections,
            continuation,
        } = cont
        {
            let start_index = self.mounted_sections.len();
            for (offset, section) in sections.iter().enumerate() {
                let sec_widget = self.build_section(section, start_index + offset, sender.clone());
                widgets.sections_container.append(&sec_widget);
            }
            self.trigger_deferred_sections_check(widgets);
            if continuation.is_none() {
                self.stop_continuation_spinner();
            }
        }
    }

    pub(super) fn render_content(
        &mut self,
        widgets: &mut FeedPageWidgets,
        sender: ComponentSender<Self>,
    ) {
        if self.is_loading || self.page_data.is_none() {
            return;
        }

        self.mounted_sections.clear();
        self.deferred_sections.borrow_mut().clear();
        self.active_artist_list = None;
        self.active_artist_spinner = None;
        self.active_artist_split = None;
        *self.active_artist_back_box.borrow_mut() = None;
        self.show_artist_detail_cell = None;
        self.active_genre_split = None;
        self.active_genre_list = None;
        *self.active_genre_back_box.borrow_mut() = None;
        self.show_genre_detail_cell = None;
        self.virtual_track_list = None;
        self.library_subtitle_lbl = None;

        let page = self.page_data.as_ref().unwrap().clone();

        // Proactively prefetch top artwork images in the background
        {
            let mut artwork_urls = Vec::new();
            if let Some(ref header) = page.header
                && let Some(ref art) = header.artwork
            {
                artwork_urls.push((art.url.clone(), crate::services::THUMB_LARGE));
            }
            for section in &page.sections {
                for item in &section.items {
                    if let Some(ref art) = item.artwork {
                        artwork_urls.push((art.url.clone(), crate::services::THUMB_MEDIUM));
                        if artwork_urls.len() >= 24 {
                            break;
                        }
                    }
                }
                if artwork_urls.len() >= 24 {
                    break;
                }
            }
            if !artwork_urls.is_empty() {
                self.artwork_service.prefetch_batch(artwork_urls);
            }
        }

        // Clear existing sections
        self.card_controllers.clear();
        self.track_controllers.clear();
        self.station_card_controllers.clear();
        self.live_station_controllers.clear();
        self.featured_banner_controllers.clear();
        self.compact_track_controllers.clear();
        self.compact_episode_controllers.clear();
        while let Some(child) = widgets.sections_container.first_child() {
            widgets.sections_container.remove(&child);
        }

        // If LibraryArtists, render master-detail split view
        if self.route == PageRoute::LibraryArtists {
            widgets
                .scrolled_window
                .set_vscrollbar_policy(gtk::PolicyType::Never);
            widgets.main_box.set_vexpand(true);
            widgets.main_box.set_margin_start(0);
            widgets.main_box.set_margin_end(0);
            widgets.main_box.set_margin_top(0);
            widgets.main_box.set_margin_bottom(0);
            widgets.sections_container.set_vexpand(true);
            let artists_view = self.build_library_artists_view(&page, sender.clone());
            widgets.sections_container.append(&artists_view);
            return;
        } else if self.route == PageRoute::LibrarySongs {
            widgets
                .scrolled_window
                .set_vscrollbar_policy(gtk::PolicyType::Never);
            widgets.main_box.set_vexpand(true);
            widgets.main_box.set_margin_start(PAGE_PADDING_NORMAL);
            widgets.main_box.set_margin_end(PAGE_PADDING_NORMAL);
            widgets.main_box.set_margin_top(24);
            widgets.main_box.set_margin_bottom(0);
            widgets.sections_container.set_vexpand(true);
            let songs_view = self.build_library_songs_view(&page, sender.clone());
            widgets.sections_container.append(&songs_view);
            return;
        } else if self.route == PageRoute::LibraryGenres {
            widgets
                .scrolled_window
                .set_vscrollbar_policy(gtk::PolicyType::Never);
            widgets.main_box.set_vexpand(true);
            widgets.main_box.set_margin_start(0);
            widgets.main_box.set_margin_end(0);
            widgets.main_box.set_margin_top(0);
            widgets.main_box.set_margin_bottom(0);
            widgets.sections_container.set_vexpand(true);
            let genres_view = self.build_library_genres_view(&page, sender.clone());
            widgets.sections_container.append(&genres_view);
            return;
        } else {
            widgets
                .scrolled_window
                .set_vscrollbar_policy(gtk::PolicyType::Automatic);
            widgets.main_box.set_vexpand(false);
            widgets.main_box.set_margin_start(PAGE_PADDING_NORMAL);
            widgets.main_box.set_margin_end(PAGE_PADDING_NORMAL);
            widgets.main_box.set_margin_top(24);
            widgets.main_box.set_margin_bottom(48);
            widgets.sections_container.set_vexpand(false);
        }

        // 1. Page Header / Hero Banner
        if let Some(ref header) = page.header {
            let hero = self.build_hero_header(header, sender.clone());
            widgets.sections_container.append(&hero);
        } else {
            // Standard Page Title
            let title_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .build();

            let title_label = gtk::Label::builder()
                .label(&page.title)
                .wrap(true)
                .xalign(0.0)
                .css_classes(vec!["page-title".to_string()])
                .build();
            title_box.append(&title_label);

            if let Some(ref sub) = page.subtitle {
                let sub_label = gtk::Label::builder()
                    .label(sub)
                    .wrap(true)
                    .xalign(0.0)
                    .css_classes(vec!["page-subtitle".to_string()])
                    .build();
                title_box.append(&sub_label);
            }

            let is_library_page = matches!(
                self.route,
                PageRoute::LibrarySongs
                    | PageRoute::LibraryAlbums
                    | PageRoute::LibraryPlaylists
                    | PageRoute::LibraryRecentlyAdded
            );

            if is_library_page {
                let header_row = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .valign(gtk::Align::Center)
                    .hexpand(true)
                    .margin_bottom(16)
                    .build();

                title_box.set_hexpand(true);
                header_row.append(&title_box);

                let sort_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(6)
                    .valign(gtk::Align::Center)
                    .halign(gtk::Align::End)
                    .build();

                let sort_lbl = gtk::Label::builder()
                    .label("Sort:")
                    .css_classes(vec!["dim-label".to_string()])
                    .build();
                sort_box.append(&sort_lbl);

                let dropdown = gtk::DropDown::from_strings(&["Recently Added", "Title", "Artist"]);
                dropdown.add_css_class("flat");
                dropdown.add_css_class("sort-dropdown");

                let initial_index = match self.current_sort {
                    LibrarySortMethod::RecentlyAdded => 0,
                    LibrarySortMethod::Title => 1,
                    LibrarySortMethod::Artist => 2,
                };
                dropdown.set_selected(initial_index);

                let s = sender.clone();
                dropdown.connect_selected_notify(move |dd| {
                    let method = match dd.selected() {
                        0 => LibrarySortMethod::RecentlyAdded,
                        1 => LibrarySortMethod::Title,
                        2 => LibrarySortMethod::Artist,
                        _ => LibrarySortMethod::RecentlyAdded,
                    };
                    s.input(FeedInput::ChangeSort(method));
                });

                if matches!(self.route, PageRoute::LibraryPlaylists) {
                    let btn_new_pl = gtk::Button::builder()
                        .label("New Playlist")
                        .icon_name("list-add-symbolic")
                        .css_classes(vec!["suggested-action".to_string()])
                        .valign(gtk::Align::Center)
                        .margin_end(8)
                        .build();
                    let s_new = sender.clone();
                    btn_new_pl.connect_clicked(move |_| {
                        let _ = s_new.output(FeedOutput::ShowNewPlaylistDialog {
                            initial_track: None,
                        });
                    });
                    sort_box.prepend(&btn_new_pl);
                }

                sort_box.append(&dropdown);
                header_row.append(&sort_box);
                widgets.sections_container.append(&header_row);
            } else {
                title_box.set_margin_bottom(12);
                widgets.sections_container.append(&title_box);
            }
        }

        // 2. Sections
        for (section_index, section) in page.sections.iter().enumerate() {
            let sec_widget = self.build_section(section, section_index, sender.clone());
            widgets.sections_container.append(&sec_widget);
        }

        // 3. Biography / Editorial Description (appended at bottom of page)
        if let Some(ref header) = page.header
            && let Some(ref bio) = header.description
        {
            let trimmed = bio.trim();
            if !trimmed.is_empty() {
                let bio_widget = self.build_biography_section(trimmed);
                widgets.sections_container.append(&bio_widget);
            }
        }

        self.trigger_deferred_sections_check(widgets);
    }

    fn build_biography_section(&self, bio_text: &str) -> gtk::Box {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_XS)
            .margin_top(SPACING_LG)
            .margin_bottom(SPACING_XL)
            .css_classes(vec!["biography-section".to_string()])
            .build();

        let title_text = match self.route {
            PageRoute::Artist(_) => "About",
            PageRoute::Album(_) => "Editorial Notes",
            PageRoute::Playlist(_) => "About this Playlist",
            _ => "About",
        };

        let title_lbl = gtk::Label::builder()
            .label(title_text)
            .xalign(0.0)
            .css_classes(vec!["section-title".to_string()])
            .build();
        root.append(&title_lbl);

        let is_long = bio_text.chars().count() > 280;

        let label = gtk::Label::builder()
            .label(bio_text)
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .css_classes(vec!["biography-text".to_string()])
            .build();

        if is_long {
            label.set_lines(4);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        }
        root.append(&label);

        if is_long {
            let toggle_btn = gtk::Button::builder()
                .label("Read More")
                .halign(gtk::Align::Start)
                .css_classes(vec!["flat".to_string(), "dim-label".to_string()])
                .build();

            let expanded = std::cell::Cell::new(false);
            let lbl = label;
            toggle_btn.connect_clicked(move |btn| {
                let is_exp = !expanded.get();
                expanded.set(is_exp);
                if is_exp {
                    lbl.set_lines(0);
                    lbl.set_ellipsize(gtk::pango::EllipsizeMode::None);
                    btn.set_label("Show Less");
                } else {
                    lbl.set_lines(4);
                    lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    btn.set_label("Read More");
                }
            });
            root.append(&toggle_btn);
        }

        root
    }

    pub(super) fn trigger_deferred_sections_check(&self, widgets: &FeedPageWidgets) {
        let vadj = widgets.scrolled_window.vadjustment();
        let container = widgets.sections_container.clone();
        let deferred = self.deferred_sections.clone();
        glib::idle_add_local_once(move || {
            let mut d = deferred.borrow_mut();
            if d.is_empty() {
                return;
            }
            let max_visible_y = vadj.value() + vadj.page_size().max(800.0) + 500.0;
            d.retain(|(sec_widget, senders)| {
                if let Some(bounds) = sec_widget.compute_bounds(&container)
                    && (bounds.y() as f64) <= max_visible_y
                {
                    for s in senders {
                        let _ = s.send(MediaCardInput::LoadArtwork);
                    }
                    return false;
                }
                true
            });
        });
    }
}

impl FeedPage {
    fn build_library_songs_view(
        &mut self,
        page: &PageWire,
        sender: ComponentSender<Self>,
    ) -> gtk::Box {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .vexpand(true)
            .hexpand(true)
            .build();

        // 1. Header Row (Title, Subtitle, Sort Dropdown)
        let header_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .margin_bottom(8)
            .build();

        let title_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .build();

        let title_lbl = gtk::Label::builder()
            .label(&page.title)
            .xalign(0.0)
            .css_classes(vec!["page-title".to_string()])
            .build();
        title_box.append(&title_lbl);

        // Gather all songs from sections
        let mut items = Vec::new();
        for sec in &page.sections {
            items.extend(sec.items.clone());
        }

        let has_continuation =
            page.sections.iter().any(|s| s.continuation.is_some()) || page.continuation.is_some();

        let count_str = if has_continuation {
            format!("{}+ songs", items.len())
        } else {
            format!("{} songs", items.len())
        };

        let sub_lbl = gtk::Label::builder()
            .label(&count_str)
            .xalign(0.0)
            .css_classes(vec!["page-subtitle".to_string()])
            .build();
        title_box.append(&sub_lbl);
        self.library_subtitle_lbl = Some(sub_lbl.clone());

        header_row.append(&title_box);

        // Sort dropdown
        let sort_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .valign(gtk::Align::Center)
            .halign(gtk::Align::End)
            .build();

        let sort_lbl = gtk::Label::builder()
            .label("Sort:")
            .css_classes(vec!["dim-label".to_string()])
            .build();
        sort_box.append(&sort_lbl);

        let dropdown = gtk::DropDown::from_strings(&["Recently Added", "Title", "Artist"]);
        dropdown.add_css_class("flat");
        dropdown.add_css_class("sort-dropdown");

        let initial_index = match self.current_sort {
            LibrarySortMethod::RecentlyAdded => 0,
            LibrarySortMethod::Title => 1,
            LibrarySortMethod::Artist => 2,
        };
        dropdown.set_selected(initial_index);

        let s = sender.clone();
        dropdown.connect_selected_notify(move |dd| {
            let method = match dd.selected() {
                0 => LibrarySortMethod::RecentlyAdded,
                1 => LibrarySortMethod::Title,
                2 => LibrarySortMethod::Artist,
                _ => LibrarySortMethod::RecentlyAdded,
            };
            s.input(FeedInput::ChangeSort(method));
        });

        sort_box.append(&dropdown);
        header_row.append(&sort_box);
        root.append(&header_row);

        // Sort items according to self.current_sort
        match self.current_sort {
            LibrarySortMethod::RecentlyAdded => {}
            LibrarySortMethod::Title => {
                items.sort_by_key(|a| a.title.to_lowercase());
            }
            LibrarySortMethod::Artist => {
                items.sort_by(|a, b| {
                    let a_sub = a.subtitle.as_deref().unwrap_or("").to_lowercase();
                    let b_sub = b.subtitle.as_deref().unwrap_or("").to_lowercase();
                    a_sub.cmp(&b_sub)
                });
            }
        }

        // 2. Virtualized Track List
        let vtl = VirtualTrackList::new(sender);
        vtl.set_items(&items);
        vtl.set_loading(has_continuation);

        root.append(vtl.widget());
        self.virtual_track_list = Some(vtl);

        root
    }

    fn build_library_artists_view(
        &mut self,
        page: &PageWire,
        sender: ComponentSender<Self>,
    ) -> gtk::Widget {
        let split = adw::OverlaySplitView::builder()
            .sidebar_position(gtk::PackType::Start)
            .min_sidebar_width(220.0)
            .max_sidebar_width(280.0)
            .sidebar_width_fraction(0.24)
            .enable_show_gesture(true)
            .enable_hide_gesture(true)
            .css_classes(vec!["master-split-view".to_string()])
            .vexpand(true)
            .hexpand(true)
            .build();

        // 1. Master List Sidebar
        let sidebar_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .css_classes(vec!["artist-master-list".to_string()])
            .vexpand(true)
            .build();

        let title_lbl = gtk::Label::builder()
            .label("Artists")
            .xalign(0.0)
            .css_classes(vec!["page-title".to_string()])
            .margin_start(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        sidebar_box.append(&title_lbl);

        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text("Find in Artists")
            .margin_start(8)
            .margin_end(8)
            .margin_bottom(4)
            .text(&self.artist_search_query)
            .build();
        sidebar_box.append(&search_entry);

        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .build();

        let list_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .margin_start(8)
            .margin_end(8)
            .build();

        let s_search = sender.clone();
        let list_box_filter = list_box.clone();
        search_entry.connect_search_changed(move |entry| {
            let query = entry.text().trim().to_lowercase();
            s_search.input(FeedInput::ArtistSearchChanged(query.clone()));
            let mut child = list_box_filter.first_child();
            while let Some(widget) = child {
                let matches = if query.is_empty() {
                    true
                } else {
                    let name = widget.widget_name().to_lowercase();
                    name != "gtkbox" && name.contains(&query)
                };
                widget.set_visible(matches);
                child = widget.next_sibling();
            }
        });

        let artist_items: Vec<_> = page
            .sections
            .iter()
            .flat_map(|s| &s.items)
            .cloned()
            .collect();

        for item in &artist_items {
            let row = self.build_artist_row(item, sender.clone());
            if !self.artist_search_query.is_empty() {
                let q = self.artist_search_query.to_lowercase();
                if !item.title.to_lowercase().contains(&q) {
                    row.set_visible(false);
                }
            }
            list_box.append(&row);
        }

        if page.sections.iter().any(|s| s.continuation.is_some()) {
            let spinner = Self::create_continuation_spinner();
            list_box.append(&spinner);
            self.active_artist_spinner = Some(spinner);
        }

        self.active_artist_list = Some(list_box.clone());

        scroll.set_child(Some(&list_box));
        sidebar_box.append(&scroll);
        split.set_sidebar(Some(&sidebar_box));

        let (detail_scroll, back_box_opt) = self.build_artist_detail_content(sender);
        *self.active_artist_back_box.borrow_mut() = back_box_opt;
        split.set_content(Some(&detail_scroll));

        let show_cell = std::rc::Rc::new(std::cell::Cell::new(self.show_artist_detail));
        self.show_artist_detail_cell = Some(show_cell.clone());
        self.active_artist_split = Some(split.clone());

        // Responsive behavior
        let split_ref = split.clone();
        let last_w = std::cell::Cell::new(0);
        let back_box_cell = self.active_artist_back_box.clone();
        let show_cell_ref = show_cell;
        split.add_tick_callback(move |widget, _| {
            let win_w = widget
                .root()
                .and_then(|r| r.downcast::<gtk::Window>().ok())
                .map(|w| w.width())
                .unwrap_or(0);
            if win_w > 0 && last_w.replace(win_w) != win_w {
                let is_narrow = win_w < 850;
                let show_detail = show_cell_ref.get();
                split_ref.set_collapsed(is_narrow);
                if is_narrow {
                    split_ref.set_show_sidebar(!show_detail);
                    if let Some(b) = back_box_cell.borrow().as_ref() {
                        b.set_visible(show_detail);
                    }
                } else {
                    split_ref.set_show_sidebar(true);
                    if let Some(b) = back_box_cell.borrow().as_ref() {
                        b.set_visible(false);
                    }
                }
            }
            relm4::gtk::glib::ControlFlow::Continue
        });

        split.upcast()
    }

    pub(super) fn build_artist_detail_content(
        &mut self,
        sender: ComponentSender<Self>,
    ) -> (gtk::ScrolledWindow, Option<gtk::Box>) {
        let detail_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .build();

        let detail_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .margin_start(24)
            .margin_end(24)
            .margin_top(16)
            .margin_bottom(24)
            .vexpand(true)
            .hexpand(true)
            .build();

        let mut back_box_opt = None;

        if let Some(detail) = self.artist_detail.clone() {
            // Narrow window back button (hidden by default on desktop)
            let back_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(6)
                .margin_bottom(8)
                .visible(false)
                .build();

            let back_btn = gtk::Button::builder()
                .icon_name(ICON_BACK)
                .label("Artists")
                .css_classes(vec!["flat".to_string()])
                .build();
            let s = sender.clone();
            back_btn.connect_clicked(move |_| {
                s.input(FeedInput::BackToArtistList);
            });
            back_box.append(&back_btn);
            detail_box.append(&back_box);
            back_box_opt = Some(back_box);

            // Header
            if let Some(ref header) = detail.header {
                let header_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(10)
                    .margin_top(4)
                    .margin_bottom(12)
                    .build();

                let title_lbl = gtk::Label::builder()
                    .label(&header.title)
                    .xalign(0.0)
                    .wrap(true)
                    .wrap_mode(gtk::pango::WrapMode::WordChar)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .lines(2)
                    .css_classes(vec!["page-title".to_string()])
                    .build();

                let cat_id = header
                    .metadata
                    .iter()
                    .find_map(|m| m.strip_prefix("catalog_id:").map(|s| s.to_string()));

                if let Some(cid) = cat_id {
                    let title_btn = gtk::Button::builder()
                        .css_classes(vec!["flat".to_string(), "artist-title-btn".to_string()])
                        .halign(gtk::Align::Start)
                        .build();
                    title_btn.set_cursor_from_name(Some("pointer"));

                    let title_inner = gtk::Box::builder()
                        .orientation(gtk::Orientation::Horizontal)
                        .spacing(8)
                        .valign(gtk::Align::Center)
                        .build();

                    title_inner.append(&title_lbl);

                    let chevron = gtk::Image::builder()
                        .icon_name("go-next-symbolic")
                        .valign(gtk::Align::Center)
                        .css_classes(vec!["dim-label".to_string()])
                        .build();
                    title_inner.append(&chevron);

                    title_btn.set_child(Some(&title_inner));
                    let s = sender.clone();
                    title_btn.connect_clicked(move |_| {
                        let _ = s.output(FeedOutput::Navigate(PageRoute::Artist(cid.clone())));
                    });
                    header_box.append(&title_btn);
                } else {
                    header_box.append(&title_lbl);
                }

                // Action buttons beneath title
                let actions = self.build_header_actions(header, sender.clone());
                actions.set_margin_top(4);
                actions.set_halign(gtk::Align::Start);
                header_box.append(&actions);

                detail_box.append(&header_box);
            }

            // Albums Grid / Sections
            for (sec_index, sec) in detail.sections.iter().enumerate() {
                let sec_widget = self.build_section(sec, sec_index, sender.clone());
                detail_box.append(&sec_widget);
            }
        } else if self.artist_detail_loading {
            let spinner = gtk::Spinner::builder()
                .spinning(true)
                .width_request(32)
                .height_request(32)
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .vexpand(true)
                .build();
            detail_box.append(&spinner);
        } else if let Some(error) = &self.artist_error {
            let status = crate::widgets::empty_state::create_empty_state(
                "network-error-symbolic",
                "Couldn’t load this artist",
                Some(error),
            );
            detail_box.append(&status);
        } else {
            let empty_lbl = gtk::Label::builder()
                .label("Select an artist to view library albums")
                .css_classes(vec!["dim-label".to_string()])
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .vexpand(true)
                .build();
            detail_box.append(&empty_lbl);
        }

        detail_scroll.set_child(Some(&detail_box));
        (detail_scroll, back_box_opt)
    }

    pub(super) fn update_artist_view_inplace(
        &mut self,
        split: &adw::OverlaySplitView,
        sender: ComponentSender<Self>,
    ) {
        if let Some(list) = &self.active_artist_list {
            let mut child = list.first_child();
            while let Some(w) = child {
                let name = w.widget_name();
                let row_id = name.split(':').next().unwrap_or("");
                let is_active = self.selected_artist_id.as_deref() == Some(row_id)
                    && self.selected_artist_id.is_some();
                if is_active {
                    w.add_css_class("artist-row-active");
                } else {
                    w.remove_css_class("artist-row-active");
                }
                child = w.next_sibling();
            }
        }

        let (detail_scroll, back_box) = self.build_artist_detail_content(sender);
        *self.active_artist_back_box.borrow_mut() = back_box.clone();
        split.set_content(Some(&detail_scroll));

        let win_narrow = split
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok())
            .map(|w| w.width() > 0 && w.width() < 850)
            .unwrap_or(false);
        let is_narrow = split.is_collapsed() || win_narrow;
        if is_narrow {
            split.set_collapsed(true);
            split.set_show_sidebar(!self.show_artist_detail);
            if let Some(b) = &back_box {
                b.set_visible(self.show_artist_detail);
            }
        } else {
            split.set_show_sidebar(true);
            if let Some(b) = &back_box {
                b.set_visible(false);
            }
        }
    }

    fn build_genre_row(
        &self,
        genre_name: Option<&str>,
        display_name: &str,
        count: usize,
        sender: ComponentSender<Self>,
    ) -> gtk::Box {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["genre-row".to_string()])
            .build();
        row.set_widget_name(display_name);
        row.set_cursor_from_name(Some("pointer"));

        let is_selected = match (&self.selected_genre, genre_name) {
            (None, None) => true,
            (Some(s), Some(g)) => s.eq_ignore_ascii_case(g),
            _ => false,
        };
        if is_selected {
            row.add_css_class("genre-row-active");
        }

        let name_lbl = gtk::Label::builder()
            .label(display_name)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .css_classes(vec!["card-title".to_string()])
            .build();
        row.append(&name_lbl);

        let count_badge = gtk::Label::builder()
            .label(count.to_string())
            .valign(gtk::Align::Center)
            .css_classes(vec!["genre-count-badge".to_string()])
            .build();
        row.append(&count_badge);

        let genre_opt = genre_name.map(str::to_string);
        let gesture = gtk::GestureClick::new();
        gesture.connect_released(move |_, _, _, _| {
            sender.input(FeedInput::SelectGenre(genre_opt.clone()));
        });
        row.add_controller(gesture);

        row
    }

    fn build_library_genres_view(
        &mut self,
        page: &PageWire,
        sender: ComponentSender<Self>,
    ) -> gtk::Widget {
        let split = adw::OverlaySplitView::builder()
            .sidebar_position(gtk::PackType::Start)
            .min_sidebar_width(220.0)
            .max_sidebar_width(280.0)
            .sidebar_width_fraction(0.24)
            .enable_show_gesture(true)
            .enable_hide_gesture(true)
            .css_classes(vec!["master-split-view".to_string()])
            .vexpand(true)
            .hexpand(true)
            .build();

        // 1. Master List Sidebar
        let sidebar_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .css_classes(vec![
                "artist-master-list".to_string(),
                "genre-master-list".to_string(),
            ])
            .vexpand(true)
            .build();

        let title_lbl = gtk::Label::builder()
            .label("Genres")
            .xalign(0.0)
            .css_classes(vec!["page-title".to_string()])
            .margin_start(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        sidebar_box.append(&title_lbl);

        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text("Find in Genres")
            .margin_start(8)
            .margin_end(8)
            .margin_bottom(4)
            .text(&self.genre_search_query)
            .build();
        sidebar_box.append(&search_entry);

        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .build();

        let list_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .margin_start(8)
            .margin_end(8)
            .build();

        let s_search = sender.clone();
        let list_box_filter = list_box.clone();
        search_entry.connect_search_changed(move |entry| {
            let query = entry.text().trim().to_lowercase();
            s_search.input(FeedInput::GenreSearchChanged(query.clone()));
            let mut child = list_box_filter.first_child();
            while let Some(widget) = child {
                let matches = if query.is_empty() {
                    true
                } else {
                    let name = widget.widget_name().to_lowercase();
                    name != "gtkbox" && name.contains(&query)
                };
                widget.set_visible(matches);
                child = widget.next_sibling();
            }
        });

        let all_items: Vec<_> = page
            .sections
            .iter()
            .flat_map(|s| &s.items)
            .cloned()
            .collect();

        let mut genre_counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut uncategorized_count = 0;

        for item in &all_items {
            if item.genres.is_empty() {
                uncategorized_count += 1;
            } else {
                for genre in &item.genres {
                    *genre_counts.entry(genre.clone()).or_insert(0) += 1;
                }
            }
        }

        if uncategorized_count > 0 && !genre_counts.contains_key("Other") {
            genre_counts.insert("Other".to_string(), uncategorized_count);
        }

        // All Genres row
        let all_row = self.build_genre_row(None, "All Genres", all_items.len(), sender.clone());
        if !self.genre_search_query.is_empty() {
            let q = self.genre_search_query.to_lowercase();
            if !"all genres".contains(&q) {
                all_row.set_visible(false);
            }
        }
        list_box.append(&all_row);

        // Individual genre rows
        for (genre, count) in &genre_counts {
            let row = self.build_genre_row(Some(genre), genre, *count, sender.clone());
            if !self.genre_search_query.is_empty() {
                let q = self.genre_search_query.to_lowercase();
                if !genre.to_lowercase().contains(&q) {
                    row.set_visible(false);
                }
            }
            list_box.append(&row);
        }

        self.active_genre_list = Some(list_box.clone());

        scroll.set_child(Some(&list_box));
        sidebar_box.append(&scroll);
        split.set_sidebar(Some(&sidebar_box));

        let (detail_scroll, back_box_opt) = self.build_genre_detail_content(page, sender);
        *self.active_genre_back_box.borrow_mut() = back_box_opt;
        split.set_content(Some(&detail_scroll));

        let show_cell = std::rc::Rc::new(std::cell::Cell::new(self.show_genre_detail));
        self.show_genre_detail_cell = Some(show_cell.clone());
        self.active_genre_split = Some(split.clone());

        // Responsive behavior
        let split_ref = split.clone();
        let last_w = std::cell::Cell::new(0);
        let back_box_cell = self.active_genre_back_box.clone();
        let show_cell_ref = show_cell;
        split.add_tick_callback(move |widget, _| {
            let win_w = widget
                .root()
                .and_then(|r| r.downcast::<gtk::Window>().ok())
                .map(|w| w.width())
                .unwrap_or(0);
            if win_w > 0 && last_w.replace(win_w) != win_w {
                let is_narrow = win_w < 850;
                let show_detail = show_cell_ref.get();
                split_ref.set_collapsed(is_narrow);
                if is_narrow {
                    split_ref.set_show_sidebar(!show_detail);
                    if let Some(b) = back_box_cell.borrow().as_ref() {
                        b.set_visible(show_detail);
                    }
                } else {
                    split_ref.set_show_sidebar(true);
                    if let Some(b) = back_box_cell.borrow().as_ref() {
                        b.set_visible(false);
                    }
                }
            }
            relm4::gtk::glib::ControlFlow::Continue
        });

        split.upcast()
    }

    pub(super) fn build_genre_detail_content(
        &mut self,
        page: &PageWire,
        sender: ComponentSender<Self>,
    ) -> (gtk::ScrolledWindow, Option<gtk::Box>) {
        let all_items: Vec<_> = page
            .sections
            .iter()
            .flat_map(|s| &s.items)
            .cloned()
            .collect();

        // 2. Detail Content
        let detail_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .build();

        let detail_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .margin_start(24)
            .margin_end(24)
            .margin_top(16)
            .margin_bottom(24)
            .vexpand(true)
            .hexpand(true)
            .build();

        // Narrow window back button
        let back_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_bottom(8)
            .visible(false)
            .build();

        let back_btn = gtk::Button::builder()
            .icon_name(ICON_BACK)
            .label("Genres")
            .css_classes(vec!["flat".to_string()])
            .focus_on_click(false)
            .build();
        let s_back = sender.clone();
        back_btn.connect_clicked(move |_| {
            s_back.input(FeedInput::BackToGenreList);
        });
        back_box.append(&back_btn);
        detail_box.append(&back_box);
        let back_box_opt = Some(back_box);

        // Filter items
        let mut filtered_items: Vec<_> = all_items
            .into_iter()
            .filter(|item| match &self.selected_genre {
                None => true,
                Some(g) if g.eq_ignore_ascii_case("Other") => {
                    item.genres.is_empty()
                        || item
                            .genres
                            .iter()
                            .any(|ig| ig.eq_ignore_ascii_case("Other"))
                }
                Some(g) => item.genres.iter().any(|ig| ig.eq_ignore_ascii_case(g)),
            })
            .collect();

        // Sort items
        match self.current_sort {
            LibrarySortMethod::RecentlyAdded => {}
            LibrarySortMethod::Title => {
                filtered_items.sort_by_key(|a| a.title.to_lowercase());
            }
            LibrarySortMethod::Artist => {
                filtered_items.sort_by(|a, b| {
                    let a_sub = a.subtitle.as_deref().unwrap_or("").to_lowercase();
                    let b_sub = b.subtitle.as_deref().unwrap_or("").to_lowercase();
                    a_sub
                        .cmp(&b_sub)
                        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                });
            }
        }

        // Header
        let header_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .margin_bottom(12)
            .build();

        let title_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .build();

        let display_title = self.selected_genre.as_deref().unwrap_or("All Genres");
        let title_lbl = gtk::Label::builder()
            .label(display_title)
            .xalign(0.0)
            .css_classes(vec!["page-title".to_string()])
            .build();
        title_box.append(&title_lbl);

        let has_continuation =
            page.sections.iter().any(|s| s.continuation.is_some()) || page.continuation.is_some();
        let count_suffix = if filtered_items.len() == 1 {
            "album"
        } else {
            "albums"
        };
        let count_str = if has_continuation && self.selected_genre.is_none() {
            format!("{}+ {}", filtered_items.len(), count_suffix)
        } else {
            format!("{} {}", filtered_items.len(), count_suffix)
        };

        let sub_lbl = gtk::Label::builder()
            .label(&count_str)
            .xalign(0.0)
            .css_classes(vec!["page-subtitle".to_string()])
            .build();
        title_box.append(&sub_lbl);
        header_row.append(&title_box);

        // Sort dropdown
        let sort_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .valign(gtk::Align::Center)
            .build();

        let sort_lbl = gtk::Label::builder()
            .label("Sort by")
            .css_classes(vec!["dim-label".to_string()])
            .build();
        sort_box.append(&sort_lbl);

        let dropdown = gtk::DropDown::from_strings(&["Recently Added", "Title", "Artist"]);
        dropdown.add_css_class("flat");
        dropdown.add_css_class("sort-dropdown");
        dropdown.set_focus_on_click(false);

        let initial_index = match self.current_sort {
            LibrarySortMethod::RecentlyAdded => 0,
            LibrarySortMethod::Title => 1,
            LibrarySortMethod::Artist => 2,
        };
        dropdown.set_selected(initial_index);

        let s_sort = sender.clone();
        dropdown.connect_selected_notify(move |dd| {
            let method = match dd.selected() {
                0 => LibrarySortMethod::RecentlyAdded,
                1 => LibrarySortMethod::Title,
                2 => LibrarySortMethod::Artist,
                _ => return,
            };
            s_sort.input(FeedInput::ChangeSort(method));
        });
        sort_box.append(&dropdown);
        header_row.append(&sort_box);
        detail_box.append(&header_row);

        // Grid of albums
        if filtered_items.is_empty() {
            let empty = crate::widgets::empty_state::create_empty_state(
                "media-optical-symbolic",
                "No albums in this genre",
                None,
            );
            detail_box.append(&empty);
        } else {
            let flow = gtk::FlowBox::builder()
                .valign(gtk::Align::Start)
                .max_children_per_line(30)
                .min_children_per_line(2)
                .selection_mode(gtk::SelectionMode::None)
                .column_spacing(16)
                .row_spacing(16)
                .build();

            let mut grid_deferred = Vec::new();
            for (idx, item) in filtered_items.iter().enumerate() {
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
                        is_circular: false,
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
            detail_box.append(&flow);

            if !grid_deferred.is_empty() {
                self.deferred_sections
                    .borrow_mut()
                    .push((flow.upcast(), grid_deferred));
            }
        }

        if has_continuation {
            let spinner = Self::create_continuation_spinner();
            detail_box.append(&spinner);
        }

        detail_scroll.set_child(Some(&detail_box));

        // Hook lazy artwork loading for detail scroll
        let vadj = detail_scroll.vadjustment();
        let deferred_clone = self.deferred_sections.clone();
        let detail_box_clone = detail_box;
        let check_scroll = move |adj: &gtk::Adjustment| {
            let mut deferred = deferred_clone.borrow_mut();
            if deferred.is_empty() {
                return;
            }
            let max_visible_y = adj.value() + adj.page_size().max(800.0) + 500.0;
            deferred.retain(|(sec_widget, senders)| {
                if let Some(bounds) = sec_widget.compute_bounds(&detail_box_clone)
                    && (bounds.y() as f64) <= max_visible_y
                {
                    for s in senders {
                        let _ = s.send(MediaCardInput::LoadArtwork);
                    }
                    return false;
                }
                true
            });
        };
        let c1 = check_scroll.clone();
        vadj.connect_value_changed(move |adj| c1(adj));
        let c2 = check_scroll.clone();
        vadj.connect_page_size_notify(move |adj| c2(adj));
        let vadj_c = vadj;
        detail_scroll.connect_map(move |_| check_scroll(&vadj_c));

        (detail_scroll, back_box_opt)
    }

    pub(super) fn update_genre_view_inplace(
        &mut self,
        split: &adw::OverlaySplitView,
        sender: ComponentSender<Self>,
    ) {
        let page = match self.page_data.as_ref() {
            Some(p) => p.clone(),
            None => return,
        };

        if let Some(list) = &self.active_genre_list {
            let mut child = list.first_child();
            while let Some(w) = child {
                let name = w.widget_name();
                let is_active = match (&self.selected_genre, name.as_str()) {
                    (None, "All Genres") => true,
                    (Some(g), n) => g.eq_ignore_ascii_case(n),
                    _ => false,
                };
                if is_active {
                    w.add_css_class("genre-row-active");
                } else {
                    w.remove_css_class("genre-row-active");
                }
                child = w.next_sibling();
            }
        }

        let (detail_scroll, back_box) = self.build_genre_detail_content(&page, sender);
        *self.active_genre_back_box.borrow_mut() = back_box.clone();
        split.set_content(Some(&detail_scroll));

        let win_narrow = split
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok())
            .map(|w| w.width() > 0 && w.width() < 850)
            .unwrap_or(false);
        let is_narrow = split.is_collapsed() || win_narrow;
        if is_narrow {
            split.set_collapsed(true);
            split.set_show_sidebar(!self.show_genre_detail);
            if let Some(b) = &back_box {
                b.set_visible(self.show_genre_detail);
            }
        } else {
            split.set_show_sidebar(true);
            if let Some(b) = &back_box {
                b.set_visible(false);
            }
        }
    }

    pub(super) fn item_to_track(&self, item: &PageItemWire) -> Track {
        let media_ref = item.entity.clone().unwrap_or_else(|| {
            MediaRef::parse(&item.id).unwrap_or_else(|_| MediaRef::Song(item.id.clone()))
        });
        let artist_name = item.subtitle.as_deref().unwrap_or("");
        let mut track = Track::new(media_ref, &item.title, artist_name);
        if let Some(ref art) = item.artwork {
            track = track.with_artwork(art.clone());
        }
        let is_explicit = item
            .badges
            .iter()
            .any(|b| b.label == "E" || b.label == "explicit");
        track = track.with_explicit(is_explicit);
        if let Some(dur) = item.duration_ms {
            track = track.with_duration_ms(dur);
        } else if let Some(meta_dur) = item.metadata.iter().find_map(|s| parse_duration_string(s)) {
            track = track.with_duration_ms(meta_dur);
        }
        track
    }
}

fn parse_duration_string(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 2 {
        let mins: u64 = parts[0].trim().parse().ok()?;
        let secs: u64 = parts[1].trim().parse().ok()?;
        Some((mins * 60 + secs) * 1000)
    } else if parts.len() == 3 {
        let hrs: u64 = parts[0].trim().parse().ok()?;
        let mins: u64 = parts[1].trim().parse().ok()?;
        let secs: u64 = parts[2].trim().parse().ok()?;
        Some((hrs * 3600 + mins * 60 + secs) * 1000)
    } else {
        None
    }
}

pub(super) fn artist_route_id(item: &PageItemWire) -> Option<String> {
    match (&item.open_route, &item.entity) {
        (Some(PageRoute::Artist(id)), _) | (_, Some(MediaRef::Artist(id))) => Some(id.clone()),
        _ => None,
    }
}
