use std::sync::Arc;
use tokio::sync::Mutex;
use zerocast_core::auth::AuthCredentials;

pub struct SessionStore {
  pub current_creds: Mutex<Option<AuthCredentials>>,
}

impl SessionStore {
  pub fn new() -> Self {
    Self {
      current_creds: Mutex::new(None),
    }
  }
}
