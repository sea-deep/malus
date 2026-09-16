//! Real GTK layout and action routing against an isolated daemon fixture.
use malus_client::MalusClient;
use malus_gtk::app::MalusApp;
use malus_ipc::{
    client::{ClientRequest, ClientResponse},
    codec::{decode_message, read_frame, write_message},
    wire::{NavigationWire, PageActionWire, PageHeaderWire, PageWire},
};
use malus_model::{MediaRef, PageRoute, PlayerStatus, Queue};
use relm4::{adw::prelude::*, gtk, prelude::*};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn pump() {
    let end = Instant::now() + Duration::from_millis(350);
    let context = gtk::glib::MainContext::default();
    while Instant::now() < end {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut all = vec![widget.as_ref().clone()];
    let mut child = widget.as_ref().first_child();
    while let Some(widget) = child {
        all.extend(descendants(&widget));
        child = widget.next_sibling();
    }
    all
}

#[test]
#[ignore = "requires GTK display; isolated IPC, no Apple account mutations"]
fn narrow_window_minimum_and_header_controls_preserve_collection_and_shuffle() {
    gtk::init().unwrap();
    relm4::adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let css = gtk::CssProvider::new();
    css.load_from_string(include_str!("../resources/style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let path = std::env::temp_dir().join(format!("malus-layout-{}.sock", std::process::id()));
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let actions = Arc::new(Mutex::new(Vec::new()));
    let received = actions.clone();
    let server = relm4::spawn(async move {
        let listener = tokio::net::UnixListener::from_std(listener).unwrap();
        while let Ok((mut stream, _)) = listener.accept().await {
            let received = received.clone();
            relm4::spawn(async move {
                let mut buffer = Vec::new();
                while let Ok(Some(frame)) = read_frame(&mut stream, &mut buffer, 1_048_576).await {
                    let request: ClientRequest = decode_message(&frame).unwrap();
                    let response = match request {
                        ClientRequest::GetStatus => ClientResponse::Status(PlayerStatus::default()),
                        ClientRequest::GetQueue => ClientResponse::Queue(Queue::default()),
                        ClientRequest::GetNavigation => {
                            ClientResponse::Navigation(NavigationWire::new(PageRoute::Home, vec![]))
                        }
                        ClientRequest::GetPage { route } => {
                            let mut page = PageWire::new(route.to_string(), "Layout fixture");
                            let mut header = PageHeaderWire::new(
                                "A long album title that must remain inside a narrow window",
                            );
                            header.actions = vec![PageActionWire::Play(MediaRef::Album(
                                "fixture-album".into(),
                            ))];
                            page.header = Some(header);
                            ClientResponse::Page(page)
                        }
                        ClientRequest::PlayCollection { reference, shuffle } => {
                            received.lock().unwrap().push((reference, shuffle));
                            ClientResponse::Ok
                        }
                        _ => ClientResponse::Ok,
                    };
                    if write_message(&mut stream, &response).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    let app = MalusApp::builder()
        .launch(MalusClient::new(path.clone()))
        .detach();
    let window = app.widget();
    window.present();
    pump();
    // Tiling compositors own surface allocation; assert the sizing contract
    // here and verify rendered resize behavior in the native acceptance run.
    assert!(
        window.measure(gtk::Orientation::Horizontal, -1).0 <= 560,
        "The wide layout must not raise the window's narrow minimum"
    );
    let hero = descendants(window)
        .into_iter()
        .find(|w| w.has_css_class("hero-header"))
        .unwrap();
    for label in ["Play", "Shuffle"] {
        let button = descendants(&hero)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .find(|button| {
                descendants(button).iter().any(|w| {
                    w.downcast_ref::<gtk::Label>()
                        .is_some_and(|l| l.text() == label)
                })
            })
            .unwrap();
        button.emit_clicked();
        pump();
    }
    pump();
    assert_eq!(
        *actions.lock().unwrap(),
        vec![
            (MediaRef::Album("fixture-album".into()), false),
            (MediaRef::Album("fixture-album".into()), true)
        ]
    );
    window.close();
    server.abort();
    std::fs::remove_file(path).unwrap();
}
