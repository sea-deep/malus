//! Clean empty state placeholder widget.

use relm4::adw;

pub fn create_empty_state(
    icon_name: &str,
    title: &str,
    description: Option<&str>,
) -> adw::StatusPage {
    let status_page = adw::StatusPage::builder()
        .icon_name(icon_name)
        .title(title)
        .vexpand(true)
        .hexpand(true)
        .build();

    if let Some(desc) = description {
        status_page.set_description(Some(desc));
    }

    status_page
}
