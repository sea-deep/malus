//! A square image with an explicit measurement boundary. A GtkPicture size
//! request alone is only a minimum and leaks the texture's natural dimensions.
use relm4::gtk::{self, glib, prelude::*, subclass::prelude::*};
mod imp {
    use super::*;
    use std::cell::Cell;
    #[derive(Default)]
    pub struct SquareArtwork {
        pub picture: gtk::Picture,
        pub side: Cell<i32>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for SquareArtwork {
        const NAME: &'static str = "MalusSquareArtwork";
        type Type = super::SquareArtwork;
        type ParentType = gtk::Widget;
    }
    impl ObjectImpl for SquareArtwork {
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
    impl WidgetImpl for SquareArtwork {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (self.side.get(), self.side.get(), -1, -1)
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
    pub struct SquareArtwork(ObjectSubclass<imp::SquareArtwork>) @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}
impl SquareArtwork {
    pub fn new(side: i32, class: &str) -> Self {
        let widget: Self = glib::Object::new();
        widget.set_side(side);
        widget.add_css_class(class);
        widget.set_halign(gtk::Align::Center);
        widget.set_valign(gtk::Align::Center);
        widget
    }
    pub fn set_side(&self, side: i32) {
        if self.imp().side.replace(side) != side {
            self.queue_resize();
        }
    }
    pub fn picture(&self) -> &gtk::Picture {
        &self.imp().picture
    }
}
