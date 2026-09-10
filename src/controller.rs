//! Input routing shared by the terminal application and interaction tests.
use crate::{
    app::{AppState, ListKind, Msg, Overlay, Screen},
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
    pub ratcn: Ratcn<AppState, Msg>,
    pub area: Rect,
}
impl App {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            area: Rect::default(),
            ratcn: Ratcn::new()
                .focus(|s: &AppState| &s.focus, Msg::FocusChanged)
                .modals(|s: &AppState| &s.modals)
                .tab_wrap(TabWrap::Wrap),
        }
    }
    pub fn handle_event(&mut self, event: impl TryInto<Event>, now: Duration) {
        let Ok(event) = event.try_into() else {
            return;
        };
        let message = match &event {
            Event::Key(key) => {
                let code = key.code;
                let ctrl = key.modifiers.ctrl;
                if ctrl && code == KeyCode::Char('c') {
                    Some(Msg::Quit)
                } else if code == KeyCode::Esc {
                    Some(if self.state.active_overlay.is_some() {
                        Msg::CloseOverlay
                    } else {
                        Msg::NavigateBack
                    })
                } else if ctrl && code == KeyCode::Char('k') {
                    Some(Msg::ToggleOverlay(Overlay::CommandPalette))
                } else if ctrl && code == KeyCode::Char('r') {
                    Some(Msg::Refresh)
                } else if self.state.active_overlay == Some(Overlay::Search) {
                    match code {
                        KeyCode::Backspace => Some(Msg::SearchBackspace),
                        KeyCode::Char('u') if ctrl => Some(Msg::SearchClear),
                        KeyCode::Char(c) if !ctrl && !key.modifiers.alt => {
                            Some(Msg::SearchInput(c))
                        }
                        _ => None,
                    }
                } else if self.state.active_overlay == Some(Overlay::CommandPalette) {
                    match code {
                        KeyCode::Backspace => Some(Msg::CommandBackspace),
                        KeyCode::Char(c) if !ctrl && !key.modifiers.alt => {
                            Some(Msg::CommandInput(c))
                        }
                        _ => None,
                    }
                } else if ctrl || key.modifiers.alt {
                    None
                } else {
                    match code {
                        KeyCode::Char(' ') => Some(Msg::TogglePlay),
                        KeyCode::Char('q') => Some(Msg::ToggleOverlay(Overlay::Queue)),
                        KeyCode::Char('Q') => Some(Msg::Quit),
                        KeyCode::Char('n') => Some(Msg::NextTrack),
                        KeyCode::Char('p') => Some(Msg::PrevTrack),
                        KeyCode::Char('+') | KeyCode::Char('=') => Some(Msg::VolumeUp),
                        KeyCode::Char('-') => Some(Msg::VolumeDown),
                        KeyCode::Char('s') => Some(Msg::ToggleShuffle),
                        KeyCode::Char('r') => Some(Msg::CycleRepeat),
                        KeyCode::Char('l') => Some(Msg::ToggleOverlay(Overlay::Lyrics)),
                        KeyCode::Char('/') => Some(Msg::OpenOverlay(Overlay::Search)),
                        KeyCode::Char('?') => Some(Msg::ToggleOverlay(Overlay::Help)),
                        KeyCode::Char(',') => Some(Msg::ToggleOverlay(Overlay::Settings)),
                        KeyCode::Char('1') => Some(Msg::Navigate(Screen::ListenNow)),
                        KeyCode::Char('2') => Some(Msg::Navigate(Screen::Browse)),
                        KeyCode::Char('3') => Some(Msg::Navigate(Screen::Radio)),
                        KeyCode::Char('4') => Some(Msg::Navigate(Screen::Library)),
                        KeyCode::Char('5') => Some(Msg::Navigate(Screen::NowPlaying)),
                        _ => None,
                    }
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
