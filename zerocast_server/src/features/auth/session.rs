use tokio::sync::Mutex;
use zerocast_core::auth::AuthRequest;

/// Thread-safe in-memory storage for session security states
pub struct SessionStore {
  // Uses a Tokio async Mutex to protect credentials across connection tasks
  pub current_creds: Mutex<Option<AuthRequest>>,
}

impl SessionStore {
  /// Instantiates a empty session storage container
  pub fn new() -> Self {
    Self {
      current_creds: Mutex::new(None),
    }
  }
}
