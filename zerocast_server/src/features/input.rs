use enigo::{Coordinate, Enigo, Mouse, Settings};
use native_tls::Identity;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_native_tls::TlsAcceptor;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RemoteInput {
  MouseMove { x: f32, y: f32 },
  MouseDown { button: String },
  MouseUp { button: String },
  Ping { client_time: u64 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerResponse {
  Pong { client_time: u64 },
}

/// Asynchronous service handling input replication and Ping-Pong latency measurement over TLS
pub async fn run_input_replication_service()
-> Result<(), Box<dyn std::error::Error>> {
  let listener = TcpListener::bind("0.0.0.0:8081").await?;
  println!("[INPUT] Secured Service successfully bound to port 8081");

  // 1. Initialize local cryptographic identity profile from PKCS#12 store
  // Resolves the absolute path relative to the crate's manifest location
  let manifest_dir =
    std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
  let cert_path = std::path::Path::new(&manifest_dir).join("identity.p12");

  println!(
    "[INPUT] Loading secure identity certificate from: {:?}",
    cert_path
  );

  let mut file = File::open(&cert_path).map_err(|e| {
    format!(
      "Missing certificate 'identity.p12' at {:?}: {}",
      cert_path, e
    )
  })?;

  let mut identity_bytes = Vec::new();
  file.read_to_end(&mut identity_bytes)?;

  let native_identity = Identity::from_pkcs12(&identity_bytes, "zerocast")?;
  let native_acceptor = native_tls::TlsAcceptor::new(native_identity)?;
  let acceptor = TlsAcceptor::from(native_acceptor);

  loop {
    let (socket, client_addr) = listener.accept().await?;
    let acceptor_clone = acceptor.clone();

    tokio::spawn(async move {
      println!(
        "[INPUT] Attempting TLS handshake with remote client: {}",
        client_addr
      );

      // Upgrade raw TCP socket context directly to secure TLS stream layer
      match acceptor_clone.accept(socket).await {
        Ok(tls_stream) => {
          println!(
            "[INPUT] TLS session successfully established for client: {}",
            client_addr
          );
          let (mut reader, mut writer) = tokio::io::split(tls_stream);
          let mut enigo = Enigo::new(&Settings::default())
            .expect("Failed to link OS Input driver");

          loop {
            // A. Extract the 4-byte length framing header
            let mut len_bytes = [0u8; 4];
            if reader.read_exact(&mut len_bytes).await.is_err() {
              break;
            }
            let packet_len = u32::from_le_bytes(len_bytes) as usize;

            // B. Allocate a dynamic packet buffer to capture the matching binary block
            let mut payload_buf = vec![0u8; packet_len];
            if reader.read_exact(&mut payload_buf).await.is_err() {
              break;
            }

            // C. Deserialize from memory space using Bincode
            if let Ok(event) = bincode::deserialize::<RemoteInput>(&payload_buf)
            {
              match event {
                RemoteInput::MouseMove { x, y } => {
                  let target_x = (x * 1920.0) as i32;
                  let target_y = (y * 1080.0) as i32;
                  let _ = enigo.move_mouse(target_x, target_y, Coordinate::Abs);
                }
                RemoteInput::MouseDown { button } => {
                  if button == "left" {
                    let _ = enigo
                      .button(enigo::Button::Left, enigo::Direction::Press);
                  }
                }
                RemoteInput::MouseUp { button } => {
                  if button == "left" {
                    let _ = enigo
                      .button(enigo::Button::Left, enigo::Direction::Release);
                  }
                }
                RemoteInput::Ping { client_time } => {
                  let response = ServerResponse::Pong { client_time };
                  if let Ok(serialized_resp) = bincode::serialize(&response) {
                    let resp_len = serialized_resp.len() as u32;
                    // Write structured binary framing layout across TLS stream
                    if writer.write_all(&resp_len.to_le_bytes()).await.is_ok() {
                      let _ = writer.write_all(&serialized_resp).await;
                    }
                  }
                }
              }
            }
          }
          println!(
            "[INPUT] Secure client connection disconnected safely: {}",
            client_addr
          );
        }
        Err(e) => {
          eprintln!(
            "[INPUT] TLS encryption handshake failure context: {:?}",
            e
          );
        }
      }
    });
  }
}
