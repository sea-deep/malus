//! Real GTK components against an isolated IPC server. No Apple account is used.
use malus_client::MalusClient;
use malus_gtk::{
    pages::{
        feed::{FeedInput, FeedPage},
        search::{SearchInput, SearchPage},
    },
    services::ArtworkService,
};
use malus_ipc::{
    client::{ClientRequest, ClientResponse},
    codec::{decode_message, read_frame, write_message},
    wire::PageWire,
};
use malus_model::PageRoute;
use relm4::{
    gtk::{self, prelude::*},
    prelude::*,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn pump(duration: Duration) {
    let until = Instant::now() + duration;
    let context = gtk::glib::MainContext::default();
    while Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn labels(widget: &impl IsA<gtk::Widget>) -> Vec<String> {
    let widget = widget.as_ref();
    let mut output = Vec::new();
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        output.push(label.text().to_string());
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        output.extend(labels(&widget));
        child = widget.next_sibling();
    }
    output
}

#[test]
#[ignore = "requires GTK display; isolated IPC fixtures, no account or playback changes"]
fn same_query_and_route_races_keep_newer_results_and_whitespace_does_not_cancel_search() {
    gtk::init().unwrap();
    relm4::adw::init().unwrap();
    let path =
        std::env::temp_dir().join(format!("malus-ui-regression-{}.sock", std::process::id()));
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let searches = Arc::new(AtomicUsize::new(0));
    let pages = Arc::new(AtomicUsize::new(0));
    let search_count = searches.clone();
    let page_count = pages.clone();
    let server = relm4::spawn(async move {
        let listener = tokio::net::UnixListener::from_std(listener).unwrap();
        while let Ok((mut stream, _)) = listener.accept().await {
            let searches = search_count.clone();
            let pages = page_count.clone();
            relm4::spawn(async move {
                let mut buffer = Vec::new();
                let frame = read_frame(&mut stream, &mut buffer, 1_048_576)
                    .await
                    .unwrap()
                    .unwrap();
                let request: ClientRequest = decode_message(&frame).unwrap();
                let (index, response) = match request {
                    ClientRequest::Search { .. } => {
                        let index = searches.fetch_add(1, Ordering::SeqCst);
                        (
                            index,
                            ClientResponse::err(
                                "test",
                                if index == 0 {
                                    "obsolete-search-error"
                                } else {
                                    "current-search-error"
                                },
                            ),
                        )
                    }
                    ClientRequest::GetPage { route } => {
                        let index = pages.fetch_add(1, Ordering::SeqCst);
                        (
                            index,
                            ClientResponse::Page(PageWire::new(
                                route.to_string(),
                                if index == 0 {
                                    "Obsolete page"
                                } else {
                                    "Current page"
                                },
                            )),
                        )
                    }
                    other => panic!("Unexpected fixture request: {other:?}"),
                };
                tokio::time::sleep(Duration::from_millis(if index == 0 { 350 } else { 20 })).await;
                write_message(&mut stream, &response).await.unwrap();
            });
        }
    });
    let client = MalusClient::new(path.clone());
    let artwork = ArtworkService::new();
    let search = SearchPage::builder()
        .launch((client.clone(), artwork.clone()))
        .detach();
    assert!(
        labels(search.widget())
            .iter()
            .any(|label| label == "Search")
    );
    search.emit(SearchInput::ExecuteSearch("same".into()));
    pump(Duration::from_millis(80));
    assert_eq!(searches.load(Ordering::SeqCst), 1);
    search.emit(SearchInput::ExecuteSearch("same".into()));
    pump(Duration::from_millis(420));
    let text = labels(search.widget()).join("\n");
    assert!(text.contains("current-search-error"));
    assert!(!text.contains("obsolete-search-error"));
    assert!(text.contains("Try Again"));
    search.emit(SearchInput::QueryChanged("new query".into()));
    pump(Duration::from_millis(40));
    search.emit(SearchInput::QueryChanged(" new query ".into()));
    pump(Duration::from_millis(350));
    assert_eq!(
        searches.load(Ordering::SeqCst),
        3,
        "Whitespace must not strand the pending search"
    );
    search.emit(SearchInput::QueryChanged(String::new()));
    pump(Duration::from_millis(20));
    assert!(
        labels(search.widget())
            .iter()
            .any(|label| label == "Search")
    );

    let feed = FeedPage::builder()
        .launch((client, artwork, PageRoute::Home))
        .detach();
    pump(Duration::from_millis(80));
    assert_eq!(pages.load(Ordering::SeqCst), 1);
    feed.emit(FeedInput::Reload);
    pump(Duration::from_millis(420));
    let text = labels(feed.widget()).join("\n");
    assert!(text.contains("Current page"));
    assert!(!text.contains("Obsolete page"));
    server.abort();
    std::fs::remove_file(path).unwrap();
}
