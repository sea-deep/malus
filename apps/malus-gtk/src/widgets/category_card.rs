//! Category Card widget for Apple Music Browse Categories.

use malus_model::PageRoute;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::services::{ArtworkService, bind_artwork};

#[derive(Debug, Clone)]
pub struct CategoryCardInit {
    pub id: String,
    pub title: String,
    pub route: PageRoute,
    pub artwork_url: String,
    pub bg_color: String,
}

#[derive(Debug)]
pub enum CategoryCardInput {
    Clicked,
}

#[derive(Debug, Clone)]
pub enum CategoryCardOutput {
    Navigate(PageRoute),
}

pub struct CategoryCard {
    info: CategoryCardInit,
}

#[relm4::component(pub)]
impl Component for CategoryCard {
    type Init = (CategoryCardInit, ArtworkService);
    type Input = CategoryCardInput;
    type Output = CategoryCardOutput;
    type CommandOutput = ();

    view! {
        #[name(card)]
        gtk::Button {
            set_cursor_from_name: Some("pointer"),
            add_css_class: "category-card",
            set_size_request: (CATEGORY_CARD_WIDTH, CATEGORY_CARD_HEIGHT),

            #[wrap(Some)]
            set_child = &gtk::Overlay {
                #[name(pic)]
                gtk::Picture {
                    set_can_shrink: true,
                    set_content_fit: gtk::ContentFit::Cover,
                    set_size_request: (CATEGORY_CARD_WIDTH, CATEGORY_CARD_HEIGHT),
                    add_css_class: "category-card-picture",
                },

                add_overlay = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_valign: gtk::Align::End,
                    add_css_class: "category-card-scrim",

                    gtk::Label {
                        set_label: &model.info.title,
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        add_css_class: "category-card-title",
                    },
                },
            },

            connect_clicked[sender] => move |_| {
                sender.input(CategoryCardInput::Clicked);
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (info, artwork_service) = init;
        let model = Self { info };
        let widgets = view_output!();

        bind_artwork(
            &widgets.pic,
            &artwork_service,
            Some(model.info.artwork_url.clone()),
            800,
        );

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            CategoryCardInput::Clicked => {
                let _ = sender.output(CategoryCardOutput::Navigate(self.info.route.clone()));
            }
        }
    }
}
