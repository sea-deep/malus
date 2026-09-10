//! Small, bounded cover images rendered with terminal half blocks.
use image::GenericImageView;
use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};
#[derive(Debug, Clone)]
pub struct Cover {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 3]>,
}
impl Cover {
    pub async fn load(template: &str) -> Option<Self> {
        let url = template
            .replace("{w}", "160")
            .replace("{h}", "160")
            .replace("{f}", "jpg");
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
        let img = image::load_from_memory(&bytes).ok()?.thumbnail(96, 96);
        let (width, height) = img.dimensions();
        Some(Self {
            width,
            height,
            pixels: img.to_rgb8().pixels().map(|p| p.0).collect(),
        })
    }
}
impl Widget for Cover {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let sample = |x: u16, y: u16| {
            let sx = u32::from(x) * self.width / u32::from(area.width);
            let sy = u32::from(y) * self.height / (u32::from(area.height) * 2);
            let [r, g, b] = self.pixels[(sy * self.width + sx) as usize];
            Color::Rgb(r, g, b)
        };
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
                    cell.set_char('▀')
                        .set_fg(sample(x, y * 2))
                        .set_bg(sample(x, y * 2 + 1));
                }
            }
        }
    }
}
