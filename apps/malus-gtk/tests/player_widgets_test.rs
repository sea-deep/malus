use malus_gtk::widgets::{interactive_scale::InteractiveScale, square_artwork::SquareArtwork};
use relm4::gtk::{self, prelude::*};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires a GTK display; run explicitly during UI acceptance"]
#[allow(deprecated)]
fn test_player_widgets_and_bar_measurements() {
    gtk::init().unwrap();

    // 1. Scale sync & artwork test
    let sent = Rc::new(Cell::new(0));
    let counter = sent.clone();
    let scale = InteractiveScale::new(
        180_000.0,
        1000.0,
        false,
        |_| {},
        move |_| counter.set(counter.get() + 1),
    );
    for value in [0.0, 20_000.0, 40_000.0, 60_000.0] {
        scale.sync(value, 180_000.0, 1500.0);
    }
    assert_eq!(sent.get(), 0);
    scale.widget.set_value(75_000.0);
    scale.sync(61_000.0, 180_000.0, 1500.0);
    assert_eq!(scale.widget.value(), 75_000.0);
    let deadline = Instant::now() + Duration::from_millis(250);
    let context = gtk::glib::MainContext::default();
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(sent.get(), 1);
    scale.sync(75_200.0, 180_000.0, 1500.0);
    assert_eq!(sent.get(), 1);

    let artwork = SquareArtwork::new(56, "player-artwork");
    let bytes = gtk::glib::Bytes::from_owned(vec![255_u8; 600 * 600 * 4]);
    let texture =
        gtk::gdk::MemoryTexture::new(600, 600, gtk::gdk::MemoryFormat::R8g8b8a8, &bytes, 2400);
    artwork.picture().set_paintable(Some(&texture));
    let (minimum, natural, _, _) = artwork.measure(gtk::Orientation::Vertical, -1);
    assert_eq!((minimum, natural), (56, 56));

    // 2. Player bar measurements test
    use malus_gtk::shell::PlayerBar;
    use malus_gtk::state::PlayerPresentation;
    use std::cell::RefCell;

    let player = Rc::new(RefCell::new(PlayerPresentation::default()));
    let send: malus_gtk::widgets::player_controls::CommandHandler = Rc::new(|_| {});
    let menu: malus_gtk::widgets::player_controls::MenuHandler = Rc::new(|_| {});
    let bar = PlayerBar::new(&player, &send, &menu, || {}, || {}, || {});

    for w in [616, 750, 850, 1100] {
        let (min_w, nat_w, _, _) = bar.root.measure(gtk::Orientation::Horizontal, 84);
        println!("Width {w}: root min_w={min_w}, nat_w={nat_w}");
    }

    let layout = bar
        .root
        .first_child()
        .unwrap()
        .downcast::<gtk::CenterBox>()
        .unwrap();
    let start = layout.start_widget().unwrap();
    let center = layout.center_widget().unwrap();
    let end = layout.end_widget().unwrap();

    let (s_min, s_nat, _, _) = start.measure(gtk::Orientation::Horizontal, 84);
    let (c_min, c_nat, _, _) = center.measure(gtk::Orientation::Horizontal, 84);
    let (e_min, e_nat, _, _) = end.measure(gtk::Orientation::Horizontal, 84);

    println!("START (left): min={s_min}, nat={s_nat}");
    println!("CENTER: min={c_min}, nat={c_nat}");
    println!("END (right): min={e_min}, nat={e_nat}");

    for (w, req) in [(616, 160), (750, 240), (850, 260), (1100, 320)] {
        center.set_width_request(req);
        layout.allocate(w, 84, -1, None);
        let s_alloc = start.allocation();
        let c_alloc = center.allocation();
        let e_alloc = end.allocation();
        println!(
            "Width {w} (center req {req}) allocations:\n  START:  x={}, w={}\n  CENTER: x={}, w={}\n  END:    x={}, w={}",
            s_alloc.x(),
            s_alloc.width(),
            c_alloc.x(),
            c_alloc.width(),
            e_alloc.x(),
            e_alloc.width(),
        );
        assert!(s_alloc.x() >= 0);
        assert!(e_alloc.x() + e_alloc.width() <= w);
        assert!(c_alloc.x() >= s_alloc.x() + s_alloc.width());
        assert!(e_alloc.x() >= c_alloc.x() + c_alloc.width());
    }
}
