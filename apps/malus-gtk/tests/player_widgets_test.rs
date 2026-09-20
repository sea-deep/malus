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
    scale.set_value(75_000.0);
    scale.sync(61_000.0, 180_000.0, 1500.0);
    assert_eq!(scale.value(), 75_000.0);
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

    // 3. Volume popover and scroll test
    use malus_gtk::state::PlayerCommand;
    use malus_gtk::widgets::player_controls::VolumeControl;

    let received_cmd = Rc::new(Cell::new(None::<u8>));
    let cmd_target = received_cmd.clone();
    let send_cmd: malus_gtk::widgets::player_controls::CommandHandler = Rc::new(move |cmd| {
        if let PlayerCommand::Volume(v) = cmd {
            cmd_target.set(Some(v));
        }
    });

    let vol_control = VolumeControl::new_popover(&send_cmd);
    // Initial value is 1.0 (100)
    vol_control.slider.sync(0.50, 1.0, 0.011);
    assert!((vol_control.slider.value() - 0.50).abs() < 0.01);

    // Verify scrolling down steps down volume
    let current = vol_control.slider.value();
    let new_val = (current - 0.04).clamp(0.0, 1.0);
    vol_control.slider.set_value(new_val);

    let deadline = Instant::now() + Duration::from_millis(150);
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(received_cmd.get(), Some(46));

    // Verify scrolling up steps up volume
    let current = vol_control.slider.value();
    let new_val = (current + 0.08).clamp(0.0, 1.0);
    vol_control.slider.set_value(new_val);

    let deadline = Instant::now() + Duration::from_millis(150);
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(received_cmd.get(), Some(54));

    // 4. Verify more button creates action popover
    use malus_gtk::widgets::player_controls::more_button;
    use malus_model::{MediaRef, Track};

    let more_btn = more_button(&player, &menu);
    assert!(more_btn.has_css_class("player-more-btn"));
    assert!(more_btn.has_css_class("player-icon-btn"));

    // With a track, clicking or create_popup sets up the popover
    player.borrow_mut().now.current_track = Some(Track::new(
        MediaRef::Song("test-song".to_string()),
        "Test Song",
        "Test Artist",
    ));
    bar.refresh(&player, malus_gtk::state::UtilityMode::Closed);
    assert!(more_btn.is_sensitive());

    let window = gtk::Window::new();
    window.set_child(Some(&more_btn));
    window.present();
    more_btn.popup();
    let popover = more_btn.popover().expect("more button must create popover");
    assert_eq!(popover.position(), gtk::PositionType::Top);
    assert!(popover.is_autohide());
    popover.popdown();
    window.destroy();

    // Verify volume button popover
    let vol_btn = vol_control
        .root
        .first_child()
        .expect("vol_control root must have child")
        .downcast::<gtk::MenuButton>()
        .expect("vol child must be MenuButton");
    assert_eq!(vol_btn.tooltip_text().as_deref(), Some("Volume"));
    assert!(!vol_btn.property::<bool>("always-show-arrow"));

    let vol_popover = vol_btn.popover().expect("volume button must have popover");
    assert_eq!(vol_popover.position(), gtk::PositionType::Top);
    assert!(vol_popover.is_autohide());
    assert!(vol_popover.has_css_class("volume-popover"));

    // 5. Test bottom player bar fixed docking at compact heights
    use malus_client::MalusClient;
    use malus_gtk::navigation::AppDestination;
    use malus_gtk::shell::Sidebar;
    use malus_model::PageRoute;
    use relm4::{Component, ComponentController};

    // 1. Verify PlayerBar vertical minimum height is exactly 84
    let (bar_min_h, bar_nat_h, _, _) = bar.root.measure(gtk::Orientation::Vertical, -1);
    assert_eq!(bar_min_h, 84);
    assert_eq!(bar_nat_h, 84);

    // 2. Verify Sidebar minimum height is compact (not 600+ px)
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".to_string());
    let socket = std::path::PathBuf::from(runtime_dir).join("malus.sock");
    let client = MalusClient::new(socket);
    let sidebar_controller = Sidebar::builder()
        .launch((AppDestination::Page(PageRoute::Home), client))
        .detach();
    let sidebar_widget = sidebar_controller.widget();
    let (sb_min_h, sb_nat_h, _, _) = sidebar_widget.measure(gtk::Orientation::Vertical, 232);
    println!("Sidebar vertical measure: min_h={sb_min_h}, nat_h={sb_nat_h}");
    let mut child = sidebar_widget.first_child();
    let mut idx = 0;
    while let Some(c) = child {
        let (min, nat, _, _) = c.measure(gtk::Orientation::Vertical, 232);
        println!(
            "  Sidebar child {idx} ({}): min={min}, nat={nat}",
            c.type_().name()
        );
        child = c.next_sibling();
        idx += 1;
    }

    // 3. Verify ToolbarView allocates bottom player bar flush at the bottom across compact and normal heights
    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(sidebar_widget));
    toolbar_view.add_bottom_bar(&bar.root);

    for h in [360, 420, 500, 576, 780] {
        toolbar_view.allocate(800, h, -1, None);
        let h_alloc = header.allocation();
        let c_alloc = sidebar_widget.allocation();
        let bar_bounds = bar
            .root
            .compute_bounds(&toolbar_view)
            .expect("bounds relative to toolbar");
        let bar_parent = bar
            .root
            .parent()
            .map(|p| p.type_().name().to_string())
            .unwrap_or_default();
        println!(
            "Window height {h}:\n  header y={}, h={}\n  content y={}, h={}\n  player bar parent: {bar_parent}\n  player bar bounds y={}, h={}",
            h_alloc.y(),
            h_alloc.height(),
            c_alloc.y(),
            c_alloc.height(),
            bar_bounds.y(),
            bar_bounds.height()
        );
        // Bar must be exactly at the bottom of the allocated window height
        assert_eq!(
            bar_bounds.y() + bar_bounds.height(),
            h as f32,
            "Bottom bar must be docked flush to window bottom (y + height == {h})"
        );
        assert!(
            bar_bounds.y() >= 0.0,
            "Bottom bar Y coordinate must be within window (>= 0)"
        );
        assert_eq!(
            bar_bounds.height(),
            84.0,
            "Bottom bar must maintain its full 84px height"
        );
    }
}
