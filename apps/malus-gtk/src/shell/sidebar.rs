//! Left navigation sidebar with search entry, Apple Music discovery, library, and user playlists.

use malus_client::MalusClient;
use malus_model::PageRoute;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::navigation::AppDestination;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistEntry {
    pub id: String,
    pub title: String,
    pub is_favorite_songs: bool,
}

pub struct Sidebar {
    active_destination: AppDestination,
    client: MalusClient,
    playlists: Vec<PlaylistEntry>,
    playlist_buttons: Vec<(AppDestination, gtk::Button)>,
}

#[derive(Debug)]
pub enum SidebarInput {
    SetActive(AppDestination),
    SearchChanged(String),
    DestinationClicked(AppDestination),
    FocusSearch,
    ReloadPlaylists,
}

#[derive(Debug, Clone)]
pub enum SidebarOutput {
    Navigate(AppDestination),
}

#[derive(Debug)]
pub enum SidebarCmd {
    PlaylistsFetched(Vec<PlaylistEntry>),
}

#[relm4::component(pub)]
impl Component for Sidebar {
    type Init = (AppDestination, MalusClient);
    type Input = SidebarInput;
    type Output = SidebarOutput;
    type CommandOutput = SidebarCmd;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "sidebar",
            set_width_request: SIDEBAR_WIDTH_NORMAL as i32,

            // Top Fixed Section: Brand + Search
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_margin_start: 10,
                set_margin_end: 10,
                set_margin_top: 8,
                set_margin_bottom: 4,

                // Brand Header — white transparent mono logo beside title
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,
                    set_margin_start: 10,
                    set_margin_bottom: 6,
                    set_valign: gtk::Align::Center,

                    gtk::Image {
                        set_icon_name: Some("malus-logo-symbolic"),
                        set_pixel_size: ICON_BRAND_SIDEBAR,
                        add_css_class: "sidebar-brand-icon",
                    },

                    gtk::Label {
                        set_xalign: 0.0,
                        set_text: "Malus",
                        add_css_class: "sidebar-brand-title",
                    },
                },
            },

            // Middle Scrollable Section: Discovery + Library + Playlists
            gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vscrollbar_policy: gtk::PolicyType::Automatic,
                set_vexpand: true,
                add_css_class: "sidebar-scrolled-window",

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 0,
                    set_margin_start: 10,
                    set_margin_end: 10,
                    set_margin_top: 2,
                    set_margin_bottom: 4,

                    // Group 1: Discovery (Search, Home, New, Radio)
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 1,
                        set_margin_bottom: 12,

                        #[name(btn_search)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::Search)),
                        },

                        #[name(btn_home)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::Home)),
                        },

                        #[name(btn_new)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::New)),
                        },

                        #[name(btn_radio)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::Radio)),
                        },
                    },

                    // Group 2: Library
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 1,

                        gtk::Label {
                            set_xalign: 0.0,
                            set_text: "Library",
                            add_css_class: "sidebar-group-header",
                            set_margin_start: 8,
                            set_margin_bottom: 4,
                        },

                        #[name(btn_recent)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::LibraryRecentlyAdded)),
                        },

                        #[name(btn_artists)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::LibraryArtists)),
                        },

                        #[name(btn_albums)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::LibraryAlbums)),
                        },

                        #[name(btn_genres)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::LibraryGenres)),
                        },

                        #[name(btn_songs)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::LibrarySongs)),
                        },

                        #[name(btn_made_for_you)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::LibraryMadeForYou)),
                        },
                    },

                    // Pinned Playlists Header
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_margin_top: 8,
                        gtk::Label {
                            set_xalign: 0.0,
                            set_text: "Playlists",
                            add_css_class: "sidebar-group-header",
                            set_margin_start: 8,
                            set_margin_bottom: 4,
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        set_margin_bottom: 8,

                        #[name(btn_all_playlists)]
                        gtk::Button {
                            add_css_class: "sidebar-row",
                            set_focusable: true,
                            connect_clicked => SidebarInput::DestinationClicked(AppDestination::Page(PageRoute::LibraryPlaylists)),
                        },

                        // Dynamic list of user playlists
                        #[name(playlists_box)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 1,
                        },
                    },
                },
            },

            // Bottom Fixed Section (Settings)
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 1,
                set_margin_start: 10,
                set_margin_end: 10,
                set_margin_bottom: 8,
                set_margin_top: 4,

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_margin_bottom: 4,
                },

                #[name(btn_settings)]
                gtk::Button {
                    add_css_class: "sidebar-row",
                    set_focusable: true,
                    connect_clicked => SidebarInput::DestinationClicked(AppDestination::Settings),
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (dest, client) = init;
        let model = Self {
            active_destination: dest,
            client: client.clone(),
            playlists: Vec::new(),
            playlist_buttons: Vec::new(),
        };

        let widgets = view_output!();

        // Populate button contents with icon and label
        Self::setup_row(&widgets.btn_search, ICON_SEARCH, "Search");
        Self::setup_row(&widgets.btn_home, ICON_HOME, "Home");
        Self::setup_row(&widgets.btn_new, ICON_NEW, "New");
        Self::setup_row(&widgets.btn_radio, ICON_RADIO, "Radio");

        Self::setup_row(&widgets.btn_recent, ICON_RECENTLY_ADDED, "Recently Added");
        Self::setup_row(&widgets.btn_artists, ICON_ARTISTS, "Artists");
        Self::setup_row(&widgets.btn_albums, ICON_ALBUMS, "Albums");
        Self::setup_row(&widgets.btn_genres, ICON_GENRES, "Genres");
        Self::setup_row(&widgets.btn_songs, ICON_SONGS, "Songs");
        Self::setup_row(&widgets.btn_made_for_you, ICON_MADE_FOR_YOU, "Made for You");

        Self::setup_row(&widgets.btn_all_playlists, ICON_PLAYLISTS, "All Playlists");

        Self::setup_row(&widgets.btn_settings, ICON_SETTINGS, "Settings");

        // Trigger initial load of user playlists
        let c = client;
        sender.oneshot_command(async move {
            let entries = Self::fetch_playlists(&c).await;
            SidebarCmd::PlaylistsFetched(entries)
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            SidebarInput::SetActive(dest) => {
                self.active_destination = dest;
            }
            SidebarInput::SearchChanged(query) => {
                let dest = AppDestination::Search(query);
                self.active_destination = dest.clone();
                let _ = sender.output(SidebarOutput::Navigate(dest));
            }
            SidebarInput::DestinationClicked(dest) => {
                self.active_destination = dest.clone();
                let _ = sender.output(SidebarOutput::Navigate(dest));
            }
            SidebarInput::FocusSearch => {
                let dest = AppDestination::Page(PageRoute::Search);
                self.active_destination = dest.clone();
                let _ = sender.output(SidebarOutput::Navigate(dest));
            }
            SidebarInput::ReloadPlaylists => {
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let entries = Self::fetch_playlists(&c).await;
                    SidebarCmd::PlaylistsFetched(entries)
                });
            }
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            SidebarCmd::PlaylistsFetched(list) => {
                self.playlists = list;
            }
        }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());
        self.render_content(widgets, sender);
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update_cmd(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());
        self.rebuild_playlists(widgets, &sender);
        self.render_content(widgets, sender);
    }
}

impl Sidebar {
    async fn fetch_playlists(client: &MalusClient) -> Vec<PlaylistEntry> {
        let mut entries = Vec::new();
        if let Ok(page) = client.get_page(&PageRoute::LibraryPlaylists).await {
            for section in page.sections {
                for item in section.items {
                    let route_id = match item.open_route {
                        Some(PageRoute::Playlist(ref pid)) => pid.clone(),
                        _ => match item.entity {
                            Some(malus_model::MediaRef::Playlist(ref pid)) => pid.clone(),
                            _ => continue,
                        },
                    };
                    let title_lower = item.title.trim().to_lowercase();
                    let is_favorite_songs = (!item.can_edit
                        && (title_lower.contains("favor") || title_lower.contains("favour")))
                        || title_lower == "favorite songs"
                        || title_lower == "favourite songs";
                    entries.push(PlaylistEntry {
                        id: route_id,
                        title: item.title,
                        is_favorite_songs,
                    });
                }
            }
        }
        if let Some(pos) = entries.iter().position(|e| e.is_favorite_songs) {
            let fav = entries.remove(pos);
            entries.insert(0, fav);
        }
        entries
    }

    fn render_content(&self, widgets: &mut SidebarWidgets, _sender: ComponentSender<Self>) {
        // Highlight active row
        Self::set_row_active(
            &widgets.btn_search,
            self.active_destination == AppDestination::Page(PageRoute::Search)
                || self.active_destination.is_search(),
        );
        Self::set_row_active(
            &widgets.btn_home,
            self.active_destination == AppDestination::Page(PageRoute::Home),
        );
        Self::set_row_active(
            &widgets.btn_new,
            self.active_destination == AppDestination::Page(PageRoute::New),
        );
        Self::set_row_active(
            &widgets.btn_radio,
            self.active_destination == AppDestination::Page(PageRoute::Radio),
        );

        Self::set_row_active(
            &widgets.btn_recent,
            self.active_destination == AppDestination::Page(PageRoute::LibraryRecentlyAdded),
        );
        Self::set_row_active(
            &widgets.btn_artists,
            self.active_destination == AppDestination::Page(PageRoute::LibraryArtists),
        );
        Self::set_row_active(
            &widgets.btn_albums,
            self.active_destination == AppDestination::Page(PageRoute::LibraryAlbums),
        );
        Self::set_row_active(
            &widgets.btn_genres,
            self.active_destination == AppDestination::Page(PageRoute::LibraryGenres),
        );
        Self::set_row_active(
            &widgets.btn_songs,
            self.active_destination == AppDestination::Page(PageRoute::LibrarySongs),
        );
        Self::set_row_active(
            &widgets.btn_made_for_you,
            self.active_destination == AppDestination::Page(PageRoute::LibraryMadeForYou),
        );

        Self::set_row_active(
            &widgets.btn_all_playlists,
            self.active_destination == AppDestination::Page(PageRoute::LibraryPlaylists),
        );
        Self::set_row_active(
            &widgets.btn_settings,
            self.active_destination == AppDestination::Settings,
        );

        for (destination, button) in &self.playlist_buttons {
            Self::set_row_active(button, self.active_destination == *destination);
        }
    }

    fn rebuild_playlists(&mut self, widgets: &SidebarWidgets, sender: &ComponentSender<Self>) {
        self.playlist_buttons.clear();
        while let Some(child) = widgets.playlists_box.first_child() {
            widgets.playlists_box.remove(&child);
        }

        for pl in &self.playlists {
            let icon = if pl.is_favorite_songs {
                ICON_FAVORITE
            } else {
                ICON_PLAYLIST_ITEM
            };
            let btn = gtk::Button::builder()
                .css_classes(vec!["sidebar-row".to_string()])
                .focusable(true)
                .build();
            Self::setup_row(&btn, icon, &pl.title);
            let dest = AppDestination::Page(PageRoute::Playlist(pl.id.clone()));
            let is_active = self.active_destination == dest;
            Self::set_row_active(&btn, is_active);
            let s = sender.clone();
            let d = dest;
            btn.connect_clicked(move |_| {
                s.input(SidebarInput::DestinationClicked(d.clone()));
            });
            widgets.playlists_box.append(&btn);
            self.playlist_buttons.push((
                AppDestination::Page(PageRoute::Playlist(pl.id.clone())),
                btn,
            ));
        }
    }
}

impl Sidebar {
    fn setup_row(button: &gtk::Button, icon_name: &str, label_text: &str) {
        let hbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .build();

        let icon = gtk::Image::builder()
            .icon_name(icon_name)
            .pixel_size(ICON_GLYPH_SM)
            .build();
        hbox.append(&icon);

        let label = gtk::Label::builder()
            .label(label_text)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        hbox.append(&label);

        button.update_property(&[gtk::accessible::Property::Label(label_text)]);
        button.set_tooltip_text(Some(label_text));
        button.set_child(Some(&hbox));
        button.set_focusable(true);
    }

    fn set_row_active(button: &gtk::Button, is_active: bool) {
        if is_active {
            button.add_css_class("sidebar-row-active");
        } else {
            button.remove_css_class("sidebar-row-active");
        }
    }
}
