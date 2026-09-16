//! Pure navigation history and destination routing.

use malus_model::PageRoute;

/// Application destinations encompassing canonical Apple page routes,
/// Search queries and Settings. Now Playing is presentation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppDestination {
    Page(PageRoute),
    Search(String),
    Settings,
}

impl AppDestination {
    pub fn is_search(&self) -> bool {
        matches!(self, Self::Search(_))
    }

    pub fn page_route(&self) -> Option<&PageRoute> {
        match self {
            Self::Page(r) => Some(r),
            _ => None,
        }
    }
}

impl Default for AppDestination {
    fn default() -> Self {
        Self::Page(PageRoute::Home)
    }
}

/// Navigation history stack supporting back/forward transitions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NavigationHistory {
    current: AppDestination,
    history: Vec<AppDestination>,
    forward_history: Vec<AppDestination>,
}

impl NavigationHistory {
    pub fn new(initial: AppDestination) -> Self {
        Self {
            current: initial,
            history: Vec::new(),
            forward_history: Vec::new(),
        }
    }

    pub fn current(&self) -> &AppDestination {
        &self.current
    }

    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward_history.is_empty()
    }

    pub fn navigate_to(&mut self, destination: AppDestination) {
        if self.current.is_search() && destination.is_search() {
            if self.current != destination {
                self.forward_history.clear();
            }
            self.current = destination;
        } else if self.current != destination {
            self.history.push(self.current.clone());
            self.forward_history.clear();
            self.current = destination;
        }
    }

    pub fn go_back(&mut self) -> Option<&AppDestination> {
        if let Some(prev) = self.history.pop() {
            self.forward_history.push(self.current.clone());
            self.current = prev;
            Some(&self.current)
        } else {
            None
        }
    }

    pub fn go_forward(&mut self) -> Option<&AppDestination> {
        if let Some(next) = self.forward_history.pop() {
            self.history.push(self.current.clone());
            self.current = next;
            Some(&self.current)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_edits_are_one_history_entry_and_invalidate_forward() {
        let mut history = NavigationHistory::default();
        history.navigate_to(AppDestination::Search("d".into()));
        history.navigate_to(AppDestination::Search("daft punk".into()));
        assert_eq!(
            history.go_back(),
            Some(&AppDestination::Page(PageRoute::Home))
        );
        assert_eq!(
            history.go_forward(),
            Some(&AppDestination::Search("daft punk".into()))
        );
        history.navigate_to(AppDestination::Settings);
        history.go_back();
        history.navigate_to(AppDestination::Search("justice".into()));
        assert!(!history.can_go_forward());
    }

    #[test]
    fn test_navigation_history_flow() {
        let mut nav = NavigationHistory::default();
        assert_eq!(*nav.current(), AppDestination::Page(PageRoute::Home));
        assert!(!nav.can_go_back());
        assert!(!nav.can_go_forward());

        nav.navigate_to(AppDestination::Page(PageRoute::New));
        assert_eq!(*nav.current(), AppDestination::Page(PageRoute::New));
        assert!(nav.can_go_back());
        assert!(!nav.can_go_forward());

        nav.navigate_to(AppDestination::Page(PageRoute::Radio));
        assert_eq!(*nav.current(), AppDestination::Page(PageRoute::Radio));
        assert_eq!(nav.history.len(), 2);

        // Back to New
        assert_eq!(nav.go_back(), Some(&AppDestination::Page(PageRoute::New)));
        assert!(nav.can_go_forward());

        // Back to Home
        assert_eq!(nav.go_back(), Some(&AppDestination::Page(PageRoute::Home)));
        assert!(!nav.can_go_back());
        assert!(nav.can_go_forward());

        // Forward to New
        assert_eq!(
            nav.go_forward(),
            Some(&AppDestination::Page(PageRoute::New))
        );
        assert!(nav.can_go_back());

        // Forward to Radio
        assert_eq!(
            nav.go_forward(),
            Some(&AppDestination::Page(PageRoute::Radio))
        );
        assert!(!nav.can_go_forward());
    }
}
