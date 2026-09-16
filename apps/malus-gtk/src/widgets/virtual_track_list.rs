//! High performance virtualized track list using GTK4 ListView and gio::ListStore.
//!
//! Recycles a fixed pool of ~25-30 row widgets regardless of library size (100 to 50,000+ tracks).
//! Eliminates layout lag, widget allocation memory leaks, and dropped frames during scrolling.

use std::cell::RefCell;

use malus_ipc::wire::{PageActionWire, PageItemWire};
use malus_model::MediaRef;
use relm4::ComponentSender;
use relm4::gtk::{self, gio, glib, prelude::*, subclass::prelude::*};

use crate::design::tokens::*;
use crate::model::format_time;
use crate::pages::feed::{FeedOutput, FeedPage};
use crate::widgets::actions_menu::{ActionMenuCommand, build_action_popover_with_actions};

// ──────────────────────── TrackObject GObject Model ────────────────────────

mod track_obj_imp {
    use super::*;

    #[derive(Default)]
    pub struct TrackObject {
        pub item: RefCell<Option<PageItemWire>>,
        pub index: RefCell<usize>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TrackObject {
        const NAME: &'static str = "MalusTrackObject";
        type Type = super::TrackObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for TrackObject {}
}

glib::wrapper! {
    pub struct TrackObject(ObjectSubclass<track_obj_imp::TrackObject>);
}

impl TrackObject {
    pub fn new(item: PageItemWire, index: usize) -> Self {
        let obj: Self = glib::Object::new();
        *obj.imp().item.borrow_mut() = Some(item);
        *obj.imp().index.borrow_mut() = index;
        obj
    }

    pub fn item(&self) -> Option<PageItemWire> {
        self.imp().item.borrow().clone()
    }

    pub fn index(&self) -> usize {
        *self.imp().index.borrow()
    }
}

// ──────────────────────── VirtualRowWidget GObject ────────────────────────

mod row_widget_imp {
    use super::*;

    pub struct VirtualRowWidget {
        pub index_lbl: gtk::Label,
        pub title_lbl: gtk::Label,
        pub explicit_lbl: gtk::Label,
        pub artist_lbl: gtk::Label,
        pub duration_lbl: gtk::Label,
        pub fav_btn: gtk::Button,
        pub menu_btn: gtk::Button,
        pub current_item: RefCell<Option<PageItemWire>>,
        pub sender: RefCell<Option<ComponentSender<FeedPage>>>,
    }

    impl Default for VirtualRowWidget {
        fn default() -> Self {
            Self {
                index_lbl: gtk::Label::new(None),
                title_lbl: gtk::Label::new(None),
                explicit_lbl: gtk::Label::new(None),
                artist_lbl: gtk::Label::new(None),
                duration_lbl: gtk::Label::new(None),
                fav_btn: gtk::Button::new(),
                menu_btn: gtk::Button::new(),
                current_item: RefCell::new(None),
                sender: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VirtualRowWidget {
        const NAME: &'static str = "MalusVirtualRowWidget";
        type Type = super::VirtualRowWidget;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for VirtualRowWidget {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_orientation(gtk::Orientation::Horizontal);
            obj.set_spacing(12);
            obj.add_css_class("track-row");
            obj.set_hexpand(true);
            obj.set_height_request(48);

            // Index
            self.index_lbl.add_css_class("track-number");
            self.index_lbl.set_width_chars(3);
            self.index_lbl.set_xalign(1.0);
            self.index_lbl.set_valign(gtk::Align::Center);
            let idx_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            idx_box.set_size_request(32, -1);
            idx_box.set_halign(gtk::Align::Center);
            idx_box.set_valign(gtk::Align::Center);
            idx_box.append(&self.index_lbl);
            obj.append(&idx_box);

            // Title & Artist
            let meta_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
            meta_box.set_hexpand(true);
            meta_box.set_valign(gtk::Align::Center);

            let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            self.title_lbl.set_xalign(0.0);
            self.title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
            self.title_lbl.add_css_class("track-title");
            title_row.append(&self.title_lbl);

            self.explicit_lbl.add_css_class("badge-explicit");
            self.explicit_lbl.set_text("E");
            self.explicit_lbl.set_visible(false);
            title_row.append(&self.explicit_lbl);
            meta_box.append(&title_row);

            self.artist_lbl.set_xalign(0.0);
            self.artist_lbl
                .set_ellipsize(gtk::pango::EllipsizeMode::End);
            self.artist_lbl.add_css_class("track-artist");
            meta_box.append(&self.artist_lbl);
            obj.append(&meta_box);

            // Duration
            self.duration_lbl.add_css_class("track-duration");
            self.duration_lbl.set_valign(gtk::Align::Center);
            obj.append(&self.duration_lbl);

            // Favorite button
            self.fav_btn.add_css_class("flat");
            self.fav_btn.add_css_class("track-action-btn");
            self.fav_btn.set_icon_name(ICON_FAVORITE_OUTLINE);
            self.fav_btn.set_valign(gtk::Align::Center);
            obj.append(&self.fav_btn);

            // Menu button
            self.menu_btn.add_css_class("flat");
            self.menu_btn.add_css_class("track-action-btn");
            self.menu_btn.set_icon_name(ICON_MORE);
            self.menu_btn.set_tooltip_text(Some("More actions"));
            self.menu_btn.set_valign(gtk::Align::Center);
            obj.append(&self.menu_btn);
        }
    }

    impl WidgetImpl for VirtualRowWidget {}
    impl BoxImpl for VirtualRowWidget {}
}

glib::wrapper! {
    pub struct VirtualRowWidget(ObjectSubclass<row_widget_imp::VirtualRowWidget>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for VirtualRowWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualRowWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn setup(&self, sender: ComponentSender<FeedPage>) {
        *self.imp().sender.borrow_mut() = Some(sender.clone());

        // Click on row to play
        let gesture = gtk::GestureClick::new();
        let this = self.downgrade();
        gesture.connect_released(move |_g, n_press, _x, _y| {
            let Some(this) = this.upgrade() else {
                return;
            };
            if n_press >= 1
                && let Some(ref item) = *this.imp().current_item.borrow()
            {
                let media_ref = item.entity.clone().unwrap_or_else(|| {
                    MediaRef::parse(&item.id).unwrap_or_else(|_| MediaRef::Song(item.id.clone()))
                });
                if let Some(ref s) = *this.imp().sender.borrow() {
                    let _ = s.output(FeedOutput::Play(media_ref));
                }
            }
        });
        self.add_controller(gesture);

        // Favorite button click
        let this_fav = self.downgrade();
        self.imp().fav_btn.connect_clicked(move |_| {
            let Some(this_fav) = this_fav.upgrade() else {
                return;
            };
            if let Some(ref item) = *this_fav.imp().current_item.borrow() {
                let media_ref = item.entity.clone().unwrap_or_else(|| {
                    MediaRef::parse(&item.id).unwrap_or_else(|_| MediaRef::Song(item.id.clone()))
                });
                if let Some(ref s) = *this_fav.imp().sender.borrow() {
                    let is_fav = item.is_favorite();
                    let action = if is_fav {
                        PageActionWire::Unfavorite(media_ref)
                    } else {
                        PageActionWire::Favorite(media_ref)
                    };
                    let _ = s.output(FeedOutput::Action(action));
                }
            }
        });

        // Lazy action popover on menu_btn clicked
        let this_menu = self.downgrade();
        self.imp().menu_btn.connect_clicked(move |btn| {
            let Some(this_menu) = this_menu.upgrade() else {
                return;
            };
            if let Some(ref item) = *this_menu.imp().current_item.borrow()
                && let Some(ref s) = *this_menu.imp().sender.borrow()
            {
                let s_clone = s.clone();
                let media_ref = item.entity.clone().unwrap_or_else(|| {
                    MediaRef::parse(&item.id).unwrap_or_else(|_| MediaRef::Song(item.id.clone()))
                });
                let popover = build_action_popover_with_actions(
                    &media_ref,
                    item.is_favorite(),
                    item.in_library(),
                    true,
                    &item.actions,
                    move |cmd| match cmd {
                        ActionMenuCommand::Action(action) => {
                            let _ = s_clone.output(FeedOutput::Action(action));
                        }
                        ActionMenuCommand::ViewCredits(r) => {
                            let _ = s_clone.output(FeedOutput::ViewCredits(r));
                        }
                        ActionMenuCommand::AddToPlaylist(r) => {
                            let _ = s_clone.output(FeedOutput::ShowAddToPlaylist(r));
                        }
                        ActionMenuCommand::RemoveFromPlaylist {
                            playlist,
                            track_index,
                            expected_track,
                        } => {
                            let _ = s_clone.output(FeedOutput::RemoveTrackFromPlaylist {
                                playlist,
                                track_index,
                                expected_track,
                            });
                        }
                    },
                );
                popover.set_parent(btn);
                popover.connect_closed(|p| {
                    p.unparent();
                });
                popover.popup();
            }
        });
    }

    pub fn bind(&self, item: &PageItemWire, index: usize) {
        *self.imp().current_item.borrow_mut() = Some(item.clone());

        self.imp().index_lbl.set_text(&(index + 1).to_string());
        self.imp().title_lbl.set_text(&item.title);
        self.imp()
            .artist_lbl
            .set_text(item.subtitle.as_deref().unwrap_or(""));

        let is_explicit = item.badges.iter().any(|b| b.label == "E");
        self.imp().explicit_lbl.set_visible(is_explicit);

        let dur_str = item
            .duration_ms
            .map(format_time)
            .or_else(|| item.metadata.first().cloned())
            .unwrap_or_default();
        self.imp().duration_lbl.set_text(&dur_str);

        let is_fav = item.is_favorite();
        self.imp().fav_btn.set_icon_name(if is_fav {
            ICON_FAVORITE
        } else {
            ICON_FAVORITE_OUTLINE
        });
        if is_fav {
            self.imp()
                .fav_btn
                .set_css_classes(&["flat", "track-action-btn", "favorite-active"]);
            self.imp().fav_btn.set_tooltip_text(Some("Unfavorite"));
        } else {
            self.imp()
                .fav_btn
                .set_css_classes(&["flat", "track-action-btn"]);
            self.imp().fav_btn.set_tooltip_text(Some("Favorite"));
        }
    }
}

// ──────────────────────── VirtualTrackList Controller ────────────────────────

pub struct VirtualTrackList {
    container: gtk::Box,
    store: gio::ListStore,
    scrolled: gtk::ScrolledWindow,
    list_view: gtk::ListView,
    spinner_box: gtk::Box,
}

impl VirtualTrackList {
    pub fn new(sender: ComponentSender<FeedPage>) -> Self {
        let store = gio::ListStore::new::<TrackObject>();
        let selection = gtk::NoSelection::new(Some(store.clone()));

        let factory = gtk::SignalListItemFactory::new();
        let s = sender.clone();
        factory.connect_setup(move |_factory, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = VirtualRowWidget::new();
            row.setup(s.clone());
            list_item.set_child(Some(&row));
        });

        factory.connect_bind(move |_factory, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            if let Some(track_obj) = list_item.item().and_downcast::<TrackObject>()
                && let Some(item) = track_obj.item()
                && let Some(row) = list_item.child().and_downcast::<VirtualRowWidget>()
            {
                row.bind(&item, track_obj.index());
            }
        });

        let list_view = gtk::ListView::new(Some(selection), Some(factory));
        list_view.set_show_separators(false);
        list_view.add_css_class("track-list-view");

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .child(&list_view)
            .build();

        let spinner_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk::Align::Center)
            .margin_top(12)
            .margin_bottom(16)
            .visible(false)
            .build();
        let spinner = gtk::Spinner::new();
        spinner.start();
        let label = gtk::Label::new(Some("Loading more songs…"));
        label.add_css_class("dim-label");
        spinner_box.append(&spinner);
        spinner_box.append(&label);

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .vexpand(true)
            .hexpand(true)
            .build();

        container.append(&scrolled);
        container.append(&spinner_box);

        Self {
            container,
            store,
            scrolled,
            list_view,
            spinner_box,
        }
    }

    pub fn update_media_state(&self, state: &malus_model::AccountMediaState) {
        for index in 0..self.store.n_items() {
            let Some(object) = self.store.item(index).and_downcast::<TrackObject>() else {
                continue;
            };
            let Some(mut item) = object.item() else {
                continue;
            };
            if item.entity.as_ref() == Some(&state.reference) {
                item.is_favorite = Some(state.favorite);
                item.in_library = Some(state.in_library);
                self.store
                    .splice(index, 1, &[TrackObject::new(item, index as usize)]);
            }
        }
    }

    pub fn set_items(&self, items: &[PageItemWire]) {
        let objects: Vec<_> = items
            .iter()
            .enumerate()
            .map(|(index, item)| TrackObject::new(item.clone(), index))
            .collect();
        self.store.splice(0, self.store.n_items(), &objects);
    }

    pub fn append_items(&self, new_items: &[PageItemWire]) {
        let start_idx = self.store.n_items() as usize;
        for (idx, item) in new_items.iter().enumerate() {
            self.store
                .append(&TrackObject::new(item.clone(), start_idx + idx));
        }
    }

    pub fn set_loading(&self, loading: bool) {
        self.spinner_box.set_visible(loading);
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    pub fn scrolled_window(&self) -> &gtk::ScrolledWindow {
        &self.scrolled
    }

    pub fn list_view(&self) -> &gtk::ListView {
        &self.list_view
    }
}
