//! Standardized section heading component.

use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

pub struct SectionHeader {
    title: String,
}

#[derive(Debug)]
pub enum SectionHeaderInput {
    SetTitle(String),
}

#[relm4::component(pub)]
impl SimpleComponent for SectionHeader {
    type Init = String;
    type Input = SectionHeaderInput;
    type Output = ();

    view! {
        gtk::Label {
            set_xalign: 0.0,
            add_css_class: "section-title",
            #[watch]
            set_text: &model.title,
        }
    }

    fn init(
        title: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self { title };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            SectionHeaderInput::SetTitle(t) => self.title = t,
        }
    }
}
