//! Malus terminal client.
use malus::{
    app::{AppState, Msg},
    cli::{Mode, Options},
    controller::App,
    engine, ui,
};
use ratcn::terminal::{Session, SessionEvent, SessionOptions};
use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

fn main() -> io::Result<()> {
    let options = Options::parse(std::env::args().skip(1))?;
    let custom_browser = options.browser;
    let demo = options.mode == Mode::Demo;
    match options.mode {
        Mode::Help => {
            println!(
                "Malus — Apple Music for your terminal\n\nUsage: malus [OPTION] [--browser-path PATH]\n\n  --login            Sign in using Apple's browser window\n  --logout           Erase the isolated session (close Malus first)\n  --status           Check MusicKit authorization\n  --detect-browsers  List installed browser candidates\n  --demo             Preview the UI without audio or a browser\n  -h, --help         Show help\n\nIn the player: ? for help, / to search, q for queue, Ctrl+C to quit."
            );
            return Ok(());
        }
        Mode::Detect => {
            for browser in engine::discover_browsers(custom_browser.as_deref()) {
                println!(
                    "{} · {}",
                    browser.display_name,
                    if browser.known_widevine {
                        "browser family normally supports Widevine"
                    } else {
                        "Widevine unverified"
                    }
                );
            }
            return Ok(());
        }
        Mode::Logout => {
            engine::ProfileManager::new()?.wipe_session()?;
            println!("Signed out. Local Apple Music session removed.");
            return Ok(());
        }
        Mode::Status => return status(custom_browser.as_deref()),
        Mode::Login => return login(custom_browser.as_deref()),
        _ => {}
    }
    let started = Instant::now();
    let mut app = App::new(if demo {
        AppState::demo()
    } else {
        AppState::new()
    });
    // Open the terminal before starting Chromium, so slow startup stays responsive.
    let mut session = Session::open(SessionOptions::new().mouse().adaptive())?;
    let rt = tokio::runtime::Runtime::new()?;
    let browser_path = custom_browser.clone();
    let mut startup = if demo {
        None
    } else {
        Some(rt.spawn(async move {
            let name = engine::select_best_browser(browser_path.as_deref())
                .map(|b| b.display_name)
                .unwrap_or_default();
            engine::start_engine(browser_path.as_deref(), false)
                .await
                .map(|(h, rx)| (name, h, rx))
        }))
    };
    let mut handle: Option<engine::EngineHandle> = None;
    let mut event_rx: Option<tokio::sync::mpsc::Receiver<engine::MusicKitEvent>> = None;
    let (cover_tx, mut cover_rx) = tokio::sync::mpsc::channel(8);
    let mut requested = std::collections::HashSet::new();
    let mut last_tick = Instant::now();
    let mut reconnect_at = None;
    let mut reconnect_attempts = 0u32;
    let result: io::Result<()> = (|| {
        loop {
            let now = started.elapsed();
            if app.state.reconnect_requested && startup.is_none() {
                app.state.reconnect_requested = false;
                if let Some(old) = handle.take() {
                    rt.block_on(old.shutdown());
                }
                event_rx = None;
                app.state.engine_tx = None;
                app.state.is_authorized = false;
                reconnect_attempts += 1;
                reconnect_at =
                    Some(now + Duration::from_secs((1u64 << reconnect_attempts.min(4)).min(15)));
            }
            if reconnect_at.is_some_and(|due| now >= due) && startup.is_none() {
                reconnect_at = None;
                app.state.connecting = true;
                app.state.library_loading = true;
                let path = custom_browser.clone();
                startup = Some(rt.spawn(async move {
                    let name = engine::select_best_browser(path.as_deref())
                        .map(|b| b.display_name)
                        .unwrap_or_default();
                    engine::start_engine(path.as_deref(), false)
                        .await
                        .map(|(h, rx)| (name, h, rx))
                }));
            }
            if startup.as_ref().is_some_and(|t| t.is_finished())
                && let Some(task) = startup.take()
            {
                match rt.block_on(task) {
                    Ok(Ok((name, h, rx))) => {
                        app.state.engine_tx = Some(h.cmd_tx.clone());
                        app.state.update(Msg::EngineConnected(name), now);
                        reconnect_attempts = 0;
                        handle = Some(h);
                        event_rx = Some(rx);
                    }
                    Ok(Err(e)) => {
                        app.state.update(Msg::EngineError(e.to_string()), now);
                        if reconnect_attempts > 0 && reconnect_attempts < 4 {
                            app.state.reconnect_requested = true;
                        }
                    }
                    Err(e) => {
                        app.state.update(Msg::EngineError(e.to_string()), now);
                        if reconnect_attempts > 0 && reconnect_attempts < 4 {
                            app.state.reconnect_requested = true;
                        }
                    }
                }
            }
            if let Some(rx) = &mut event_rx {
                while let Ok(event) = rx.try_recv() {
                    app.state.update(Msg::EngineMusicKitEvent(event), now);
                }
            }
            while let Ok((url, cover)) = cover_rx.try_recv() {
                if app.state.artwork.len() >= 64 {
                    app.state.artwork.clear();
                    requested.clear();
                }
                app.state.artwork.insert(url, cover);
            }
            let mut covers: Vec<String> = app
                .state
                .library
                .albums
                .iter()
                .take(3)
                .filter_map(|a| {
                    a.track_ids
                        .first()
                        .and_then(|id| app.state.find_track(id))
                        .and_then(|t| t.artwork_url.clone())
                })
                .collect();
            if let Some(url) = app
                .state
                .player
                .current_track()
                .and_then(|t| t.artwork_url.clone())
            {
                covers.push(url);
            }
            for url in covers {
                if requested.insert(url.clone()) {
                    let tx = cover_tx.clone();
                    rt.spawn(async move {
                        if let Some(cover) = ui::artwork::Cover::load(&url).await {
                            let _ = tx.send((url, cover)).await;
                        }
                    });
                }
            }
            if app.state.should_quit {
                break;
            }
            if last_tick.elapsed() >= Duration::from_millis(100) {
                app.state.update(Msg::Tick, now);
                last_tick = Instant::now();
            }
            let theme = ui::theme(&session.theme());
            session
                .terminal_mut()
                .draw(|frame| app.draw(frame, &theme, now))?;
            // Flush Kitty image AFTER ratatui draw so the buffer flush doesn't clobber it.
            if malus::ui::artwork::supports_kitty_graphics()
                && let Some(track) = app.state.player.current_track()
                && app.state.active_screen == malus::app::Screen::NowPlaying
                && let Some(url) = track.artwork_url.as_ref()
                && let Some(cover) = app.state.artwork.get(url)
            {
                // Reproduce the exact content geometry from ui/mod.rs
                let term = session.terminal_mut().size()?;
                let margin: u16 = if term.width >= 65 { 2 } else { 1 };
                let top_h: u16 = if term.height >= 22 { 3 } else { 1 };
                let content_y = top_h + 2; // top_bar + rule + inset(1)
                let content_h = term.height.saturating_sub(top_h + 6); // top(top_h) + rule(1) + margin(1) + margin(1) + rule(1) + player(3)
                let content_w = term.width.saturating_sub(margin * 2);
                if content_w >= 78 {
                    let left_w = (content_w * 38 / 100)
                        .clamp(34, 44)
                        .min(content_w.saturating_sub(20));
                    let art_h = left_w.min(content_h.saturating_sub(4));
                    let art_rect = ratatui::layout::Rect::new(margin, content_y, left_w, art_h);
                    malus::ui::artwork::flush_kitty_image(&cover.raw_bytes, art_rect);
                }
            }
            let moving = !app.state.reduced_motion
                && (app.state.connecting
                    || app.state.player.status == malus::model::PlaybackStatus::Playing
                    || now.saturating_sub(app.state.transition_at) < Duration::from_millis(180));
            let wait = if moving {
                Duration::from_millis(40)
            } else {
                Duration::from_millis(100)
            };
            if let Some(SessionEvent::Input(event)) = session.next(Some(wait))? {
                app.handle_event(event, started.elapsed());
            }
        }
        Ok(())
    })();
    drop(session);
    if let Some(task) = startup {
        task.abort();
        let _ = rt.block_on(task);
    }
    if let Some(handle) = handle {
        rt.block_on(handle.shutdown());
    }
    rt.shutdown_timeout(Duration::from_secs(3));
    result
}

fn candidate(path: Option<&Path>) -> io::Result<engine::BrowserCandidate> {
    engine::select_best_browser(path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No Chromium browser found"))
}
async fn authorized(cdp: &engine::CdpClient) -> io::Result<Option<bool>> {
    let value=cdp.evaluate_js("(() => {try {const m=window.MusicKit && MusicKit.getInstance();return m ? !!m.isAuthorized : null;} catch (_) {return null;}})()").await?;
    Ok(value.as_bool())
}
fn status(path: Option<&Path>) -> io::Result<()> {
    let candidate = candidate(path)?;
    println!("Engine: {}", candidate.display_name);
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let browser = engine::BrowserProcess::spawn(
            &candidate,
            engine::ProfileManager::new()?,
            false,
            "https://music.apple.com/",
        )?;
        let cdp = engine::CdpClient::connect_to_page(browser.port, "music.apple.com").await?;
        let start = Instant::now();
        let mut ready = false;
        while start.elapsed() < Duration::from_secs(20) {
            match authorized(&cdp).await? {
                Some(true) => {
                    println!("Apple Music: Signed in");
                    return Ok(());
                }
                Some(false) => ready = true,
                None => {}
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        if ready {
            println!("Apple Music: Sign-in required; run malus --login");
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "MusicKit did not initialize",
            ))
        }
    })
}
fn login(path: Option<&Path>) -> io::Result<()> {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(io::Error::other(
            "A graphical desktop session is required for sign-in",
        ));
    }
    println!(
        "Sign in to Apple Music in the opened window. It closes when MusicKit confirms authorization."
    );
    let candidate = candidate(path)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut browser = engine::BrowserProcess::spawn(
            &candidate,
            engine::ProfileManager::new()?,
            true,
            "https://music.apple.com/",
        )?;
        let cdp = engine::CdpClient::connect_to_page(browser.port, "music.apple.com").await?;
        while browser.is_alive() {
            if authorized(&cdp).await.ok().flatten() == Some(true) {
                tokio::time::sleep(Duration::from_secs(1)).await;
                browser.terminate();
                println!("Signed in. Start Malus with: malus");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(io::Error::other(
            "Sign-in window closed before authorization was confirmed",
        ))
    })
}
