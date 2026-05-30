use crate::shared::events::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use zerocast_core::auth::{AuthRequest, AuthResponse};

pub async fn run_auth_interactor(
  mut ui_rx: mpsc::Receiver<UiMessage>,
  system_tx: mpsc::Sender<SystemEvent>,
  auth_status: Arc<tokio::sync::Mutex<AuthResult>>,
) {
  while let Some(msg) = ui_rx.recv().await {
    match msg {
      UiMessage::AuthRequest(server_ip, login, password) => {
        {
          let mut status = auth_status.lock().await;
          *status = AuthResult::Pending;
        }

        let result = perform_server_auth(server_ip, login, password).await;

        let mut status = auth_status.lock().await;
        match result {
          Ok(_) => {
            *status = AuthResult::Success;
            let _ = system_tx.send(SystemEvent::AuthSuccess).await;
          }
          Err(err_msg) => {
            *status = AuthResult::Error(err_msg);
          }
        }
      }
    }
  }
}

async fn perform_server_auth(
  server_ip: String,
  login: String,
  password: String,
) -> Result<(), String> {
  if login.is_empty() || password.is_empty() {
    return Err("Login or password cannot be empty.".to_string());
  }
  if server_ip.is_empty() {
    return Err("Server IP address cannot be empty.".to_string());
  }

  // Define target connection socket address
  let target_address = format!("{}:8080", server_ip);

  // Wrap the connection future with an explicit 3-second timeout constraint
  let connection_result =
    timeout(Duration::from_secs(3), TcpStream::connect(target_address)).await;

  // Unpack the timeout wrapper layer safely
  let mut stream = match connection_result {
      Ok(Ok(connected_stream)) => connected_stream,
      Ok(Err(net_err)) => return Err(format!("Network connection failed: {}", net_err)),
      Err(_) => return Err("Connection timed out. Please check your Server IP or Windows Firewall settings.".to_string()),
  };

  let request = AuthRequest { login, password };
  let req_bytes = serde_json::to_vec(&request)
    .map_err(|e| format!("Serialization payload error: {}", e))?;

  stream
    .write_all(&req_bytes)
    .await
    .map_err(|e| format!("Failed to write to stream socket: {}", e))?;

  let mut buffer = [0; 1024];
  let n = stream
    .read(&mut buffer)
    .await
    .map_err(|e| format!("Failed to read from stream socket: {}", e))?;

  if n == 0 {
    return Err("Server closed the connection unexpectedly.".to_string());
  }

  let response: AuthResponse = serde_json::from_slice(&buffer[..n])
    .map_err(|e| format!("Deserialization response error: {}", e))?;

  match response {
    AuthResponse::Success { session_token } => {
      println!("Session token verified and acquired: {}", session_token);
      Ok(())
    }
    AuthResponse::Failure { reason } => Err(reason),
  }
}
