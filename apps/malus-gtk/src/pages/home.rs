//! Music-led Home page backed strictly by real library data.
//!
//! Enforces:
//! - "Home" title (32px bold, zero explanatory subtitles)
//! - "Albums" section with responsive 24px-gap grid
//! - "Playlists" section with responsive 24px-gap grid
//! - Real data only (no mock recommendations, no fabricated radio)
//! - Borderless artwork-led presentation

use malus_client::{AlbumWire, LibraryKindWire, LibraryPageWire, MalusClient, PlaylistWire};
use relm4::factory::FactoryVecDeque;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::components::album_tile::{AlbumTile, AlbumTileInit, AlbumTileOutput};
use crate::components::section_header::SectionHeader;
use crate::design::tokens::*;
use crate::services::ArtworkService;

pub struct HomePage {
    client: MalusClient,
    artwork_service: ArtworkService,
    albums: FactoryVecDeque<AlbumTile>,
    playlists: FactoryVecDeque<AlbumTile>,
    albums_empty: bool,
    playlists_empty: bool,
    is_loading: bool,
    albums_header: Controller<SectionHeader>,
    playlists_header: Controller<SectionHeader>,
}

#[derive(Debug)]
pub enum HomeInput {
    Reload,
    TileSelected(String),
}

#[derive(Debug)]
pub enum HomeCmd {
    AlbumsLoaded(Vec<AlbumWire>),
    PlaylistsLoaded(Vec<PlaylistWire>),
}

#[derive(Debug)]
pub enum HomeOutput {
    PlayItem(String),
}

#[relm4::component(pub)]
impl Component for HomePage {
    type Init = (MalusClient, ArtworkService);
    type Input = HomeInput;
    type Output = HomeOutput;
    type CommandOutput = HomeCmd;

    view! {
        gtk::ScrolledWindow {
            set_hscrollbar_policy: gtk::PolicyType::Never,
            set_vscrollbar_policy: gtk::PolicyType::Automatic,
            set_vexpand: true,
            set_hexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "page",

                // Page Heading (32px bold)
                gtk::Label {
                    set_xalign: 0.0,
                    add_css_class: "page-title",
                    set_text: "Home",
                },

                // Loading spinner (subtle, quiet)
                gtk::Spinner {
                    #[watch]
                    set_spinning: model.is_loading,
                    #[watch]
                    set_visible: model.is_loading,
                    set_halign: gtk::Align::Start,
                    set_size_request: (24, 24),
                    set_margin_bottom: 16,
                },

                // --- Section: Albums ---
                #[local_ref]
                albums_header_widget -> gtk::Label {},

                #[local_ref]
                albums_flow -> gtk::FlowBox {
                    set_valign: gtk::Align::Start,
                    set_max_children_per_line: 12,
                    set_min_children_per_line: 2,
                    set_selection_mode: gtk::SelectionMode::None,
                    set_homogeneous: true,
                    set_column_spacing: GRID_GAP,
                    set_row_spacing: GRID_GAP,
                    add_css_class: "content-grid",
                },

                // Empty state for albums
                gtk::Label {
                    set_xalign: 0.0,
                    add_css_class: "secondary",
                    set_text: "No albums in library.",
                    #[watch]
                    set_visible: !model.is_loading && model.albums_empty,
                    set_margin_bottom: 24,
                },

                // --- Section: Playlists ---
                #[local_ref]
                playlists_header_widget -> gtk::Label {},

                #[local_ref]
                playlists_flow -> gtk::FlowBox {
                    set_valign: gtk::Align::Start,
                    set_max_children_per_line: 12,
                    set_min_children_per_line: 2,
                    set_selection_mode: gtk::SelectionMode::None,
                    set_homogeneous: true,
                    set_column_spacing: GRID_GAP,
                    set_row_spacing: GRID_GAP,
                    add_css_class: "content-grid",
                },

                // Empty state for playlists
                gtk::Label {
                    set_xalign: 0.0,
                    add_css_class: "secondary",
                    set_text: "No playlists in library.",
                    #[watch]
                    set_visible: !model.is_loading && model.playlists_empty,
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (client, artwork_service) = init;

        let albums_header = SectionHeader::builder()
            .launch("Albums".to_string())
            .detach();
        let playlists_header = SectionHeader::builder()
            .launch("Playlists".to_string())
            .detach();

        let albums = FactoryVecDeque::builder()
            .launch(gtk::FlowBox::default())
            .forward(sender.input_sender(), |out| match out {
                AlbumTileOutput::Selected(id) => HomeInput::TileSelected(id),
            });

        let playlists = FactoryVecDeque::builder()
            .launch(gtk::FlowBox::default())
            .forward(sender.input_sender(), |out| match out {
                AlbumTileOutput::Selected(id) => HomeInput::TileSelected(id),
            });

        let model = Self {
            client: client.clone(),
            artwork_service,
            albums,
            playlists,
            albums_empty: false,
            playlists_empty: false,
            is_loading: true,
            albums_header,
            playlists_header,
        };

        // Initial fetch of real library data
        let client_albums = client.clone();
        sender.oneshot_command(async move {
            let res = client_albums
                .get_library(LibraryKindWire::Albums, None, Some(24), None)
                .await;
            match res {
                Ok(LibraryPageWire::Albums(page)) => HomeCmd::AlbumsLoaded(page.items),
                _ => HomeCmd::AlbumsLoaded(Vec::new()),
            }
        });

        let client_playlists = client.clone();
        sender.oneshot_command(async move {
            let res = client_playlists
                .get_library(LibraryKindWire::Playlists, None, Some(12), None)
                .await;
            match res {
                Ok(LibraryPageWire::Playlists(page)) => HomeCmd::PlaylistsLoaded(page.items),
                _ => HomeCmd::PlaylistsLoaded(Vec::new()),
            }
        });

        let albums_header_widget = model.albums_header.widget();
        let playlists_header_widget = model.playlists_header.widget();
        let albums_flow = model.albums.widget();
        let playlists_flow = model.playlists.widget();
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            HomeInput::Reload => {
                self.is_loading = true;
                let client_albums = self.client.clone();
                sender.oneshot_command(async move {
                    let res = client_albums
                        .get_library(LibraryKindWire::Albums, None, Some(24), None)
                        .await;
                    match res {
                        Ok(LibraryPageWire::Albums(page)) => HomeCmd::AlbumsLoaded(page.items),
                        _ => HomeCmd::AlbumsLoaded(Vec::new()),
                    }
                });

                let client_playlists = self.client.clone();
                sender.oneshot_command(async move {
                    let res = client_playlists
                        .get_library(LibraryKindWire::Playlists, None, Some(12), None)
                        .await;
                    match res {
                        Ok(LibraryPageWire::Playlists(page)) => {
                            HomeCmd::PlaylistsLoaded(page.items)
                        }
                        _ => HomeCmd::PlaylistsLoaded(Vec::new()),
                    }
                });
            }
            HomeInput::TileSelected(id) => {
                let _ = sender.output(HomeOutput::PlayItem(id));
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
            HomeCmd::AlbumsLoaded(album_list) => {
                eprintln!(
                    "[DEBUG home] AlbumsLoaded: received {} albums from library",
                    album_list.len()
                );
                self.is_loading = false;
                self.albums_empty = album_list.is_empty();
                let mut guard = self.albums.guard();
                guard.clear();
                for album in album_list {
                    let artist_name = album
                        .artists
                        .first()
                        .map(|a| a.name.clone())
                        .unwrap_or_default();
                    guard.push_back(AlbumTileInit {
                        id: album.id,
                        title: album.title,
                        subtitle: artist_name,
                        artwork_url: album.artwork.map(|a| a.url),
                        service: self.artwork_service.clone(),
                    });
                }
            }
            HomeCmd::PlaylistsLoaded(pl_list) => {
                eprintln!(
                    "[DEBUG home] PlaylistsLoaded: received {} playlists from library",
                    pl_list.len()
                );
                self.playlists_empty = pl_list.is_empty();
                let mut guard = self.playlists.guard();
                guard.clear();
                for pl in pl_list {
                    let subtitle = pl.curator.unwrap_or_else(|| "Playlist".to_string());
                    guard.push_back(AlbumTileInit {
                        id: pl.id,
                        title: pl.title,
                        subtitle,
                        artwork_url: pl.artwork.map(|a| a.url),
                        service: self.artwork_service.clone(),
                    });
                }
            }
        }
    }
}
