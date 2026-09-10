//! Two-tier artwork engine for Malus.
//!
//! Tier 1: Crisp native terminal raster via Kitty Graphics Protocol (Ghostty/Kitty/WezTerm).
//!         Written AFTER ratatui's draw cycle so it isn't clobbered by the buffer flush.
//! Tier 2: High-contrast bordered placeholder card — title, artist, ♫ glyph.
//!
//! Half-block `▀` pixelation is rejected entirely.

use crate::ui::widgets::fit;
use image::GenericImageView;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

// ─── Terminal capability detection ───────────────────────────────────────────

/// Check if the running terminal supports the Kitty Graphics Protocol.
pub fn supports_kitty_graphics() -> bool {
    if std::env::var("MALUS_NO_GRAPHICS").is_ok_and(|v| v == "1" || v == "true") {
        return false;
    }
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
        || std::env::var("TERM").is_ok_and(|t| t == "xterm-kitty")
        || std::env::var("TERM_PROGRAM").is_ok_and(|p| p == "ghostty" || p == "WezTerm")
}

// ─── Base64 ──────────────────────────────────────────────────────────────────

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

// ─── Kitty Graphics Protocol — called AFTER ratatui draw ─────────────────────

/// Emit Kitty Graphics Protocol sequences to paint a native image.
///
/// **Must be called after ratatui's draw completes** — ratatui would overwrite
/// pixel data written during its own render pass.
pub fn render_kitty_image(raw_bytes: &[u8], area: Rect, buf: &mut Buffer) {
    if area.is_empty() || raw_bytes.is_empty() {
        return;
    }
    // Clear buffer cells so ratatui won't paint over the graphic.
    for y in 0..area.height {
        for x in 0..area.width {
            if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
                cell.set_char(' ');
                cell.set_style(Style::default());
            }
        }
    }
    // The actual terminal write is done post-draw via `flush_kitty_image`.
}

/// Write Kitty APC sequence to stdout.  Call this *immediately after*
/// `ratatui::Terminal::draw` returns so we don't get overwritten.
pub fn flush_kitty_image(raw_bytes: &[u8], area: Rect) {
    use std::io::Write;
    // Move cursor to top-left of the artwork area (1-based)
    let mut stdout = std::io::stdout().lock();
    let _ = write!(stdout, "\x1b[{};{}H", area.y + 1, area.x + 1);
    let b64 = base64_encode(raw_bytes);
    let chunk_size = 4096;
    let total = b64.len().div_ceil(chunk_size);
    for (i, chunk) in b64.as_bytes().chunks(chunk_size).enumerate() {
        let m = if i + 1 == total { 0 } else { 1 };
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

// ─── Placeholder card ─────────────────────────────────────────────────────────

/// Card background: slightly lighter than the terminal bg to be distinguishable
/// even on transparent/blurred terminals.
const CARD_BG: Color = Color::Rgb(36, 38, 48);
/// Card border: brighter than default border so it's clearly visible
const CARD_BORDER: Color = Color::Rgb(85, 88, 110);
/// Accent for the ♫ glyph
const CARD_ACCENT: Color = Color::Rgb(250, 99, 126); // theme.primary

/// Render a clean, high-contrast typography card as artwork placeholder.
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

    // 1. Solid background fill — distinct from the terminal background
    for y in 0..area.height {
        for x in 0..area.width {
            if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
                cell.set_char(' ');
                cell.set_bg(CARD_BG);
            }
        }
    }

    // 2. Visible border using box-drawing characters
    let right = area.x + area.width.saturating_sub(1);
    let bottom = area.y + area.height.saturating_sub(1);
    let border_style = Style::default().fg(CARD_BORDER).bg(CARD_BG);

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
    for (x, y, ch) in [
        (area.x, area.y, '┌'),
        (right, area.y, '┐'),
        (area.x, bottom, '└'),
        (right, bottom, '┘'),
    ] {
        if let Some(c) = buf.cell_mut((x, y)) {
            c.set_char(ch).set_style(border_style);
        }
    }

    // 3. Inner content
    let inner_w = area.width.saturating_sub(4);
    let inner_x = area.x + 2;
    let _ = theme; // keep for callers that pass theme; we use const colours

    if area.height >= 8 {
        // ♫ glyph centred near top
        let glyph_y = area.y + 2;
        let glyph_x = area.x + area.width / 2;
        if let Some(c) = buf.cell_mut((glyph_x, glyph_y)) {
            c.set_char('♫')
                .set_fg(CARD_ACCENT)
                .set_bg(CARD_BG)
                .set_style(
                    Style::default()
                        .fg(CARD_ACCENT)
                        .bg(CARD_BG)
                        .add_modifier(Modifier::BOLD),
                );
        }

        let title_y = glyph_y + 2;
        if title_y < bottom && inner_w > 0 {
            buf.set_string(
                inner_x,
                title_y,
                fit(title, inner_w),
                Style::default()
                    .fg(Color::White)
                    .bg(CARD_BG)
                    .add_modifier(Modifier::BOLD),
            );
        }
        let artist_y = title_y + 1;
        if artist_y < bottom && inner_w > 0 {
            buf.set_string(
                inner_x,
                artist_y,
                fit(artist, inner_w),
                Style::default().fg(CARD_ACCENT).bg(CARD_BG),
            );
        }
        if let Some(alb) = album {
            let album_y = artist_y + 1;
            if album_y < bottom && inner_w > 0 {
                buf.set_string(
                    inner_x,
                    album_y,
                    fit(alb, inner_w),
                    Style::default().fg(CARD_BORDER).bg(CARD_BG),
                );
            }
        }
        // AAC badge at bottom
        let badge_y = bottom.saturating_sub(1);
        if badge_y > area.y + 4 && inner_w >= 9 {
            buf.set_string(
                inner_x,
                badge_y,
                "AAC 256",
                Style::default().fg(CARD_BORDER).bg(CARD_BG),
            );
        }
    } else if area.height >= 4 {
        let ty = area.y + 1;
        if ty < bottom && inner_w > 0 {
            buf.set_string(
                inner_x,
                ty,
                fit(title, inner_w),
                Style::default()
                    .fg(Color::White)
                    .bg(CARD_BG)
                    .add_modifier(Modifier::BOLD),
            );
        }
        let ay = ty + 1;
        if ay < bottom && inner_w > 0 {
            buf.set_string(
                inner_x,
                ay,
                fit(artist, inner_w),
                Style::default().fg(CARD_BORDER).bg(CARD_BG),
            );
        }
    } else if inner_w > 0 {
        buf.set_string(
            inner_x,
            area.y,
            fit(title, inner_w),
            Style::default().fg(Color::White).bg(CARD_BG),
        );
    }
}

// ─── ArtworkCard widget ───────────────────────────────────────────────────────

/// Combined widget: renders Kitty image if supported and cover is loaded,
/// otherwise renders the typography placeholder card.
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
        // Always render the placeholder card with metadata and border.
        // Kitty graphics (if available and loaded) will be drawn on top by the post-draw hook.
        // We never clear the cells to empty spaces so the UI never drops into a black hole.
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

// ─── Cover ───────────────────────────────────────────────────────────────────

/// Downloaded artwork resource with raw PNG bytes.
#[derive(Debug, Clone)]
pub struct Cover {
    pub width: u32,
    pub height: u32,
    pub raw_bytes: Vec<u8>,
}

impl Cover {
    pub async fn load(template: &str) -> Option<Self> {
        let url = template
            .replace("{w}", "400")
            .replace("{h}", "400")
            .replace("{f}", "jpg");
        let parsed = url::Url::parse(&url).ok()?;
        let host = parsed.host_str()?;
        if parsed.scheme() != "https"
            || (!host.ends_with(".mzstatic.com")
                && host != "mzstatic.com"
                && !host.ends_with(".apple.com")
                && host != "apple.com")
        {
            return None;
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .ok()?;
        let response = client
            .get(parsed)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let bytes = response.bytes().await.ok()?;
        if bytes.len() > 5_000_000 {
            return None;
        }
        let img = image::load_from_memory(&bytes).ok()?;
        let (width, height) = img.dimensions();
        // Convert to verified PNG format so Kitty Graphics Protocol decoding always succeeds
        let mut png_bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .ok()?;
        Some(Self {
            width,
            height,
            raw_bytes: png_bytes,
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
            // Box outline fallback
            let right = area.x + area.width.saturating_sub(1);
            let bottom = area.y + area.height.saturating_sub(1);
            let s = Style::default().fg(CARD_BORDER).bg(CARD_BG);
            for x in area.x..=right {
                if let Some(c) = buf.cell_mut((x, area.y)) {
                    c.set_char('─').set_style(s);
                }
                if let Some(c) = buf.cell_mut((x, bottom)) {
                    c.set_char('─').set_style(s);
                }
            }
            for y in area.y..=bottom {
                if let Some(c) = buf.cell_mut((area.x, y)) {
                    c.set_char('│').set_style(s);
                }
                if let Some(c) = buf.cell_mut((right, y)) {
                    c.set_char('│').set_style(s);
                }
            }
            for (x, y, ch) in [
                (area.x, area.y, '┌'),
                (right, area.y, '┐'),
                (area.x, bottom, '└'),
                (right, bottom, '┘'),
            ] {
                if let Some(c) = buf.cell_mut((x, y)) {
                    c.set_char(ch).set_style(s);
                }
            }
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

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
        assert_eq!(buf.cell((0, 0)).unwrap().symbol(), "┌");
        assert_eq!(buf.cell((29, 0)).unwrap().symbol(), "┐");
        assert_eq!(buf.cell((0, 9)).unwrap().symbol(), "└");
        assert_eq!(buf.cell((29, 9)).unwrap().symbol(), "┘");
    }
}
