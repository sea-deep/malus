//! Apple Music credentials and token provider abstraction.

use async_trait::async_trait;
use malus_web_runtime::ProfileManager;
use serde::{Deserialize, Serialize};
use std::{fmt, path::Path, sync::Arc};

use crate::{
    api::error::AppleApiError,
    error::AppleError,
    web::{APPLE_PROFILE_NAMESPACE, AppleWebSession},
};

/// Apple Music API credentials.
#[derive(Clone, Serialize, Deserialize)]
pub struct AppleCredentials {
    #[serde(alias = "devToken")]
    pub developer_token: String,
    #[serde(alias = "userToken")]
    pub music_user_token: String,
    #[serde(default = "default_storefront")]
    pub storefront: String,
}

fn default_storefront() -> String {
    "us".to_string()
}

impl AppleCredentials {
    pub fn new(
        developer_token: impl Into<String>,
        music_user_token: impl Into<String>,
        storefront: impl Into<String>,
    ) -> Self {
        let sf = storefront.into();
        Self {
            developer_token: developer_token.into(),
            music_user_token: music_user_token.into(),
            storefront: if sf.is_empty() { "us".to_string() } else { sf },
        }
    }

    /// Load credentials from a JSON file.
    pub fn load_from_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let creds: Self = serde_json::from_str(&content).ok()?;
        if !creds.developer_token.is_empty() && !creds.music_user_token.is_empty() {
            Some(creds)
        } else {
            None
        }
    }

    /// Save credentials to a JSON file, setting 0600 permissions on unix.
    pub fn save_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(path, perms);
        }
        Ok(())
    }
}

impl fmt::Debug for AppleCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppleCredentials")
            .field("developer_token", &"[REDACTED]")
            .field("music_user_token", &"[REDACTED]")
            .field("storefront", &self.storefront)
            .finish()
    }
}

/// Credential provider abstraction for Apple Music HTTP API operations.
#[async_trait]
pub trait TokenProvider: Send + Sync {
    /// Get current cached credentials.
    fn get_credentials(&self) -> Result<AppleCredentials, AppleApiError>;

    /// Force refresh credentials through the session runtime and return updated credentials.
    async fn refresh_credentials(&self) -> Result<AppleCredentials, AppleApiError>;
}

/// Production token provider backed by the managed Apple browser profile and AppleWebSession.
pub struct ProfileTokenProvider {
    session: Arc<dyn AppleWebSession>,
}

impl ProfileTokenProvider {
    pub fn new(session: Arc<dyn AppleWebSession>) -> Self {
        Self { session }
    }

    fn token_file_path() -> Result<std::path::PathBuf, AppleApiError> {
        let pm = ProfileManager::for_namespace(APPLE_PROFILE_NAMESPACE).map_err(|e| {
            AppleApiError::Other(format!("Failed to locate Apple profile directory: {e}"))
        })?;
        Ok(pm.profile_dir().join("tokens.json"))
    }
}

#[async_trait]
impl TokenProvider for ProfileTokenProvider {
    fn get_credentials(&self) -> Result<AppleCredentials, AppleApiError> {
        let path = Self::token_file_path()?;
        AppleCredentials::load_from_file(&path).ok_or_else(|| {
            AppleApiError::AuthRequired("No Apple Music credentials cached".to_string())
        })
    }

    async fn refresh_credentials(&self) -> Result<AppleCredentials, AppleApiError> {
        tracing::info!("Refreshing Apple Music credentials via session runtime...");
        let creds = self.session.refresh_tokens().await.map_err(|e| match e {
            AppleError::NotAuthorized => {
                AppleApiError::AuthRequired("Apple Music session is not authorized".to_string())
            }
            other => AppleApiError::AuthRequired(format!("Token refresh failed: {other}")),
        })?;

        // Re-read or return the freshly persisted credentials
        let path = Self::token_file_path()?;
        if let Some(loaded) = AppleCredentials::load_from_file(&path) {
            Ok(loaded)
        } else {
            Ok(creds)
        }
    }
}

/// Static or in-memory token provider for tests.
pub struct StaticTokenProvider {
    credentials: std::sync::RwLock<AppleCredentials>,
    next_credentials: std::sync::RwLock<Option<AppleCredentials>>,
    refresh_counter: Arc<std::sync::atomic::AtomicUsize>,
}

impl StaticTokenProvider {
    pub fn new(credentials: AppleCredentials) -> Self {
        Self {
            credentials: std::sync::RwLock::new(credentials),
            next_credentials: std::sync::RwLock::new(None),
            refresh_counter: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    pub fn with_refresh_counter(
        credentials: AppleCredentials,
        counter: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self {
            credentials: std::sync::RwLock::new(credentials),
            next_credentials: std::sync::RwLock::new(None),
            refresh_counter: counter,
        }
    }

    pub fn set_credentials(&self, credentials: AppleCredentials) {
        if let Ok(mut lock) = self.credentials.write() {
            *lock = credentials;
        }
    }

    pub fn set_next_credentials(&self, credentials: AppleCredentials) {
        if let Ok(mut lock) = self.next_credentials.write() {
            *lock = Some(credentials);
        }
    }

    pub fn refresh_count(&self) -> usize {
        self.refresh_counter
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl TokenProvider for StaticTokenProvider {
    fn get_credentials(&self) -> Result<AppleCredentials, AppleApiError> {
        let creds = self
            .credentials
            .read()
            .map_err(|e| AppleApiError::Other(format!("Lock poisoned: {e}")))?
            .clone();
        if creds.developer_token.is_empty() || creds.music_user_token.is_empty() {
            Err(AppleApiError::AuthRequired(
                "Missing test credentials".to_string(),
            ))
        } else {
            Ok(creds)
        }
    }

    async fn refresh_credentials(&self) -> Result<AppleCredentials, AppleApiError> {
        self.refresh_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut next_lock) = self.next_credentials.write()
            && let Some(next) = next_lock.take()
            && let Ok(mut creds_lock) = self.credentials.write()
        {
            *creds_lock = next;
        }

        let creds = self
            .credentials
            .read()
            .map_err(|e| AppleApiError::Other(format!("Lock poisoned: {e}")))?
            .clone();
        if creds.developer_token.is_empty() || creds.music_user_token.is_empty() {
            Err(AppleApiError::AuthRequired(
                "Test token refresh failed".to_string(),
            ))
        } else {
            Ok(creds)
        }
    }
}
