//! Settings and Diagnostics page.

use malus_client::MalusClient;
use malus_ipc::wire::{AuthStateWire, AuthStatusWire};
use relm4::adw::{self, prelude::*};
use relm4::gtk;
use relm4::prelude::*;

use crate::design::theme::{Appearance, apply_theme_settings};

pub struct SettingsPage {
    client: MalusClient,
    auth_status: Option<AuthStatusWire>,
    is_loading: bool,
    error: Option<String>,
    selected_theme_idx: u32,
    selected_accent_idx: u32,
    selected_corner_idx: u32,
    is_translucent: bool,
}

#[derive(Debug)]
pub enum SettingsInput {
    Reload,
    SignInClicked,
    SignOutClicked,
    SetTheme(u32),
    SetAccent(u32),
    SetCorner(u32),
    SetTranslucent(bool),
}

#[derive(Debug)]
pub enum SettingsCmd {
    AuthStatusLoaded(Result<AuthStatusWire, String>),
}

#[relm4::component(pub)]
impl Component for SettingsPage {
    type Init = MalusClient;
    type Input = SettingsInput;
    type Output = ();
    type CommandOutput = SettingsCmd;

    view! {
        gtk::ScrolledWindow {
            set_hscrollbar_policy: gtk::PolicyType::Never,
            set_vscrollbar_policy: gtk::PolicyType::Automatic,
            set_vexpand: true,
            set_hexpand: true,

            adw::Clamp {
                set_maximum_size: 680,
                set_tightening_threshold: 540,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 24,
                    set_margin_top: 32,
                    set_margin_bottom: 48,
                    set_margin_start: 16,
                    set_margin_end: 16,

                    gtk::Label {
                        set_xalign: 0.0,
                        add_css_class: "page-title",
                        set_text: "Settings",
                    },

                    // Appearance & Ricing Preferences Group
                    adw::PreferencesGroup {
                        set_title: "Make it yours",
                        set_description: Some("Choose the palette and finish for your music"),

                        #[name(theme_row)]
                        adw::ComboRow {
                            set_title: "Color Scheme",
                            set_subtitle: "Use your desktop appearance or choose your own",
                            set_model: Some(&gtk::StringList::new(&[
                                "Follow System",
                                "Dark",
                                "Light",
                            ])),
                            #[watch]
                            set_selected: model.selected_theme_idx,
                            connect_selected_notify[sender] => move |row| {
                                sender.input(SettingsInput::SetTheme(row.selected()));
                            },
                        },

                        #[name(accent_row)]
                        adw::ComboRow {
                            set_title: "Accent Color",
                            set_subtitle: "Highlights, selection, and playback controls",
                            set_model: Some(&gtk::StringList::new(&[
                                "System Default",
                                "Apple",
                                "Plum",
                                "Blueberry",
                                "Kiwi",
                                "Tangerine",
                                "Lemon",
                                "Blackberry",
                            ])),
                            #[watch]
                            set_selected: model.selected_accent_idx,
                            connect_selected_notify[sender] => move |row| {
                                sender.input(SettingsInput::SetAccent(row.selected()));
                            },
                        },

                        #[name(corner_row)]
                        adw::ComboRow {
                            set_title: "Artwork Corner Style",
                            set_subtitle: "Corner curvature for albums, playlists, and cards",
                            set_model: Some(&gtk::StringList::new(&[
                                "Soft",
                                "Rounded",
                                "Crisp",
                            ])),
                            #[watch]
                            set_selected: model.selected_corner_idx,
                            connect_selected_notify[sender] => move |row| {
                                sender.input(SettingsInput::SetCorner(row.selected()));
                            },
                        },

                        #[name(translucent_row)]
                        adw::SwitchRow {
                            set_title: "Soft surfaces",
                            set_subtitle: "Bespoke translucent depth for player bar, sidebar, and chrome",
                            #[watch]
                            set_active: model.is_translucent,
                            connect_active_notify[sender] => move |row| {
                                sender.input(SettingsInput::SetTranslucent(row.is_active()));
                            },
                        },
                    },

                    gtk::Label {
                        set_wrap: true,
                        set_xalign: 0.0,
                        add_css_class: "error",
                        #[watch]
                        set_visible: model.error.is_some(),
                        #[watch]
                        set_text: model.error.as_deref().unwrap_or(""),
                    },
                    // Account Preferences Group
                    adw::PreferencesGroup {
                        set_title: "Account",
                        set_description: Some("Manage your sign-in and subscription"),

                        adw::ActionRow {
                            set_title: "Status",
                            #[watch]
                            set_subtitle: &model.auth_state_description(),

                            add_suffix = &gtk::Button {
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_sensitive: !model.is_loading,
                                #[watch]
                                set_visible: model.is_authenticated(),
                                set_label: "Sign Out",
                                add_css_class: "destructive-action",
                                connect_clicked => SettingsInput::SignOutClicked,
                            },

                            add_suffix = &gtk::Button {
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_sensitive: !model.is_loading,
                                #[watch]
                                set_visible: !model.is_authenticated(),
                                set_label: "Sign In",
                                add_css_class: "suggested-action",
                                connect_clicked => SettingsInput::SignInClicked,
                            },
                        },

                        adw::ActionRow {
                            set_title: "Session Info",
                            #[watch]
                            set_subtitle: &model.session_description(),
                        },
                    },

                    // Diagnostics Preferences Group
                    adw::PreferencesGroup {
                        set_title: "Diagnostics",
                        set_description: Some("Connection details for troubleshooting"),

                        adw::ActionRow {
                            set_title: "Daemon Socket",
                            #[watch]
                            set_subtitle: &model.client.socket_path().display().to_string(),
                        },

                        adw::ActionRow {
                            set_title: "Client Application ID",
                            set_subtitle: crate::APP_ID,
                        },
                    },
                },
            },
        }
    }

    fn init(
        client: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let appearance = Appearance::load();
        let model = Self {
            client: client.clone(),
            auth_status: None,
            is_loading: true,
            error: None,
            selected_theme_idx: appearance.scheme,
            selected_accent_idx: appearance.accent,
            selected_corner_idx: appearance.corners,
            is_translucent: appearance.translucent,
        };

        let widgets = view_output!();

        // Initial fetch of auth status
        let c = client;
        sender.oneshot_command(async move {
            let status = c.get_auth_status().await.map_err(|error| error.to_string());
            SettingsCmd::AuthStatusLoaded(status)
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            SettingsInput::Reload => {
                if self.is_loading {
                    return;
                }
                self.is_loading = true;
                self.error = None;
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let status = c.get_auth_status().await.map_err(|error| error.to_string());
                    SettingsCmd::AuthStatusLoaded(status)
                });
            }
            SettingsInput::SignInClicked => {
                if self.is_loading {
                    return;
                }
                self.is_loading = true;
                self.error = None;
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    if let Err(error) = c.auth_begin().await {
                        return SettingsCmd::AuthStatusLoaded(Err(error.to_string()));
                    }
                    let status = c.get_auth_status().await.map_err(|error| error.to_string());
                    SettingsCmd::AuthStatusLoaded(status)
                });
            }
            SettingsInput::SignOutClicked => {
                if self.is_loading {
                    return;
                }
                self.is_loading = true;
                self.error = None;
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    if let Err(error) = c.auth_logout().await {
                        return SettingsCmd::AuthStatusLoaded(Err(error.to_string()));
                    }
                    let status = c.get_auth_status().await.map_err(|error| error.to_string());
                    SettingsCmd::AuthStatusLoaded(status)
                });
            }
            SettingsInput::SetTheme(idx) => {
                self.selected_theme_idx = idx;
                self.apply_theme();
            }
            SettingsInput::SetAccent(idx) => {
                self.selected_accent_idx = idx;
                self.apply_theme();
            }
            SettingsInput::SetCorner(idx) => {
                self.selected_corner_idx = idx;
                self.apply_theme();
            }
            SettingsInput::SetTranslucent(val) => {
                self.is_translucent = val;
                self.apply_theme();
            }
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            SettingsCmd::AuthStatusLoaded(status) => {
                match status {
                    Ok(status) => {
                        self.auth_status = Some(status);
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
                self.is_loading = false;
            }
        }
    }
}

impl SettingsPage {
    fn apply_theme(&mut self) {
        apply_theme_settings(
            self.selected_theme_idx,
            self.selected_accent_idx,
            self.selected_corner_idx,
            self.is_translucent,
        );
        let appearance = Appearance {
            scheme: self.selected_theme_idx,
            accent: self.selected_accent_idx,
            corners: self.selected_corner_idx,
            translucent: self.is_translucent,
        };
        if let Err(error) = appearance.save() {
            self.error = Some(format!("Couldn’t save appearance: {error}"));
        }
    }

    fn is_authenticated(&self) -> bool {
        self.auth_status
            .as_ref()
            .map(|s| s.state == AuthStateWire::Authenticated)
            .unwrap_or(false)
    }

    fn auth_state_description(&self) -> String {
        if self.is_loading {
            return "Checking account…".into();
        }
        match self.auth_status.as_ref().map(|s| s.state) {
            Some(AuthStateWire::Authenticated) => "Signed in".to_string(),
            Some(AuthStateWire::Authenticating) => "Authenticating in progress...".to_string(),
            Some(AuthStateWire::Checking) => "Checking session credentials...".to_string(),
            Some(AuthStateWire::NeedsAuth) => "Sign-in required".to_string(),
            Some(AuthStateWire::Failed) => "Authentication failed".to_string(),
            _ => "Not connected".to_string(),
        }
    }

    fn session_description(&self) -> String {
        self.auth_status
            .as_ref()
            .and_then(|s| s.message.clone())
            .unwrap_or_else(|| "No session details available".to_string())
    }
}
