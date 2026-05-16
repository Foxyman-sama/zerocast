use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiMessage {
  AuthRequest(String, String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthResult {
  Pending,
  Success,
  Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEvent {
  AuthSuccess,
}
