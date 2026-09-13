//! Quiet native sidebar navigation (224px).
//!
//! Enforces:
//! - 224px width (28 * 8px)
//! - 40px item row height
//! - 20px GTK symbolic icon glyphs
//! - 12px horizontal padding & 12px icon/text gap
//! - Semantic GTK icon theme inheritance
//! - Subtle selection state (6px radius, no gradients, no glows)

use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::model::Route;

pub struct Sidebar {
    active_route: Route,
}

#[derive(Debug)]
pub enum SidebarInput {
    SetActiveRoute(Route),
    ItemClicked(Route),
}

#[derive(Debug)]
pub enum SidebarOutput {
    Navigate(Route),
}

#[relm4::component(pub)]
impl SimpleComponent for Sidebar {
    type Init = Route;
    type Input = SidebarInput;
    type Output = SidebarOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 4,
            add_css_class: "sidebar",
            set_width_request: SIDEBAR_WIDTH_NORMAL as i32,

            // --- Search ---
            gtk::Button {
                add_css_class: "sidebar-item",
                #[watch]
                set_css_classes: if model.active_route == Route::Search {
                    &["sidebar-item", "sidebar-item-selected"]
                } else {
                    &["sidebar-item"]
                },
                set_tooltip_text: Some("Search"),
                connect_clicked => SidebarInput::ItemClicked(Route::Search),

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,

                    gtk::Image {
                        set_icon_name: Some(ICON_SEARCH),
                        set_pixel_size: ICON_GLYPH_MD,
                    },

                    gtk::Label {
                        set_text: "Search",
                        set_xalign: 0.0,
                        set_hexpand: true,
                    },
                },
            },

            // --- Section: DISCOVER ---
            gtk::Label {
                set_text: "DISCOVER",
                set_xalign: 0.0,
                add_css_class: "sidebar-heading",
            },

            gtk::Button {
                add_css_class: "sidebar-item",
                #[watch]
                set_css_classes: if model.active_route == Route::Home {
                    &["sidebar-item", "sidebar-item-selected"]
                } else {
                    &["sidebar-item"]
                },
                set_tooltip_text: Some("Home"),
                connect_clicked => SidebarInput::ItemClicked(Route::Home),

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,

                    gtk::Image {
                        set_icon_name: Some(ICON_HOME),
                        set_pixel_size: ICON_GLYPH_MD,
                    },

                    gtk::Label {
                        set_text: "Home",
                        set_xalign: 0.0,
                        set_hexpand: true,
                    },
                },
            },

            // --- Section: LIBRARY ---
            gtk::Label {
                set_text: "LIBRARY",
                set_xalign: 0.0,
                add_css_class: "sidebar-heading",
            },

            gtk::Button {
                add_css_class: "sidebar-item",
                #[watch]
                set_css_classes: if model.active_route == Route::Albums {
                    &["sidebar-item", "sidebar-item-selected"]
                } else {
                    &["sidebar-item"]
                },
                set_tooltip_text: Some("Albums"),
                connect_clicked => SidebarInput::ItemClicked(Route::Albums),

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,

                    gtk::Image {
                        set_icon_name: Some(ICON_ALBUMS),
                        set_pixel_size: ICON_GLYPH_MD,
                    },

                    gtk::Label {
                        set_text: "Albums",
                        set_xalign: 0.0,
                        set_hexpand: true,
                    },
                },
            },

            gtk::Button {
                add_css_class: "sidebar-item",
                #[watch]
                set_css_classes: if model.active_route == Route::Songs {
                    &["sidebar-item", "sidebar-item-selected"]
                } else {
                    &["sidebar-item"]
                },
                set_tooltip_text: Some("Songs"),
                connect_clicked => SidebarInput::ItemClicked(Route::Songs),

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,

                    gtk::Image {
                        set_icon_name: Some(ICON_SONGS),
                        set_pixel_size: ICON_GLYPH_MD,
                    },

                    gtk::Label {
                        set_text: "Songs",
                        set_xalign: 0.0,
                        set_hexpand: true,
                    },
                },
            },

            gtk::Button {
                add_css_class: "sidebar-item",
                #[watch]
                set_css_classes: if model.active_route == Route::Playlists {
                    &["sidebar-item", "sidebar-item-selected"]
                } else {
                    &["sidebar-item"]
                },
                set_tooltip_text: Some("Playlists"),
                connect_clicked => SidebarInput::ItemClicked(Route::Playlists),

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,

                    gtk::Image {
                        set_icon_name: Some(ICON_PLAYLISTS),
                        set_pixel_size: ICON_GLYPH_MD,
                    },

                    gtk::Label {
                        set_text: "Playlists",
                        set_xalign: 0.0,
                        set_hexpand: true,
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self { active_route: init };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            SidebarInput::SetActiveRoute(route) => {
                self.active_route = route;
            }
            SidebarInput::ItemClicked(route) => {
                self.active_route = route.clone();
                let _ = sender.output(SidebarOutput::Navigate(route));
            }
        }
    }
}
