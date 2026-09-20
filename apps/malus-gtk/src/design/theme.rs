//! Application appearance, persisted independently of playback and account state.

use relm4::adw;
use relm4::gtk;
use std::cell::{Cell, RefCell};

thread_local! {
    static CUSTOM_CSS_PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    static DARK_NOTIFY_INITIALIZED: Cell<bool> = const { Cell::new(false) };
}

fn with_css_provider<F: FnOnce(&gtk::CssProvider)>(f: F) {
    CUSTOM_CSS_PROVIDER.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            let provider = gtk::CssProvider::new();
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
                );
            }
            *opt = Some(provider);
        }
        if let Some(ref p) = *opt {
            f(p);
        }
    });
}

fn ensure_dark_mode_listener(manager: &adw::StyleManager) {
    DARK_NOTIFY_INITIALIZED.with(|cell| {
        if !cell.get() {
            cell.set(true);
            manager.connect_dark_notify(|mgr| {
                let appearance = Appearance::load();
                if appearance.scheme == 0 {
                    // Scheme 0 is Follow System: re-apply theme with new dark/light state
                    let is_dark = mgr.is_dark();
                    apply_palette_and_rules(
                        is_dark,
                        appearance.accent,
                        appearance.corners,
                        appearance.translucent,
                    );
                }
            });
        }
    });
}

/// A complete coherent color palette covering all primary GTK/Libadwaita semantic tokens.
#[derive(Debug, Clone, Copy)]
pub struct FruitPalette {
    pub accent_color: &'static str,
    pub accent_bg_color: &'static str,
    pub accent_fg_color: &'static str,
    pub window_bg_color: &'static str,
    pub window_fg_color: &'static str,
    pub view_bg_color: &'static str,
    pub view_fg_color: &'static str,
    pub headerbar_bg_color: &'static str,
    pub headerbar_fg_color: &'static str,
    pub card_bg_color: &'static str,
    pub card_fg_color: &'static str,
    pub popover_bg_color: &'static str,
    pub popover_fg_color: &'static str,
    pub sidebar_bg_color: &'static str,
    pub sidebar_fg_color: &'static str,
    pub dialog_bg_color: &'static str,
    pub dialog_fg_color: &'static str,
    pub borders: &'static str,
    pub shade_color: &'static str,
}

impl FruitPalette {
    /// Returns the bespoke Dark or Light palette for a given fruit theme index:
    /// 1: Apple, 2: Plum, 3: Blueberry, 4: Kiwi, 5: Tangerine, 6: Lemon, 7: Blackberry.
    /// Index 0 is System Default (returns None, applying zero color overrides).
    pub fn for_fruit(accent_idx: u32, is_dark: bool) -> Option<Self> {
        match (accent_idx, is_dark) {
            // 1: Apple — Signature Apple Music Tahoe Crimson Red
            (1, true) => Some(Self {
                accent_color: "#fa2d48",
                accent_bg_color: "#fa2d48",
                accent_fg_color: "#ffffff",
                window_bg_color: "#161618",
                window_fg_color: "#f5f5f7",
                view_bg_color: "#1c1c1f",
                view_fg_color: "#f5f5f7",
                headerbar_bg_color: "#161618",
                headerbar_fg_color: "#f5f5f7",
                card_bg_color: "#232327",
                card_fg_color: "#f5f5f7",
                popover_bg_color: "#26262a",
                popover_fg_color: "#f5f5f7",
                sidebar_bg_color: "#161618",
                sidebar_fg_color: "#f5f5f7",
                dialog_bg_color: "#1c1c1f",
                dialog_fg_color: "#f5f5f7",
                borders: "rgba(255, 255, 255, 0.08)",
                shade_color: "rgba(0, 0, 0, 0.36)",
            }),
            (1, false) => Some(Self {
                accent_color: "#fa2d48",
                accent_bg_color: "#fa2d48",
                accent_fg_color: "#ffffff",
                window_bg_color: "#fbfbfd",
                window_fg_color: "#1d1d1f",
                view_bg_color: "#ffffff",
                view_fg_color: "#1d1d1f",
                headerbar_bg_color: "#fbfbfd",
                headerbar_fg_color: "#1d1d1f",
                card_bg_color: "#f2f2f7",
                card_fg_color: "#1d1d1f",
                popover_bg_color: "#ffffff",
                popover_fg_color: "#1d1d1f",
                sidebar_bg_color: "#f7f7f9",
                sidebar_fg_color: "#1d1d1f",
                dialog_bg_color: "#ffffff",
                dialog_fg_color: "#1d1d1f",
                borders: "rgba(0, 0, 0, 0.08)",
                shade_color: "rgba(0, 0, 0, 0.07)",
            }),

            // 2: Plum — Royal Berry Violet
            (2, true) => Some(Self {
                accent_color: "#9141ac",
                accent_bg_color: "#9141ac",
                accent_fg_color: "#ffffff",
                window_bg_color: "#16141a",
                window_fg_color: "#f3eefa",
                view_bg_color: "#1d1a24",
                view_fg_color: "#f3eefa",
                headerbar_bg_color: "#16141a",
                headerbar_fg_color: "#f3eefa",
                card_bg_color: "#262130",
                card_fg_color: "#f3eefa",
                popover_bg_color: "#2a2536",
                popover_fg_color: "#f3eefa",
                sidebar_bg_color: "#16141a",
                sidebar_fg_color: "#f3eefa",
                dialog_bg_color: "#1d1a24",
                dialog_fg_color: "#f3eefa",
                borders: "rgba(215, 185, 245, 0.09)",
                shade_color: "rgba(0, 0, 0, 0.36)",
            }),
            (2, false) => Some(Self {
                accent_color: "#842ea0",
                accent_bg_color: "#842ea0",
                accent_fg_color: "#ffffff",
                window_bg_color: "#faf7fd",
                window_fg_color: "#1f1926",
                view_bg_color: "#ffffff",
                view_fg_color: "#1f1926",
                headerbar_bg_color: "#faf7fd",
                headerbar_fg_color: "#1f1926",
                card_bg_color: "#f3ecf8",
                card_fg_color: "#1f1926",
                popover_bg_color: "#ffffff",
                popover_fg_color: "#1f1926",
                sidebar_bg_color: "#f5f0f9",
                sidebar_fg_color: "#1f1926",
                dialog_bg_color: "#ffffff",
                dialog_fg_color: "#1f1926",
                borders: "rgba(132, 46, 160, 0.12)",
                shade_color: "rgba(0, 0, 0, 0.07)",
            }),

            // 3: Blueberry — Oceanic Deep Sky Blue
            (3, true) => Some(Self {
                accent_color: "#3584e4",
                accent_bg_color: "#3584e4",
                accent_fg_color: "#ffffff",
                window_bg_color: "#13171d",
                window_fg_color: "#eaf1fb",
                view_bg_color: "#181e26",
                view_fg_color: "#eaf1fb",
                headerbar_bg_color: "#13171d",
                headerbar_fg_color: "#eaf1fb",
                card_bg_color: "#202835",
                card_fg_color: "#eaf1fb",
                popover_bg_color: "#242e3d",
                popover_fg_color: "#eaf1fb",
                sidebar_bg_color: "#13171d",
                sidebar_fg_color: "#eaf1fb",
                dialog_bg_color: "#181e26",
                dialog_fg_color: "#eaf1fb",
                borders: "rgba(165, 205, 255, 0.08)",
                shade_color: "rgba(0, 0, 0, 0.36)",
            }),
            (3, false) => Some(Self {
                accent_color: "#1c71d8",
                accent_bg_color: "#1c71d8",
                accent_fg_color: "#ffffff",
                window_bg_color: "#f6f9fd",
                window_fg_color: "#15202e",
                view_bg_color: "#ffffff",
                view_fg_color: "#15202e",
                headerbar_bg_color: "#f6f9fd",
                headerbar_fg_color: "#15202e",
                card_bg_color: "#ecf2fb",
                card_fg_color: "#15202e",
                popover_bg_color: "#ffffff",
                popover_fg_color: "#15202e",
                sidebar_bg_color: "#f0f5fb",
                sidebar_fg_color: "#15202e",
                dialog_bg_color: "#ffffff",
                dialog_fg_color: "#15202e",
                borders: "rgba(28, 113, 216, 0.12)",
                shade_color: "rgba(0, 0, 0, 0.07)",
            }),

            // 4: Kiwi — Fresh Botanical Emerald Green
            (4, true) => Some(Self {
                accent_color: "#2ec27e",
                accent_bg_color: "#2ec27e",
                accent_fg_color: "#12241b",
                window_bg_color: "#131714",
                window_fg_color: "#eafbf2",
                view_bg_color: "#18201b",
                view_fg_color: "#eafbf2",
                headerbar_bg_color: "#131714",
                headerbar_fg_color: "#eafbf2",
                card_bg_color: "#202b24",
                card_fg_color: "#eafbf2",
                popover_bg_color: "#24322a",
                popover_fg_color: "#eafbf2",
                sidebar_bg_color: "#131714",
                sidebar_fg_color: "#eafbf2",
                dialog_bg_color: "#18201b",
                dialog_fg_color: "#eafbf2",
                borders: "rgba(165, 245, 205, 0.08)",
                shade_color: "rgba(0, 0, 0, 0.36)",
            }),
            (4, false) => Some(Self {
                accent_color: "#26a269",
                accent_bg_color: "#26a269",
                accent_fg_color: "#ffffff",
                window_bg_color: "#f6fbf8",
                window_fg_color: "#14221a",
                view_bg_color: "#ffffff",
                view_fg_color: "#14221a",
                headerbar_bg_color: "#f6fbf8",
                headerbar_fg_color: "#14221a",
                card_bg_color: "#edf7f1",
                card_fg_color: "#14221a",
                popover_bg_color: "#ffffff",
                popover_fg_color: "#14221a",
                sidebar_bg_color: "#f0f8f3",
                sidebar_fg_color: "#14221a",
                dialog_bg_color: "#ffffff",
                dialog_fg_color: "#14221a",
                borders: "rgba(38, 162, 105, 0.12)",
                shade_color: "rgba(0, 0, 0, 0.07)",
            }),

            // 5: Tangerine — Warm Sunset Peach & Amber
            (5, true) => Some(Self {
                accent_color: "#e66100",
                accent_bg_color: "#e66100",
                accent_fg_color: "#ffffff",
                window_bg_color: "#181412",
                window_fg_color: "#fbeee8",
                view_bg_color: "#201b17",
                view_fg_color: "#fbeee8",
                headerbar_bg_color: "#181412",
                headerbar_fg_color: "#fbeee8",
                card_bg_color: "#2b231f",
                card_fg_color: "#fbeee8",
                popover_bg_color: "#312723",
                popover_fg_color: "#fbeee8",
                sidebar_bg_color: "#181412",
                sidebar_fg_color: "#fbeee8",
                dialog_bg_color: "#201b17",
                dialog_fg_color: "#fbeee8",
                borders: "rgba(255, 190, 160, 0.08)",
                shade_color: "rgba(0, 0, 0, 0.36)",
            }),
            (5, false) => Some(Self {
                accent_color: "#c64600",
                accent_bg_color: "#c64600",
                accent_fg_color: "#ffffff",
                window_bg_color: "#fdf8f6",
                window_fg_color: "#291b14",
                view_bg_color: "#ffffff",
                view_fg_color: "#291b14",
                headerbar_bg_color: "#fdf8f6",
                headerbar_fg_color: "#291b14",
                card_bg_color: "#faeee8",
                card_fg_color: "#291b14",
                popover_bg_color: "#ffffff",
                popover_fg_color: "#291b14",
                sidebar_bg_color: "#fcf3ef",
                sidebar_fg_color: "#291b14",
                dialog_bg_color: "#ffffff",
                dialog_fg_color: "#291b14",
                borders: "rgba(198, 70, 0, 0.12)",
                shade_color: "rgba(0, 0, 0, 0.07)",
            }),

            // 6: Lemon — Sunlit Citrus Yellow (dark text for optimal yellow contrast)
            (6, true) => Some(Self {
                accent_color: "#f6d32d",
                accent_bg_color: "#f6d32d",
                accent_fg_color: "#181912",
                window_bg_color: "#171712",
                window_fg_color: "#fcfbe8",
                view_bg_color: "#1f1f17",
                view_fg_color: "#fcfbe8",
                headerbar_bg_color: "#171712",
                headerbar_fg_color: "#fcfbe8",
                card_bg_color: "#2a2a1f",
                card_fg_color: "#fcfbe8",
                popover_bg_color: "#303023",
                popover_fg_color: "#fcfbe8",
                sidebar_bg_color: "#171712",
                sidebar_fg_color: "#fcfbe8",
                dialog_bg_color: "#1f1f17",
                dialog_fg_color: "#fcfbe8",
                borders: "rgba(255, 240, 160, 0.08)",
                shade_color: "rgba(0, 0, 0, 0.36)",
            }),
            (6, false) => Some(Self {
                accent_color: "#e5a50a",
                accent_bg_color: "#e5a50a",
                accent_fg_color: "#ffffff",
                window_bg_color: "#fdfdf6",
                window_fg_color: "#252514",
                view_bg_color: "#ffffff",
                view_fg_color: "#252514",
                headerbar_bg_color: "#fdfdf6",
                headerbar_fg_color: "#252514",
                card_bg_color: "#faf8e7",
                card_fg_color: "#252514",
                popover_bg_color: "#ffffff",
                popover_fg_color: "#252514",
                sidebar_bg_color: "#faf8ed",
                sidebar_fg_color: "#252514",
                dialog_bg_color: "#ffffff",
                dialog_fg_color: "#252514",
                borders: "rgba(229, 165, 10, 0.14)",
                shade_color: "rgba(0, 0, 0, 0.07)",
            }),

            // 7: Blackberry — Monochromatic Dark Berry / Sophisticated Slate
            (7, true) => Some(Self {
                accent_color: "#787482",
                accent_bg_color: "#787482",
                accent_fg_color: "#ffffff",
                window_bg_color: "#131315",
                window_fg_color: "#f4f4f5",
                view_bg_color: "#19191c",
                view_fg_color: "#f4f4f5",
                headerbar_bg_color: "#131315",
                headerbar_fg_color: "#f4f4f5",
                card_bg_color: "#222226",
                card_fg_color: "#f4f4f5",
                popover_bg_color: "#26262b",
                popover_fg_color: "#f4f4f5",
                sidebar_bg_color: "#131315",
                sidebar_fg_color: "#f4f4f5",
                dialog_bg_color: "#19191c",
                dialog_fg_color: "#f4f4f5",
                borders: "rgba(255, 255, 255, 0.08)",
                shade_color: "rgba(0, 0, 0, 0.36)",
            }),
            (7, false) => Some(Self {
                accent_color: "#5e5c64",
                accent_bg_color: "#5e5c64",
                accent_fg_color: "#ffffff",
                window_bg_color: "#f4f4f6",
                window_fg_color: "#18181b",
                view_bg_color: "#ffffff",
                view_fg_color: "#18181b",
                headerbar_bg_color: "#f4f4f6",
                headerbar_fg_color: "#18181b",
                card_bg_color: "#eaeaee",
                card_fg_color: "#18181b",
                popover_bg_color: "#ffffff",
                popover_fg_color: "#18181b",
                sidebar_bg_color: "#eeeeee",
                sidebar_fg_color: "#18181b",
                dialog_bg_color: "#ffffff",
                dialog_fg_color: "#18181b",
                borders: "rgba(0, 0, 0, 0.08)",
                shade_color: "rgba(0, 0, 0, 0.07)",
            }),

            // 0 or out of range: System Default (no override, defer to system GTK theme)
            _ => None,
        }
    }

    /// Formats the palette as GTK CSS `@define-color` definitions and suggested action button style.
    pub fn to_css(&self) -> String {
        format!(
            "@define-color accent_color {};\n\
             @define-color accent_bg_color {};\n\
             @define-color accent_fg_color {};\n\
             @define-color window_bg_color {};\n\
             @define-color window_fg_color {};\n\
             @define-color view_bg_color {};\n\
             @define-color view_fg_color {};\n\
             @define-color headerbar_bg_color {};\n\
             @define-color headerbar_fg_color {};\n\
             @define-color headerbar_backdrop_color {};\n\
             @define-color card_bg_color {};\n\
             @define-color card_fg_color {};\n\
             @define-color popover_bg_color {};\n\
             @define-color popover_fg_color {};\n\
             @define-color dialog_bg_color {};\n\
             @define-color dialog_fg_color {};\n\
             @define-color sidebar_bg_color {};\n\
             @define-color sidebar_fg_color {};\n\
             @define-color borders {};\n\
             @define-color shade_color {};\n\
             @define-color slider_bg_color #ffffff;\n\
             @define-color slider_border_color alpha(currentColor, 0.15);\n\
             button.suggested-action {{ background-color: @accent_bg_color; color: @accent_fg_color; }}\n",
            self.accent_color,
            self.accent_bg_color,
            self.accent_fg_color,
            self.window_bg_color,
            self.window_fg_color,
            self.view_bg_color,
            self.view_fg_color,
            self.headerbar_bg_color,
            self.headerbar_fg_color,
            self.headerbar_bg_color,
            self.card_bg_color,
            self.card_fg_color,
            self.popover_bg_color,
            self.popover_fg_color,
            self.dialog_bg_color,
            self.dialog_fg_color,
            self.sidebar_bg_color,
            self.sidebar_fg_color,
            self.borders,
            self.shade_color,
        )
    }
}

fn soft_surfaces_css(is_dark: bool) -> &'static str {
    if is_dark {
        "\n/* Bespoke Soft Surfaces (Dark Mode) */\n\
         .player-bar {\n\
             background-color: alpha(mix(@headerbar_bg_color, @view_bg_color, 0.35), 0.84);\n\
             background-image: linear-gradient(180deg, rgba(255, 255, 255, 0.05) 0%, transparent 70%);\n\
             border-top: 1px solid rgba(255, 255, 255, 0.08);\n\
             transition: background-color 200ms ease;\n\
         }\n\
         .sidebar {\n\
             background-color: alpha(mix(@window_bg_color, @view_bg_color, 0.30), 0.82);\n\
             border-right: 1px solid rgba(255, 255, 255, 0.06);\n\
             transition: background-color 200ms ease;\n\
         }\n\
         .master-split-view > .sidebar,\n\
         .master-split-view .sidebar,\n\
         .master-split-view .sidebar-pane {\n\
             background-color: transparent !important;\n\
             background-image: none !important;\n\
             box-shadow: none !important;\n\
             border-right: none !important;\n\
         }\n\
         .sidebar-search {\n\
             background-color: alpha(currentColor, 0.055);\n\
             border: 1px solid alpha(currentColor, 0.06);\n\
         }\n\
         .utility-pane {\n\
             background-color: alpha(mix(@window_bg_color, @view_bg_color, 0.30), 0.82);\n\
             border-left: 1px solid rgba(255, 255, 255, 0.06);\n\
             transition: background-color 200ms ease;\n\
         }\n\
         .malus-window headerbar {\n\
             background-color: alpha(@headerbar_bg_color, 0.84);\n\
             background-image: linear-gradient(180deg, rgba(255, 255, 255, 0.05) 0%, transparent 80%);\n\
             border-bottom: 1px solid rgba(255, 255, 255, 0.06);\n\
             transition: background-color 200ms ease;\n\
         }\n\
         .malus-window headerbar button:hover {\n\
             background-color: alpha(currentColor, 0.08);\n\
         }\n\
         popover contents {\n\
             border-radius: 14px;\n\
             background-color: alpha(@popover_bg_color, 0.92);\n\
             border: 1px solid rgba(255, 255, 255, 0.08);\n\
         }\n"
    } else {
        "\n/* Bespoke Soft Surfaces (Light Mode) */\n\
         .player-bar {\n\
             background-color: alpha(mix(@headerbar_bg_color, @view_bg_color, 0.35), 0.88);\n\
             background-image: linear-gradient(180deg, rgba(255, 255, 255, 0.50) 0%, transparent 70%);\n\
             border-top: 1px solid rgba(0, 0, 0, 0.08);\n\
             transition: background-color 200ms ease;\n\
         }\n\
         .sidebar {\n\
             background-color: alpha(mix(@window_bg_color, @view_bg_color, 0.30), 0.86);\n\
             border-right: 1px solid rgba(0, 0, 0, 0.07);\n\
             transition: background-color 200ms ease;\n\
         }\n\
         .master-split-view > .sidebar,\n\
         .master-split-view .sidebar,\n\
         .master-split-view .sidebar-pane {\n\
             background-color: transparent !important;\n\
             background-image: none !important;\n\
             box-shadow: none !important;\n\
             border-right: none !important;\n\
         }\n\
         .sidebar-search {\n\
             background-color: alpha(currentColor, 0.045);\n\
             border: 1px solid alpha(currentColor, 0.06);\n\
         }\n\
         .utility-pane {\n\
             background-color: alpha(mix(@window_bg_color, @view_bg_color, 0.30), 0.86);\n\
             border-left: 1px solid rgba(0, 0, 0, 0.07);\n\
             transition: background-color 200ms ease;\n\
         }\n\
         .malus-window headerbar {\n\
             background-color: alpha(@headerbar_bg_color, 0.88);\n\
             background-image: linear-gradient(180deg, rgba(255, 255, 255, 0.55) 0%, transparent 80%);\n\
             border-bottom: 1px solid rgba(0, 0, 0, 0.07);\n\
             transition: background-color 200ms ease;\n\
         }\n\
         .malus-window headerbar button:hover {\n\
             background-color: alpha(currentColor, 0.07);\n\
         }\n\
         popover contents {\n\
             border-radius: 14px;\n\
             background-color: alpha(@popover_bg_color, 0.95);\n\
             border: 1px solid rgba(0, 0, 0, 0.08);\n\
         }\n"
    }
}

fn apply_palette_and_rules(is_dark: bool, accent_idx: u32, corner_idx: u32, translucent: bool) {
    let mut css = String::new();

    // 1. Palette tokens (only injected if a fruit theme 1..=7 is chosen; System Default injects nothing)
    if let Some(palette) = FruitPalette::for_fruit(accent_idx, is_dark) {
        css.push_str(&palette.to_css());
    }

    // 2. Corner curvature (independent from palette selection)
    let radius = match corner_idx {
        0 => "8px",
        1 => "16px",
        2 => "2px",
        _ => "8px",
    };
    css.push_str(&format!(
        ".card-artwork, .track-artwork, .player-artwork {{ border-radius: {radius}; }}\n"
    ));

    // 3. Bespoke soft surfaces (independent from palette selection)
    if translucent {
        css.push_str(soft_surfaces_css(is_dark));
    }

    with_css_provider(|provider| {
        provider.load_from_string(&css);
    });
}

/// Applies theme, accent color, corner curvature, and translucency.
pub fn apply_theme_settings(
    color_scheme_idx: u32,
    accent_idx: u32,
    corner_idx: u32,
    translucent: bool,
) {
    // 1. Color scheme (Follow System / Dark / Light)
    let manager = adw::StyleManager::default();
    match color_scheme_idx {
        0 => manager.set_color_scheme(adw::ColorScheme::Default),
        1 => manager.set_color_scheme(adw::ColorScheme::ForceDark),
        2 => manager.set_color_scheme(adw::ColorScheme::ForceLight),
        _ => {}
    }

    // Register the system dark-mode notification listener once
    ensure_dark_mode_listener(&manager);

    let is_dark = match color_scheme_idx {
        1 => true,
        2 => false,
        _ => manager.is_dark(),
    };

    apply_palette_and_rules(is_dark, accent_idx, corner_idx, translucent);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub scheme: u32,
    pub accent: u32,
    pub corners: u32,
    pub translucent: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            scheme: 0,
            accent: 0, // 0 = System Default
            corners: 1,
            translucent: false,
        }
    }
}

impl Appearance {
    fn path() -> Option<std::path::PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".config"))
            })
            .map(|p| p.join("malus/appearance.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .map(|mut value| {
                value.scheme = value.scheme.min(2);
                value.accent = value.accent.min(7);
                value.corners = value.corners.min(2);
                value
            })
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Err(std::io::Error::other(
                "No configuration directory is available",
            ));
        };
        std::fs::create_dir_all(path.parent().expect("appearance has a parent directory"))?;
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(temp, path)
    }

    pub fn apply(&self) {
        apply_theme_settings(self.scheme, self.accent, self.corners, self.translucent);
    }
}
