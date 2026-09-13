//! Authentication state model for Apple Music provider.

use malus_protocol::wire::{AuthStateWire, AuthStatusWire};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthState {
    #[default]
    Unknown,
    Checking,
    NeedsAuth,
    Authenticating,
    Authenticated,
    Failed,
}

impl From<AuthState> for AuthStateWire {
    fn from(state: AuthState) -> Self {
        match state {
            AuthState::Unknown => AuthStateWire::Unknown,
            AuthState::Checking => AuthStateWire::Checking,
            AuthState::NeedsAuth => AuthStateWire::NeedsAuth,
            AuthState::Authenticating => AuthStateWire::Authenticating,
            AuthState::Authenticated => AuthStateWire::Authenticated,
            AuthState::Failed => AuthStateWire::Failed,
        }
    }
}

impl From<AuthStateWire> for AuthState {
    fn from(wire: AuthStateWire) -> Self {
        match wire {
            AuthStateWire::Unknown => AuthState::Unknown,
            AuthStateWire::Checking => AuthState::Checking,
            AuthStateWire::NeedsAuth => AuthState::NeedsAuth,
            AuthStateWire::Authenticating => AuthState::Authenticating,
            AuthStateWire::Authenticated => AuthState::Authenticated,
            AuthStateWire::Failed => AuthState::Failed,
        }
    }
}

impl AuthState {
    pub fn to_status(self, provider: &str, message: Option<String>) -> AuthStatusWire {
        AuthStatusWire {
            provider: provider.to_string(),
            state: self.into(),
            message,
        }
    }
}
