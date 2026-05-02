use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthCredentials {
  pub login: String,
  pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthRequest {
  pub login: String,
  pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AuthResponse {
  Success { session_token: String },
  Failure { reason: String },
}
