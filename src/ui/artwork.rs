//! Two-tier artwork engine for Malus.
//!
//! Tier 1: Crisp native terminal raster rendering via Kitty Graphics Protocol when supported.
//! Tier 2: Clean, high-contrast typography placeholder card with metadata and audio traits.
//!
//! Strictly rejects muddy half-block (`▀`) pixelation.

use crate::ui::widgets::fit;
use image::GenericImageView;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

/// Check if the terminal emulator supports the Kitty Graphics Protocol.
pub fn supports_kitty_graphics() -> bool {
    if std::env::var("MALUS_NO_GRAPHICS").is_ok_and(|v| v == "1" || v == "true") {
        return false;
    }
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
        || std::env::var("TERM").is_ok_and(|t| t == "xterm-kitty")
        || std::env::var("TERM_PROGRAM").is_ok_and(|p| p == "ghostty" || p == "WezTerm")
}

/// Base64 encoder for terminal graphic escape payloads.
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Emit Kitty Graphics Protocol sequences to render a native image in the designated area.
pub fn render_kitty_image(raw_bytes: &[u8], area: Rect, buf: &mut Buffer) {
    if area.is_empty() || raw_bytes.is_empty() {
        return;
    }

    // Clear buffer cells so terminal graphics can shine through
    for y in 0..area.height {
        for x in 0..area.width {
            if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
                cell.set_char(' ');
                cell.set_style(Style::default());
            }
        }
    }

    let b64 = base64_encode(raw_bytes);
    if b64.is_empty() {
        return;
    }

    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    let chunk_size = 4096;
    let total_chunks = b64.len().div_ceil(chunk_size);

    for (i, chunk) in b64.as_bytes().chunks(chunk_size).enumerate() {
        let is_last = i + 1 == total_chunks;
        let m = if is_last { 0 } else { 1 };
        if i == 0 {
            let _ = write!(
                stdout,
                "\x1b_Ga=T,f=100,c={},r={},q=2,m={};{}\x1b\\",
                area.width,
                area.height,
                m,
                std::str::from_utf8(chunk).unwrap_or("")
            );
        } else {
            let _ = write!(
                stdout,
                "\x1b_Gm={};{}\x1b\\",
                m,
                std::str::from_utf8(chunk).unwrap_or("")
            );
        }
    }
    let _ = stdout.flush();
}

/// Render a clean, crisp typography card with track and album metadata.
pub fn render_placeholder_card(
    area: Rect,
    buf: &mut Buffer,
    title: &str,
    artist: &str,
    album: Option<&str>,
    theme: &ratcn::Theme,
) {
    if area.is_empty() {
        return;
    }

    // 1. Fill background with theme.secondary
    for y in 0..area.height {
        for x in 0..area.width {
            if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
                cell.set_char(' ');
                cell.set_bg(theme.secondary);
            }
        }
    }

    // 2. Draw clean single-cell card border
    let right = area.x + area.width.saturating_sub(1);
    let bottom = area.y + area.height.saturating_sub(1);
    let border_style = Style::default().fg(theme.border).bg(theme.secondary);

    for x in area.x..=right {
        if let Some(c) = buf.cell_mut((x, area.y)) {
            c.set_char('─').set_style(border_style);
        }
        if let Some(c) = buf.cell_mut((x, bottom)) {
            c.set_char('─').set_style(border_style);
        }
    }
    for y in area.y..=bottom {
        if let Some(c) = buf.cell_mut((area.x, y)) {
            c.set_char('│').set_style(border_style);
        }
        if let Some(c) = buf.cell_mut((right, y)) {
            c.set_char('│').set_style(border_style);
        }
    }
    if let Some(c) = buf.cell_mut((area.x, area.y)) {
        c.set_char('┌').set_style(border_style);
    }
    if let Some(c) = buf.cell_mut((right, area.y)) {
        c.set_char('┐').set_style(border_style);
    }
    if let Some(c) = buf.cell_mut((area.x, bottom)) {
        c.set_char('└').set_style(border_style);
    }
    if let Some(c) = buf.cell_mut((right, bottom)) {
        c.set_char('┘').set_style(border_style);
    }

    // 3. Render typography within inner rectangle
    let inner_w = area.width.saturating_sub(4);
    let inner_x = area.x + 2;

    if area.height >= 8 {
        // Glyphs and metadata
        let glyph_y = area.y + 2;
        let glyph_x = area.x + (area.width.saturating_sub(1)) / 2;
        if let Some(c) = buf.cell_mut((glyph_x, glyph_y)) {
            c.set_char('♫').set_style(
                Style::default()
                    .fg(theme.primary)
                    .bg(theme.secondary)
                    .add_modifier(Modifier::BOLD),
            );
        }

        let title_y = glyph_y + 2;
        if title_y < bottom && inner_w > 0 {
            let title_str = fit(title, inner_w);
            buf.set_string(
                inner_x,
                title_y,
                &title_str,
                Style::default()
                    .fg(theme.foreground)
                    .bg(theme.secondary)
                    .add_modifier(Modifier::BOLD),
            );
        }

        let artist_y = title_y + 1;
        if artist_y < bottom && inner_w > 0 {
            let artist_str = fit(artist, inner_w);
            buf.set_string(
                inner_x,
                artist_y,
                &artist_str,
                Style::default().fg(theme.primary).bg(theme.secondary),
            );
        }

        let album_y = artist_y + 1;
        if let Some(alb) = album
            && album_y < bottom
            && inner_w > 0
        {
            let alb_str = fit(alb, inner_w);
            buf.set_string(
                inner_x,
                album_y,
                &alb_str,
                Style::default()
                    .fg(theme.muted_foreground)
                    .bg(theme.secondary),
            );
        }

        let badge_y = bottom.saturating_sub(1);
        if badge_y > album_y && inner_w >= 9 {
            buf.set_string(
                inner_x,
                badge_y,
                "[AAC 256]",
                Style::default()
                    .fg(theme.muted_foreground)
                    .bg(theme.secondary),
            );
        }
    } else if area.height >= 4 {
        let line1_y = area.y + 1;
        if line1_y < bottom && inner_w > 0 {
            let t_fit = fit(title, inner_w);
            buf.set_string(
                inner_x,
                line1_y,
                &t_fit,
                Style::default()
                    .fg(theme.foreground)
                    .bg(theme.secondary)
                    .add_modifier(Modifier::BOLD),
            );
        }
        let line2_y = line1_y + 1;
        if line2_y < bottom && inner_w > 0 {
            let a_fit = fit(artist, inner_w);
            buf.set_string(
                inner_x,
                line2_y,
                &a_fit,
                Style::default()
                    .fg(theme.muted_foreground)
                    .bg(theme.secondary),
            );
        }
    } else if inner_w > 0 {
        buf.set_string(
            inner_x,
            area.y,
            fit(title, inner_w),
            Style::default().fg(theme.foreground).bg(theme.secondary),
        );
    }
}

/// Component card widget combining native protocol image rendering with typography fallback.
#[derive(Debug, Clone)]
pub struct ArtworkCard {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub theme: ratcn::Theme,
    pub cover: Option<Cover>,
}

impl Widget for ArtworkCard {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        if supports_kitty_graphics()
            && let Some(cover) = &self.cover
        {
            render_kitty_image(&cover.raw_bytes, area, buf);
            return;
        }
        render_placeholder_card(
            area,
            buf,
            &self.title,
            &self.artist,
            self.album.as_deref(),
            &self.theme,
        );
    }
}

/// Downloaded artwork resource with raw bytes and dimensions.
#[derive(Debug, Clone)]
pub struct Cover {
    pub width: u32,
    pub height: u32,
    pub raw_bytes: Vec<u8>,
}

impl Cover {
    pub async fn load(template: &str) -> Option<Self> {
        let url = template
            .replace("{w}", "300")
            .replace("{h}", "300")
            .replace("{f}", "png");
        let parsed = url::Url::parse(&url).ok()?;
        if parsed.scheme() != "https" || !parsed.host_str()?.ends_with(".mzstatic.com") {
            return None;
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .ok()?;
        let mut response = client
            .get(parsed)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if bytes.len() + chunk.len() > 2_000_000 {
                return None;
            }
            bytes.extend_from_slice(&chunk);
        }
        let img = image::load_from_memory(&bytes).ok()?;
        let (width, height) = img.dimensions();
        Some(Self {
            width,
            height,
            raw_bytes: bytes,
        })
    }
}

impl Widget for Cover {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        if supports_kitty_graphics() {
            render_kitty_image(&self.raw_bytes, area, buf);
        } else {
            // Clean single-cell box fallback without muddy half-blocks
            let right = area.x + area.width.saturating_sub(1);
            let bottom = area.y + area.height.saturating_sub(1);
            for x in area.x..=right {
                if let Some(c) = buf.cell_mut((x, area.y)) {
                    c.set_char('─');
                }
                if let Some(c) = buf.cell_mut((x, bottom)) {
                    c.set_char('─');
                }
            }
            for y in area.y..=bottom {
                if let Some(c) = buf.cell_mut((area.x, y)) {
                    c.set_char('│');
                }
                if let Some(c) = buf.cell_mut((right, y)) {
                    c.set_char('│');
                }
            }
            if let Some(c) = buf.cell_mut((area.x, area.y)) {
                c.set_char('┌');
            }
            if let Some(c) = buf.cell_mut((right, area.y)) {
                c.set_char('┐');
            }
            if let Some(c) = buf.cell_mut((area.x, bottom)) {
                c.set_char('└');
            }
            if let Some(c) = buf.cell_mut((right, bottom)) {
                c.set_char('┘');
            }
            let glyph_y = area.y + area.height / 2;
            let glyph_x = area.x + area.width / 2;
            if let Some(c) = buf.cell_mut((glyph_x, glyph_y)) {
                c.set_char('♫');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"a"), "YQ==");
    }

    #[test]
    fn test_render_placeholder_card_bounds() {
        let area = Rect::new(0, 0, 30, 10);
        let mut buf = Buffer::empty(area);
        let theme = ratcn::Theme::default_dark();

        render_placeholder_card(
            area,
            &mut buf,
            "Blinding Lights",
            "The Weeknd",
            Some("After Hours"),
            &theme,
        );

        // Check border corners
        assert_eq!(buf.cell((0, 0)).unwrap().symbol(), "┌");
        assert_eq!(buf.cell((29, 0)).unwrap().symbol(), "┐");
        assert_eq!(buf.cell((0, 9)).unwrap().symbol(), "└");
        assert_eq!(buf.cell((29, 9)).unwrap().symbol(), "┘");
    }
}
