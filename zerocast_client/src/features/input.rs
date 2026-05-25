use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_native_tls::TlsConnector;

use egui::{Event, Key};
pub use zerocast_core::input::RemoteInput;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerResponse {
  Pong { client_time: u64 },
}

/// Asynchronous loop executing secure remote control replication and dynamic connection latency measurements
pub async fn run_input_service(
  target_host: String,
  mut input_rx: tokio::sync::mpsc::Receiver<RemoteInput>,
  latency_tx: tokio::sync::mpsc::Sender<f64>,
) {
  let connection_addr = format!("{}:8081", target_host);
  println!(
    "[INPUT] Connecting secure socket channel to: {}",
    connection_addr
  );

  match TcpStream::connect(&connection_addr).await {
    Ok(raw_stream) => {
      // Configure TLS rules wrapper layout
      let native_connector = native_tls::TlsConnector::builder()
        // CRITICAL FOR DEMO: Bypasses domain verification and root check rules for local/self-signed certs
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
      let connector = TlsConnector::from(native_connector);

      match connector.connect(&target_host, raw_stream).await {
        Ok(tls_stream) => {
          println!(
            "[INPUT] Secure TLS tunnel connection completed on host target."
          );
          let (mut reader, mut writer) = tokio::io::split(tls_stream);

          // Asynchronous reader thread extracting binary structured host PONG packages
          let latency_tx_clone = latency_tx.clone();
          tokio::spawn(async move {
            loop {
              let mut len_bytes = [0u8; 4];
              if reader.read_exact(&mut len_bytes).await.is_err() {
                break;
              }
              let packet_len = u32::from_le_bytes(len_bytes) as usize;

              let mut payload_buf = vec![0u8; packet_len];
              if reader.read_exact(&mut payload_buf).await.is_err() {
                break;
              }

              if let Ok(ServerResponse::Pong { client_time }) =
                bincode::deserialize::<ServerResponse>(&payload_buf)
              {
                let now = std::time::SystemTime::now()
                  .duration_since(std::time::UNIX_EPOCH)
                  .unwrap_or_default()
                  .as_millis() as u64;

                let rtt = now.saturating_sub(client_time);
                let _ = latency_tx_clone.try_send(rtt as f64 / 2.0);
              }
            }
            println!("[INPUT] Host telemetry read connection broke down.");
          });

          // Main transmission event coordinator loop
          let mut ping_timer =
            tokio::time::interval(std::time::Duration::from_millis(500));
          loop {
            tokio::select! {
                // Outbound user movement and action channel events
                Some(event) = input_rx.recv() => {
                    if let Ok(serialized_bytes) = bincode::serialize(&event) {
                        let packet_len = serialized_bytes.len() as u32;
                        if writer.write_all(&packet_len.to_le_bytes()).await.is_err() { break; }
                        if writer.write_all(&serialized_bytes).await.is_err() { break; }
                    }
                }
                // Autonomous network telemetry ticks
                _ = ping_timer.tick() => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;

                    let ping_packet = RemoteInput::Ping { client_time: now };
                    if let Ok(serialized_bytes) = bincode::serialize(&ping_packet) {
                        let packet_len = serialized_bytes.len() as u32;
                        if writer.write_all(&packet_len.to_le_bytes()).await.is_err() { break; }
                        if writer.write_all(&serialized_bytes).await.is_err() { break; }
                    }
                }
            }
          }
        }
        Err(tls_err) => {
          eprintln!("[INPUT] TLS upgrade pipeline failed: {:?}", tls_err);
        }
      }
    }
    Err(net_err) => {
      eprintln!(
        "[INPUT] Unable to mount endpoint TCP socket path: {:?}",
        net_err
      );
    }
  }
}

pub fn handle_client_keyboard_input(
  ctx: &egui::Context,
  tx: &tokio::sync::mpsc::Sender<RemoteInput>,
) {
  ctx.input(|i| {
    for event in &i.events {
      match event {
        Event::Key {
          key,
          pressed,
          modifiers: _,
          ..
        } => {
          // Map egui::Key enums to Windows Virtual Key codes
          if let Some(vk_code) = map_egui_key_to_vk(*key) {
            let input_msg = if *pressed {
              RemoteInput::KeyPress { key_code: vk_code }
            } else {
              RemoteInput::KeyRelease { key_code: vk_code }
            };

            // Dispatch asynchronously to the TLS network worker loop
            let _ = tx.try_send(input_msg);
          }
        }
        _ => {}
      }
    }
  });
}

/// Helper to map common egui layout keys to standard Windows Virtual Keys (VK_*)
fn map_egui_key_to_vk(key: Key) -> Option<u16> {
  match key {
    Key::A => Some(0x41),
    Key::B => Some(0x42),
    Key::C => Some(0x43),
    Key::D => Some(0x44),
    Key::E => Some(0x45),
    Key::F => Some(0x46),
    Key::Enter => Some(0x0D),
    Key::Escape => Some(0x1B),
    Key::Space => Some(0x20),
    Key::Backspace => Some(0x08),
    Key::Tab => Some(0x09),
    _ => None, // Expand mappings as required by the systems layout specification
  }
}
