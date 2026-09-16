//! Horizontal scrollable media shelf container.

use crate::widgets::media_card::MediaCardInput;
use relm4::gtk::{self, prelude::*};

/// Creates a horizontal scrolling container for media cards.
pub fn create_shelf_container() -> (gtk::ScrolledWindow, gtk::Box) {
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(false)
        .overlay_scrolling(true)
        .build();
    scrolled.add_css_class("media-shelf-scrolled");

    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(16)
        .margin_start(4)
        .margin_end(4)
        .margin_top(4)
        .margin_bottom(12)
        .build();
    container.add_css_class("media-shelf-box");

    scrolled.set_child(Some(&container));
    (scrolled, container)
}

pub type ShelfSenders =
    std::rc::Rc<std::cell::RefCell<Vec<(usize, relm4::Sender<MediaCardInput>)>>>;

/// Connects scroll, resize, and allocation events to trigger deferred card loading
/// when cards approach or enter the visible horizontal shelf viewport.
pub fn hook_shelf_artwork_trigger(
    scrolled: &gtk::ScrolledWindow,
    card_size: f64,
    senders: Vec<(usize, relm4::Sender<MediaCardInput>)>,
) -> ShelfSenders {
    let senders = std::rc::Rc::new(std::cell::RefCell::new(senders));
    if senders.borrow().is_empty() {
        return senders;
    }
    let hadj = scrolled.hadjustment();
    let pitch = card_size + 16.0; // card width + 16px spacing
    let senders_clone = senders.clone();
    let check_and_load = {
        let hadj = hadj.clone();
        let senders = senders_clone;
        move || {
            let mut s_borrow = senders.borrow_mut();
            if s_borrow.is_empty() {
                return;
            }
            let page_size = hadj.page_size().max(1000.0);
            let max_visible_x = hadj.value() + page_size + 250.0;
            s_borrow.retain(|(idx, s)| {
                let card_x = *idx as f64 * pitch;
                if card_x <= max_visible_x {
                    let _ = s.send(MediaCardInput::LoadArtwork);
                    false
                } else {
                    true
                }
            });
        }
    };

    let cb1 = check_and_load.clone();
    hadj.connect_value_changed(move |_| cb1());

    let cb2 = check_and_load.clone();
    hadj.connect_page_size_notify(move |_| cb2());

    let cb3 = check_and_load;
    scrolled.connect_map(move |_| cb3());

    senders
}
