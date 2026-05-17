use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use zerocast_core::auth::{AuthRequest, AuthResponse};

mod features;

use features::auth::interactor::AuthInteractor;
use features::auth::session::SessionStore;
use features::input::run_input_replication_service;
use features::media::run_media_pipeline;

#[tokio::main]
async fn main() {
  // 1. Data layer initialization and host credentials generation
  let store = Arc::new(SessionStore::new());
  let creds = AuthInteractor::generate_host_credentials();

  {
    let mut guard = store.current_creds.lock().await;
    *guard = Some(creds.clone());
  }

  println!("=====================================================");
  println!("     ZEROCAST SERVER HARDWARE ENGINE INITIALIZED     ");
  println!("     LOGIN: {} | PASSWORD: {}", creds.login, creds.password);
  println!("=====================================================");

  // Signal channel to pass the authorized client's IP into the media pipeline
  let (stream_signal_tx, mut stream_signal_rx) =
    mpsc::channel::<std::net::IpAddr>(1);

  // 2. Spawn modular control services using Tokio runtime executors
  let store_clone = Arc::clone(&store);
  tokio::spawn(async move {
    if let Err(e) = run_auth_service(store_clone, stream_signal_tx).await {
      eprintln!("Critical error in Auth Service: {}", e);
    }
  });

  tokio::spawn(async move {
    if let Err(e) = run_input_replication_service().await {
      eprintln!("Critical error in Input Service: {}", e);
    }
  });

  println!(
    "Waiting for an authorized client connection to initialize streaming..."
  );

  // Block main execution until a valid client completes the handshake loop
  let client_ip = stream_signal_rx
    .recv()
    .await
    .expect("Stream coordinator signal channel broke down unexpectedly");

  // 3. Dispatch real-time media streaming loop onto a dedicated operating system thread
  std::thread::spawn(move || {
    if let Err(e) = run_media_pipeline(client_ip) {
      eprintln!("Media pipeline execution failure: {}", e);
    }
  });

  // Keep system active until explicit OS termination sequence occurs (Ctrl+C)
  tokio::signal::ctrl_c().await.unwrap();
  println!("\nServer shutting down safely. Disposing VRAM pipelines...");
}

/// Asynchronous service handling authentication requests (TCP Port 8080)
async fn run_auth_service(
  store: Arc<SessionStore>,
  signal_tx: mpsc::Sender<std::net::IpAddr>,
) -> Result<(), Box<dyn std::error::Error>> {
  let listener = TcpListener::bind("0.0.0.0:8080").await?;
  println!("[AUTH] Service successfully bound to port 8080");

  loop {
    let (mut socket, peer_addr) = listener.accept().await?;
    let store_for_task = Arc::clone(&store);
    let signal_tx_clone = signal_tx.clone();

    tokio::spawn(async move {
      let mut buffer = [0; 1024];
      if let Ok(n) = socket.read(&mut buffer).await {
        if n == 0 {
          return;
        }

        if let Ok(req) = serde_json::from_slice::<AuthRequest>(&buffer[..n]) {
          let response =
            AuthInteractor::validate_client(store_for_task, req).await;
          let is_success = matches!(response, AuthResponse::Success { .. });

          if let Ok(resp_bytes) = serde_json::to_vec(&response) {
            let _ = socket.write_all(&resp_bytes).await;
          }

          if is_success {
            println!(
              "[AUTH] Client authorized successfully from: {}",
              peer_addr
            );
            let _ = signal_tx_clone.send(peer_addr.ip()).await;
          } else {
            println!(
              "[AUTH] Unauthorized connection attempt rejected from: {}",
              peer_addr
            );
          }
        }
      }
    });
  }
}
