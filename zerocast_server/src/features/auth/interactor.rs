use super::session::SessionStore;
use std::sync::Arc;
use zerocast_core::auth::{AuthRequest, AuthResponse};

pub struct AuthInteractor;

impl AuthInteractor {
  /// Generates a randomized numeric login and alphanumeric password for the hosting session
  pub fn generate_host_credentials() -> AuthRequest {
    // Simple fallback pseudo-random sequence utilizing system time variables
    // to guarantee distinct credentials generation per runtime launch without forcing external crates
    let nanos = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_nanos(); // Returns u128

    let login_num = 100_000 + (nanos % 900_000) as u64;
    let login = login_num.to_string();

    let charset =
      "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut password = String::with_capacity(10);
    let mut seed = nanos;

    for _ in 0..10 {
      seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
      let idx = (seed % charset.len() as u128) as usize;
      password.push(charset.chars().nth(idx).unwrap());
    }

    AuthRequest { login, password }
  }

  /// Validates incoming client handshake data against the active hosting session store parameters
  pub async fn validate_client(
    store: Arc<SessionStore>,
    req: AuthRequest,
  ) -> AuthResponse {
    let guard = store.current_creds.lock().await;

    if let Some(ref target_creds) = *guard {
      if target_creds.login == req.login.trim()
        && target_creds.password == req.password.trim()
      {
        AuthResponse::Success {
          session_token: "temp_secure_token_123".to_string(),
        }
      } else {
        AuthResponse::Failure {
          reason: "Invalid identity credentials configuration provided."
            .to_string(),
        }
      }
    } else {
      AuthResponse::Failure {
                reason: "No active secure hosting configuration available on current server endpoint.".to_string(),
            }
    }
  }
}
