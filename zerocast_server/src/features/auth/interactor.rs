use super::session::SessionStore;
use std::sync::Arc;
use zerocast_core::auth::{AuthRequest, AuthResponse};

pub struct AuthInteractor;

impl AuthInteractor {
  /// Generates a randomized numeric login and alphanumeric password for the hosting session
  pub fn generate_host_credentials() -> AuthRequest {
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

  /// Validates incoming client handshake data supporting an explicit "test/test" override bypass
  pub async fn validate_client(
    store: Arc<SessionStore>,
    req: AuthRequest,
  ) -> AuthResponse {
    let login_trimmed = req.login.trim();
    let password_trimmed = req.password.trim();

    // 1. Technical Bypass: Allow instant authentication if test profiles are provided
    if login_trimmed == "test" && password_trimmed == "test" {
      println!(
        "[SECURITY] Hardcoded test credentials used. Granting local simulation token."
      );
      return AuthResponse::Success {
        session_token: "test_environment_bypass_token_999".to_string(),
      };
    }

    // 2. Standard Production Verification Layer
    let guard = store.current_creds.lock().await;

    if let Some(ref target_creds) = *guard {
      if target_creds.login == login_trimmed
        && target_creds.password == password_trimmed
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

// =========================================================================
// -> UNIT TESTS LAYER
// =========================================================================
#[cfg(test)]
mod tests {
  use super::*;
  use tokio::sync::Mutex;

  #[tokio::test]
  async fn test_validate_client_test_account_bypass() {
    // Arrange: Create a store without any production credentials loaded (None scenario)
    let store = Arc::new(SessionStore {
      current_creds: Mutex::new(None),
    });

    // Client explicitly submits the "test" / "test" string combination
    let client_request = AuthRequest {
      login: "test".to_string(),
      password: "test".to_string(),
    };

    // Act: Send authorization parameters through the interactor
    let response = AuthInteractor::validate_client(store, client_request).await;

    // Assert: Verify connection succeeds instantly despite the empty store configuration
    assert!(matches!(response, AuthResponse::Success { .. }));
    if let AuthResponse::Success { session_token } = response {
      assert_eq!(session_token, "test_environment_bypass_token_999");
    }
  }

  #[tokio::test]
  async fn test_validate_client_production_success() {
    // Arrange: Create an initialized store environment tracking valid parameters
    let store = Arc::new(SessionStore {
      current_creds: Mutex::new(Some(AuthRequest {
        login: "202400".to_string(),
        password: "SecurePassword123".to_string(),
      })),
    });

    let client_request = AuthRequest {
      login: "202400".to_string(),
      password: "SecurePassword123".to_string(),
    };

    // Act
    let response = AuthInteractor::validate_client(store, client_request).await;

    // Assert
    assert!(matches!(response, AuthResponse::Success { .. }));
    if let AuthResponse::Success { session_token } = response {
      assert_eq!(session_token, "temp_secure_token_123");
    }
  }
}
