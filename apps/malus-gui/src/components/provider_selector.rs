//! ProviderSelector component.
//!
//! Emits `BrowseProviderSelected(String)` and `BeginAuth(String)`.
//! Capability-gated authentication button (hidden unless provider advertises "auth").

use malus_client::{AuthStateWire, ProviderInfoWire};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

pub struct ProviderSelector {
    providers: Vec<ProviderInfoWire>,
    selected_id: String,
    is_authenticating: bool,
    provider_names: gtk::StringList,
}

#[derive(Debug)]
pub enum ProviderSelectorInput {
    SetProviders(Vec<ProviderInfoWire>),
    SetSelected(String),
    DropdownChanged(u32),
    AuthClicked,
    AuthFinished(bool),
}

#[derive(Debug)]
pub enum ProviderSelectorOutput {
    BrowseProviderSelected(String),
    BeginAuth(String),
}

#[relm4::component(pub)]
impl Component for ProviderSelector {
    type Init = (Vec<ProviderInfoWire>, String);
    type Input = ProviderSelectorInput;
    type Output = ProviderSelectorOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 6,
            set_valign: gtk::Align::Center,

            #[name(dropdown)]
            gtk::DropDown {
                set_model: Some(&model.provider_names),
                add_css_class: "toolbar-dropdown",
                #[watch]
                set_selected: model.selected_index(),
                connect_selected_notify[sender] => move |dd| {
                    sender.input(ProviderSelectorInput::DropdownChanged(dd.selected()));
                },
            },

            #[name(auth_btn)]
            gtk::Button {
                add_css_class: "toolbar-btn-auth",
                #[watch]
                set_visible: model.current_provider_has_auth(),
                #[watch]
                set_sensitive: !model.is_authenticating,
                #[watch]
                set_label: model.auth_button_label(),
                connect_clicked => ProviderSelectorInput::AuthClicked,
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (providers, selected_id) = init;
        let names: Vec<&str> = providers.iter().map(|p| p.name.as_str()).collect();
        let provider_names = gtk::StringList::new(&names);

        let model = Self {
            providers,
            selected_id,
            is_authenticating: false,
            provider_names,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            ProviderSelectorInput::SetProviders(providers) => {
                self.providers = providers;
                self.rebuild_string_list();
            }
            ProviderSelectorInput::SetSelected(id) => {
                self.selected_id = id;
            }
            ProviderSelectorInput::DropdownChanged(idx) => {
                if let Some(info) = self.providers.get(idx as usize)
                    && info.id != self.selected_id
                {
                    self.selected_id = info.id.clone();
                    let _ = sender.output(ProviderSelectorOutput::BrowseProviderSelected(
                        self.selected_id.clone(),
                    ));
                }
            }
            ProviderSelectorInput::AuthClicked => {
                if self.current_provider_has_auth() {
                    self.is_authenticating = true;
                    let _ =
                        sender.output(ProviderSelectorOutput::BeginAuth(self.selected_id.clone()));
                }
            }
            ProviderSelectorInput::AuthFinished(_success) => {
                self.is_authenticating = false;
            }
        }
    }
}

impl ProviderSelector {
    fn current_provider(&self) -> Option<&ProviderInfoWire> {
        self.providers.iter().find(|p| p.id == self.selected_id)
    }

    fn current_provider_has_auth(&self) -> bool {
        self.current_provider()
            .map(|p| p.capabilities.iter().any(|c| c == "auth"))
            .unwrap_or(false)
    }

    fn auth_button_label(&self) -> &str {
        if self.is_authenticating {
            "Signing In..."
        } else if let Some(info) = self.current_provider() {
            if let Some(auth) = info.auth_state {
                match auth {
                    AuthStateWire::Authenticated => "Signed In",
                    AuthStateWire::NeedsAuth => "Sign In",
                    AuthStateWire::Authenticating => "Signing In...",
                    AuthStateWire::Failed => "Retry Sign In",
                    _ => "Sign In",
                }
            } else {
                "Sign In"
            }
        } else {
            "Sign In"
        }
    }

    fn rebuild_string_list(&self) {
        while self.provider_names.n_items() > 0 {
            self.provider_names.remove(0);
        }
        for p in &self.providers {
            self.provider_names.append(&p.name);
        }
    }

    fn selected_index(&self) -> u32 {
        self.providers
            .iter()
            .position(|p| p.id == self.selected_id)
            .unwrap_or(0) as u32
    }
}
