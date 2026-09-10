//! Input routing shared by the terminal application and interaction tests.
use crate::{
    app::{AppState, ListKind, Msg, Overlay, Screen},
    config::{Action, KeyScope, Keymap},
    ui,
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
};
use ratcn::{
    Theme, ToasterWidget,
    runtime::{Event, EventResult, KeyCode, MouseButton, MouseKind, Ratcn, TabWrap},
};
use std::time::Duration;
pub struct App {
    pub state: AppState,
    pub keymap: Keymap,
    pub ratcn: Ratcn<AppState, Msg>,
    pub area: Rect,
}
impl App {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            keymap: Keymap::load(),
            area: Rect::default(),
            ratcn: Ratcn::new()
                .focus(|s: &AppState| &s.focus, Msg::FocusChanged)
                .modals(|s: &AppState| &s.modals)
                .tab_wrap(TabWrap::Wrap),
        }
    }
    pub fn with_keymap(mut self, keymap: Keymap) -> Self {
        self.keymap = keymap;
        self
    }
    pub fn handle_event(&mut self, event: impl TryInto<Event>, now: Duration) {
        let Ok(event) = event.try_into() else {
            return;
        };
        let message = match &event {
            Event::Key(key) => {
                let scope = match self.state.active_overlay {
                    Some(Overlay::Search) => KeyScope::Search,
                    Some(Overlay::CommandPalette) => KeyScope::CommandPalette,
                    Some(Overlay::Queue) => KeyScope::Queue,
                    Some(Overlay::Lyrics) => KeyScope::Lyrics,
                    _ => KeyScope::Global,
                };

                if let Some(action) = self.keymap.resolve(scope, key) {
                    match action {
                        Action::Quit => Some(Msg::Quit),
                        Action::Escape => Some(if self.state.active_overlay.is_some() {
                            Msg::CloseOverlay
                        } else {
                            Msg::NavigateBack
                        }),
                        Action::CloseOverlay => Some(Msg::CloseOverlay),
                        Action::NavigateBack => Some(Msg::NavigateBack),
                        Action::ToggleCommandPalette => {
                            Some(Msg::ToggleOverlay(Overlay::CommandPalette))
                        }
                        Action::ToggleHelp => Some(Msg::ToggleOverlay(Overlay::Help)),
                        Action::ToggleSettings => Some(Msg::ToggleOverlay(Overlay::Settings)),
                        Action::ToggleLyrics => Some(Msg::ToggleOverlay(Overlay::Lyrics)),
                        Action::ToggleQueue => Some(Msg::ToggleOverlay(Overlay::Queue)),
                        Action::OpenSearch => Some(Msg::OpenOverlay(Overlay::Search)),
                        Action::Refresh => Some(Msg::Refresh),
                        Action::TogglePlay => Some(Msg::TogglePlay),
                        Action::NextTrack => Some(Msg::NextTrack),
                        Action::PrevTrack => Some(Msg::PrevTrack),
                        Action::VolumeUp => Some(Msg::VolumeUp),
                        Action::VolumeDown => Some(Msg::VolumeDown),
                        Action::ToggleShuffle => Some(Msg::ToggleShuffle),
                        Action::CycleRepeat => Some(Msg::CycleRepeat),
                        Action::NavigateListenNow => Some(Msg::Navigate(Screen::ListenNow)),
                        Action::NavigateBrowse => Some(Msg::Navigate(Screen::Browse)),
                        Action::NavigateRadio => Some(Msg::Navigate(Screen::Radio)),
                        Action::NavigateLibrary => Some(Msg::Navigate(Screen::Library)),
                        Action::NavigateNowPlaying => Some(Msg::Navigate(Screen::NowPlaying)),
                        Action::SearchClear => Some(Msg::SearchClear),
                        Action::SearchBackspace => Some(Msg::SearchBackspace),
                        Action::CommandBackspace => Some(Msg::CommandBackspace),
                        _ => None,
                    }
                } else if self.state.active_overlay == Some(Overlay::Search) {
                    match key.code {
                        KeyCode::Char(c) if !key.modifiers.ctrl && !key.modifiers.alt => {
                            Some(Msg::SearchInput(c))
                        }
                        _ => None,
                    }
                } else if self.state.active_overlay == Some(Overlay::CommandPalette) {
                    match key.code {
                        KeyCode::Char(c) if !key.modifiers.ctrl && !key.modifiers.alt => {
                            Some(Msg::CommandInput(c))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            Event::Paste(text) if self.state.active_overlay == Some(Overlay::Search) => {
                Some(Msg::SearchPaste(text.clone()))
            }
            Event::Mouse(mouse) if matches!(mouse.kind, MouseKind::Down(MouseButton::Left)) => self
                .state
                .active_overlay
                .as_ref()
                .filter(|o| {
                    !ui::overlays::bounds(self.area, o)
                        .contains(Position::new(mouse.column, mouse.row))
                })
                .map(|_| Msg::CloseOverlay),
            _ => None,
        };
        if let Some(message) = message {
            self.state.update(message, now);
            return;
        }
        match self.ratcn.handle_event(event.clone(), &self.state) {
            EventResult::Emit(msg) => self.state.update(msg, now),
            EventResult::Consumed => {}
            EventResult::Ignored => {
                let kind = match self.state.active_overlay {
                    Some(Overlay::Search) => ListKind::Search,
                    Some(Overlay::Queue) => ListKind::Queue,
                    Some(Overlay::CommandPalette | Overlay::Help) => ListKind::Commands,
                    None => ListKind::Main,
                    _ => return,
                };
                if let Event::Key(key) = event {
                    let rows = self.area.height.saturating_sub(12).max(1) as usize;
                    let msg = match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            Some(Msg::MoveSelection(kind, -1, rows))
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            Some(Msg::MoveSelection(kind, 1, rows))
                        }
                        KeyCode::PageDown => Some(Msg::MoveSelection(kind, rows as i32, rows)),
                        KeyCode::PageUp => Some(Msg::MoveSelection(kind, -(rows as i32), rows)),
                        KeyCode::Home => Some(Msg::SelectRow(kind, 0, rows)),
                        KeyCode::End => Some(Msg::SelectRow(
                            kind,
                            self.state.list_len(kind).saturating_sub(1),
                            rows,
                        )),
                        KeyCode::Enter => self.state.selection_action(kind),
                        _ => None,
                    };
                    if let Some(msg) = msg {
                        self.state.update(msg, now);
                    }
                }
            }
        }
    }
    pub fn draw(&mut self, frame: &mut Frame, theme: &Theme, now: Duration) {
        self.state.now = now;
        self.area = frame.area();
        let _ = self.state.toasts.prune_expired(now);
        self.ratcn
            .render(frame, self.area, &self.state, theme, |ctx| {
                ui::render_app(ctx, &self.state, theme, self.area)
            });
        frame.render_widget(
            ToasterWidget::new(&self.state.toasts, now).themed(theme),
            self.area,
        );
    }
}
