use super::*;

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
        row.set_cursor_from_name(Some("pointer"));

        let artist_id = artist_route_id(item);
        let is_selected = self.selected_artist_id == artist_id && artist_id.is_some();
        if is_selected {
            row.add_css_class("artist-row-active");
        }

        let pic = gtk::Picture::builder()
            .can_shrink(true)
            .content_fit(gtk::ContentFit::Cover)
            .width_request(36)
            .height_request(36)
            .css_classes(vec!["card-artwork-circular".to_string()])
            .build();
        bind_artwork(
            &pic,
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
                                    show_artwork: false,
                                    is_favorite: item.is_favorite(),
                                    in_library: item.in_library(),
                                    actions: item.actions.clone(),
                                    artwork_service: self.artwork_service.clone(),
                                    playlist_context: playlist_ctx
                                        .as_ref()
                                        .map(|p| (p.clone(), idx)),
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
        self.virtual_track_list = None;
        self.library_subtitle_lbl = None;

        let page = self.page_data.as_ref().unwrap().clone();

        // Clear existing sections
        self.card_controllers.clear();
        self.track_controllers.clear();
        while let Some(child) = widgets.sections_container.first_child() {
            widgets.sections_container.remove(&child);
        }

        // If LibraryArtists, render master-detail split view
        if self.route == PageRoute::LibraryArtists {
            widgets
                .scrolled_window
                .set_vscrollbar_policy(gtk::PolicyType::Never);
            widgets.main_box.set_vexpand(true);
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
            widgets.main_box.set_margin_bottom(0);
            widgets.sections_container.set_vexpand(true);
            let songs_view = self.build_library_songs_view(&page, sender.clone());
            widgets.sections_container.append(&songs_view);
            return;
        } else {
            widgets
                .scrolled_window
                .set_vscrollbar_policy(gtk::PolicyType::Automatic);
            widgets.main_box.set_vexpand(false);
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
            let eyebrow = gtk::Label::new(Some(
                if matches!(
                    self.route,
                    PageRoute::Home | PageRoute::New | PageRoute::Radio
                ) {
                    "APPLE MUSIC"
                } else {
                    "YOUR COLLECTION"
                },
            ));
            eyebrow.set_xalign(0.0);
            eyebrow.add_css_class("feed-eyebrow");
            title_box.append(&eyebrow);
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

        self.trigger_deferred_sections_check(widgets);
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

        let artist_items: Vec<_> = page
            .sections
            .iter()
            .flat_map(|s| &s.items)
            .cloned()
            .collect();

        for item in &artist_items {
            let row = self.build_artist_row(item, sender.clone());
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
            .margin_start(16)
            .margin_end(16)
            .margin_top(8)
            .vexpand(true)
            .hexpand(true)
            .build();

        if let Some(detail) = self.artist_detail.clone() {
            // Narrow window back button
            let back_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(6)
                .margin_bottom(4)
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

            // Header
            if let Some(ref header) = detail.header {
                let header_row = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(16)
                    .valign(gtk::Align::Center)
                    .build();

                let title_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(6)
                    .valign(gtk::Align::Center)
                    .build();

                let title_btn = gtk::Button::builder()
                    .css_classes(vec!["flat".to_string()])
                    .build();

                let title_inner = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(6)
                    .valign(gtk::Align::Center)
                    .build();

                let title_lbl = gtk::Label::builder()
                    .label(&header.title)
                    .xalign(0.0)
                    .css_classes(vec!["page-title".to_string()])
                    .build();
                title_inner.append(&title_lbl);

                let chevron = gtk::Image::from_icon_name("go-next-symbolic");
                title_inner.append(&chevron);
                title_btn.set_child(Some(&title_inner));

                let cat_id = header
                    .metadata
                    .iter()
                    .find_map(|m| m.strip_prefix("catalog_id:").map(|s| s.to_string()));
                if let Some(cid) = cat_id {
                    let s = sender.clone();
                    title_btn.connect_clicked(move |_| {
                        let _ = s.output(FeedOutput::Navigate(PageRoute::Artist(cid.clone())));
                    });
                }
                title_box.append(&title_btn);
                header_row.append(&title_box);

                // Action buttons
                let actions_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(8)
                    .valign(gtk::Align::Center)
                    .hexpand(true)
                    .halign(gtk::Align::End)
                    .build();

                actions_box.append(&self.build_header_actions(header, sender.clone()));
                header_row.append(&actions_box);
                detail_box.append(&header_row);
            }

            // Albums Grid
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
        split.set_content(Some(&detail_scroll));

        // Responsive behavior
        let split_ref = split.clone();
        let show_detail = self.show_artist_detail;
        let last_w = std::cell::Cell::new(0);
        split.add_tick_callback(move |widget, _| {
            let win_w = widget
                .root()
                .and_then(|r| r.downcast::<gtk::Window>().ok())
                .map(|w| w.width())
                .unwrap_or_else(|| widget.width());
            if win_w > 0 && last_w.replace(win_w) != win_w {
                let is_narrow = win_w < 850;
                split_ref.set_collapsed(is_narrow);
                if is_narrow {
                    split_ref.set_show_sidebar(!show_detail);
                } else {
                    split_ref.set_show_sidebar(true);
                }
            }
            relm4::gtk::glib::ControlFlow::Continue
        });

        split.upcast()
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
