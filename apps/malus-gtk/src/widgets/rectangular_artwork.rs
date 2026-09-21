//! A rectangular image with an explicit measurement boundary. A GtkPicture size
//! request alone is only a minimum and leaks the texture's natural dimensions.

use relm4::gtk::{self, glib, prelude::*, subclass::prelude::*};

mod imp {
    use super::*;
    use std::cell::Cell;

    #[derive(Default)]
    pub struct RectangularArtwork {
        pub picture: gtk::Picture,
        pub width: Cell<i32>,
        pub height: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RectangularArtwork {
        const NAME: &'static str = "MalusRectangularArtwork";
        type Type = super::RectangularArtwork;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for RectangularArtwork {
        fn constructed(&self) {
            self.parent_constructed();
            self.picture.set_can_shrink(true);
            self.picture.set_content_fit(gtk::ContentFit::Cover);
            self.picture.set_parent(&*self.obj());
            self.obj().set_overflow(gtk::Overflow::Hidden);
        }

        fn dispose(&self) {
            self.picture.unparent();
        }
    }

    impl WidgetImpl for RectangularArtwork {
        fn measure(&self, orientation: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => {
                    let w = self.width.get();
                    let min_w = 180.min(w);
                    (min_w, w, -1, -1)
                }
                gtk::Orientation::Vertical => {
                    let h = self.height.get();
                    let min_h = 100.min(h);
                    (min_h, h, -1, -1)
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.picture.allocate(width, height, baseline, None);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().snapshot_child(&self.picture, snapshot);
        }
    }
}

glib::wrapper! {
    pub struct RectangularArtwork(ObjectSubclass<imp::RectangularArtwork>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for RectangularArtwork {
    fn default() -> Self {
        Self::new(0, 0, "")
    }
}

impl RectangularArtwork {
    pub fn new(width: i32, height: i32, class: &str) -> Self {
        let widget: Self = glib::Object::new();
        widget.set_dimensions(width, height);
        if !class.is_empty() {
            widget.add_css_class(class);
        }
        widget.set_size_request(180.min(width), 100.min(height));
        widget.set_hexpand(true);
        widget.set_vexpand(false);
        widget
    }

    pub fn set_dimensions(&self, width: i32, height: i32) {
        let imp = self.imp();
        imp.width.set(width);
        imp.height.set(height);
        self.set_size_request(180.min(width), 100.min(height));
        self.queue_resize();
    }

    pub fn picture(&self) -> &gtk::Picture {
        &self.imp().picture
    }
}
