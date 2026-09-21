//! Browser launch mode definitions for WPE WebKit.

/// Controls whether the browser window is visible, headless, or windowless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchMode {
    /// Normal headed browser window.
    #[default]
    Headed,
    /// Headless mode (suitable for background runtime, automation, and headless servers).
    Headless,
    /// Headed browser without initial window.
    Windowless,
}
