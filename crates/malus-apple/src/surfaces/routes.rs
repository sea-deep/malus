//! Apple page route re-exports and aliases.

pub use malus_protocol::wire::ApplePageRoute;

/// Backwards compatibility alias for ApplePageRoute.
pub type AppleSurfaceRoute = ApplePageRoute;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_roundtrip() {
        assert_eq!(
            ApplePageRoute::parse("apple:page:home"),
            Some(ApplePageRoute::Home)
        );
        assert_eq!(
            ApplePageRoute::parse("apple:surface:home"),
            Some(ApplePageRoute::Home)
        );
        assert_eq!(ApplePageRoute::parse("home"), Some(ApplePageRoute::Home));
        assert_eq!(
            ApplePageRoute::parse("apple:page:album:1440857780"),
            Some(ApplePageRoute::Album("1440857780".to_string()))
        );
    }
}
