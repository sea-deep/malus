use malus_client::MalusClient;
use malus_gtk::{
    app::{AppInput, MalusApp},
    navigation::AppDestination,
};
use malus_model::PageRoute;
use relm4::gtk::prelude::*;
use relm4::{gtk, prelude::*};
use std::time::{Duration, Instant};

fn pump(duration: Duration) {
    let until = Instant::now() + duration;
    let context = gtk::glib::MainContext::default();
    while Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn collect_track_rows(widget: &gtk::Widget, rows: &mut Vec<gtk::Widget>) {
    if widget.has_css_class("track-row") {
        rows.push(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        collect_track_rows(&c, rows);
        child = c.next_sibling();
    }
}

#[test]
#[ignore = "visual verification test against live malusd"]
fn test_playlist_row_height_coherency() {
    gtk::init().unwrap();
    relm4::adw::init().unwrap();

    let socket_path = malus_client::default_socket_path();
    let client = MalusClient::new(socket_path);
    let app_handle = MalusApp::builder().launch(client).detach();
    let root = app_handle.widget();
    root.set_default_size(800, 800);
    root.present();

    pump(Duration::from_millis(1500));

    // 1. Navigate to lup (4 items)
    println!("Navigating to lup (p.b16GBbahaoB5Wzk)...");
    app_handle.emit(AppInput::Navigate(AppDestination::Page(
        PageRoute::Playlist("p.b16GBbahaoB5Wzk".to_string()),
    )));
    pump(Duration::from_millis(2500));

    let mut lup_rows = Vec::new();
    collect_track_rows(root.upcast_ref(), &mut lup_rows);
    println!("Found {} track rows in lup", lup_rows.len());
    assert_eq!(lup_rows.len(), 4, "lup should have exactly 4 items");
    for (i, row) in lup_rows.iter().enumerate() {
        let (min_h, nat_h, _, _) = row.measure(gtk::Orientation::Vertical, -1);
        println!("  lup row {}: min_h = {}px, nat_h = {}px", i, min_h, nat_h);
        assert_eq!(min_h, 48);
        assert_eq!(nat_h, 48);
    }

    // 2. Navigate to chud (more items)
    println!("Navigating to chud (p.4Y0Jg1GsXxBYq4L)...");
    app_handle.emit(AppInput::Navigate(AppDestination::Page(
        PageRoute::Playlist("p.4Y0Jg1GsXxBYq4L".to_string()),
    )));
    pump(Duration::from_millis(2500));

    let mut chud_rows = Vec::new();
    collect_track_rows(root.upcast_ref(), &mut chud_rows);
    println!("Found {} track rows in chud", chud_rows.len());
    assert!(chud_rows.len() > 4, "chud should have more than 4 items");
    for (_i, row) in chud_rows.iter().enumerate() {
        let (min_h, nat_h, _, _) = row.measure(gtk::Orientation::Vertical, -1);
        assert_eq!(min_h, 48);
        assert_eq!(nat_h, 48);
    }

    println!(
        "\nSUCCESS: All track rows across both lup (4 tracks) and chud (many tracks) are strictly and coherently 48px!"
    );
}

fn collect_sidebar_rows(widget: &gtk::Widget, rows: &mut Vec<(String, Option<String>)>) {
    if widget.has_css_class("sidebar-row") {
        if let Some(btn) = widget.downcast_ref::<gtk::Button>() {
            let label = btn.tooltip_text().map(|s| s.to_string());
            let mut icon_name = None;
            let mut queue = vec![btn.first_child()];
            while let Some(Some(c)) = queue.pop() {
                if let Some(img) = c.downcast_ref::<gtk::Image>() {
                    icon_name = img.icon_name().map(|s| s.to_string());
                    break;
                }
                queue.push(c.next_sibling());
                queue.push(c.first_child());
            }
            if let Some(l) = label {
                rows.push((l, icon_name));
            }
        }
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        collect_sidebar_rows(&c, rows);
        child = c.next_sibling();
    }
}

#[test]
#[ignore = "visual verification test against live malusd"]
fn test_sidebar_favorite_songs_pinned_and_live_reload() {
    gtk::init().unwrap();
    relm4::adw::init().unwrap();

    let socket_path = malus_client::default_socket_path();
    let client = MalusClient::new(socket_path);
    let app_handle = MalusApp::builder().launch(client).detach();
    let root = app_handle.widget();
    root.set_default_size(1280, 800);
    root.present();

    pump(Duration::from_millis(2500));

    let mut rows = Vec::new();
    collect_sidebar_rows(root.upcast_ref(), &mut rows);
    for (i, (label, icon)) in rows.iter().enumerate() {
        println!("Sidebar row {i}: label = {:?}, icon = {:?}", label, icon);
    }

    let all_pl_idx = rows
        .iter()
        .position(|(l, _)| l == "All Playlists")
        .expect("All Playlists button must exist in sidebar");

    assert!(
        rows.len() > all_pl_idx + 1,
        "There must be playlists below All Playlists"
    );
    let (fav_label, fav_icon) = &rows[all_pl_idx + 1];
    println!(
        "Playlist right below All Playlists: label = {:?}, icon = {:?}",
        fav_label, fav_icon
    );
    assert!(
        fav_label.to_lowercase().contains("favour") || fav_label.to_lowercase().contains("favor"),
        "Playlist directly below All Playlists must be Favorite Songs, got {:?}",
        fav_label
    );
    assert_eq!(
        fav_icon.as_deref(),
        Some("starred-symbolic"),
        "Favorite Songs must have starred-symbolic icon"
    );

    if rows.len() > all_pl_idx + 2 && rows[all_pl_idx + 2].0 != "Settings" {
        let (next_label, next_icon) = &rows[all_pl_idx + 2];
        println!(
            "Subsequent playlist: label = {:?}, icon = {:?}",
            next_label, next_icon
        );
        assert_eq!(
            next_icon.as_deref(),
            Some("playlist-symbolic"),
            "Standard playlists must have playlist-symbolic icon"
        );
    }

    app_handle.emit(AppInput::ReloadPlaylists);
    pump(Duration::from_millis(2500));

    let mut reloaded_rows = Vec::new();
    collect_sidebar_rows(root.upcast_ref(), &mut reloaded_rows);
    let reloaded_all_pl_idx = reloaded_rows
        .iter()
        .position(|(l, _)| l == "All Playlists")
        .expect("All Playlists button must exist in sidebar after reload");
    let (reloaded_fav_label, reloaded_fav_icon) = &reloaded_rows[reloaded_all_pl_idx + 1];
    assert!(
        reloaded_fav_label.to_lowercase().contains("favour")
            || reloaded_fav_label.to_lowercase().contains("favor"),
        "Favorite Songs must remain locked after reload"
    );
    assert_eq!(reloaded_fav_icon.as_deref(), Some("starred-symbolic"));
    let paintable = gtk::WidgetPaintable::new(Some(root));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, 1280.0, 800.0);
    if let Some(node) = snapshot.to_node() {
        if let Some(native) = root.native() {
            if let Some(renderer) = native.renderer() {
                let texture = renderer.render_texture(&node, None);
                let artifact_path = "/home/dipak/.gemini/antigravity/brain/11ae4e26-326a-4b91-a6bc-9fad632a1100/sidebar_fav_songs.png";
                let _ = texture.save_to_png(std::path::Path::new(artifact_path));
            }
        }
    }

    println!(
        "\nSUCCESS: Favorite Songs is locked under All Playlists with starred-symbolic icon and refreshes instantly!"
    );
}
