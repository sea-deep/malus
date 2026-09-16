//! Editorial section header with optional subtitle.

use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

pub struct SectionHeader {
    title: String,
    subtitle: Option<String>,
}

#[derive(Debug)]
pub enum SectionHeaderInput {
    SetTitle(String),
    SetSubtitle(Option<String>),
}

#[derive(Debug, Clone)]
pub struct SectionHeaderInit {
    pub title: String,
    pub subtitle: Option<String>,
}

#[relm4::component(pub)]
impl Component for SectionHeader {
    type Init = SectionHeaderInit;
    type Input = SectionHeaderInput;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 2,
            add_css_class: "section-header-box",
            set_margin_top: 16,
            set_margin_bottom: 8,

            gtk::Label {
                set_xalign: 0.0,
                add_css_class: "section-title",
                #[watch]
                set_text: &model.title,
            },

            gtk::Label {
                set_xalign: 0.0,
                add_css_class: "section-subtitle",
                #[watch]
                set_visible: model.subtitle.is_some(),
                #[watch]
                set_text: model.subtitle.as_deref().unwrap_or(""),
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            title: init.title,
            subtitle: init.subtitle,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            SectionHeaderInput::SetTitle(t) => self.title = t,
            SectionHeaderInput::SetSubtitle(s) => self.subtitle = s,
        }
    }
}
