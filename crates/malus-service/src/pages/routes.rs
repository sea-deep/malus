//! Page route definitions and re-exports.

pub use malus_ipc::wire::PageRoute;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_roundtrip() {
        assert_eq!(PageRoute::parse("home"), Some(PageRoute::Home));
        assert_eq!(
            PageRoute::parse("album:1440857780"),
            Some(PageRoute::Album("1440857780".to_string()))
        );
        assert_eq!(
            PageRoute::parse("replay:2024"),
            Some(PageRoute::Replay(2024))
        );
    }
}
