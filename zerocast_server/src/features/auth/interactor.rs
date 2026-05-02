use crate::features::auth::{session::SessionStore, *};
use std::sync::Arc;
use zerocast_core::auth::*;

pub struct AuthInteractor;

impl AuthInteractor {
  pub fn generate_host_credentials() -> AuthCredentials {
    AuthCredentials {
      login: crypto::generate_numeric_code(6),
      password: crypto::generate_random_string(10),
    }
  }

  pub async fn validate_client(
    store: Arc<SessionStore>,
    request: AuthRequest,
  ) -> AuthResponse {
    let guard = store.current_creds.lock().await;

    if let Some(ref server_creds) = *guard {
      if server_creds.login == request.login
        && server_creds.password == request.password
      {
        return AuthResponse::Success {
          session_token: "temp_secure_token_123".to_string(),
        };
      }
    }

    AuthResponse::Failure {
      reason: "Invalid credentials".to_string(),
    }
  }
}
