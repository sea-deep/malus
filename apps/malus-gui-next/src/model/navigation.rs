//! Pure navigation routing and history model.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Home,
    Search,
    Albums,
    Songs,
    Playlists,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationHistory {
    current: Route,
    history: Vec<Route>,
    forward_history: Vec<Route>,
}

impl Default for NavigationHistory {
    fn default() -> Self {
        Self {
            current: Route::Home,
            history: Vec::new(),
            forward_history: Vec::new(),
        }
    }
}

impl NavigationHistory {
    pub fn new(initial: Route) -> Self {
        Self {
            current: initial,
            history: Vec::new(),
            forward_history: Vec::new(),
        }
    }

    pub fn current(&self) -> &Route {
        &self.current
    }

    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward_history.is_empty()
    }

    pub fn navigate_to(&mut self, route: Route) {
        if self.current != route {
            self.history.push(self.current.clone());
            self.forward_history.clear();
            self.current = route;
        }
    }

    pub fn go_back(&mut self) -> Option<&Route> {
        if let Some(prev) = self.history.pop() {
            self.forward_history.push(self.current.clone());
            self.current = prev;
            Some(&self.current)
        } else {
            None
        }
    }

    pub fn go_forward(&mut self) -> Option<&Route> {
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
    fn test_navigation_history_flow() {
        let mut nav = NavigationHistory::default();
        assert_eq!(*nav.current(), Route::Home);
        assert!(!nav.can_go_back());
        assert!(!nav.can_go_forward());

        nav.navigate_to(Route::Albums);
        assert_eq!(*nav.current(), Route::Albums);
        assert!(nav.can_go_back());

        nav.navigate_to(Route::Search);
        assert_eq!(*nav.current(), Route::Search);

        assert_eq!(nav.go_back(), Some(&Route::Albums));
        assert_eq!(nav.go_back(), Some(&Route::Home));
        assert_eq!(nav.go_back(), None);

        assert!(nav.can_go_forward());
        assert_eq!(nav.go_forward(), Some(&Route::Albums));
    }
}
