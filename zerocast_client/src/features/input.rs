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

// zerocast_client/src/features/input.rs

pub fn handle_client_keyboard_input(
  ctx: &egui::Context,
  tx: &tokio::sync::mpsc::Sender<RemoteInput>,
  last_modifiers: &mut egui::Modifiers,
) {
  // 1. Process standard alphanumeric and navigation key events
  ctx.input(|i| {
    for event in &i.events {
      match event {
        Event::Key { key, pressed, .. } => {
          if let Some(vk_code) = map_egui_key_to_vk(*key) {
            let input_msg = if *pressed {
              RemoteInput::KeyPress { key_code: vk_code }
            } else {
              RemoteInput::KeyRelease { key_code: vk_code }
            };
            let _ = tx.try_send(input_msg);
          }
        }
        _ => {}
      }
    }
  });

  // 2. Intercept modifier mutations via frame-by-frame differential checks
  let current_modifiers = ctx.input(|i| i.modifiers);

  if current_modifiers.ctrl != last_modifiers.ctrl {
    let _ = tx.try_send(if current_modifiers.ctrl {
      RemoteInput::KeyPress { key_code: 0x11 } // VK_CONTROL
    } else {
      RemoteInput::KeyRelease { key_code: 0x11 }
    });
  }
  if current_modifiers.shift != last_modifiers.shift {
    let _ = tx.try_send(if current_modifiers.shift {
      RemoteInput::KeyPress { key_code: 0x10 } // VK_SHIFT
    } else {
      RemoteInput::KeyRelease { key_code: 0x10 }
    });
  }
  if current_modifiers.alt != last_modifiers.alt {
    let _ = tx.try_send(if current_modifiers.alt {
      RemoteInput::KeyPress { key_code: 0x12 } // VK_MENU (Alt)
    } else {
      RemoteInput::KeyRelease { key_code: 0x12 }
    });
  }
  if current_modifiers.command != last_modifiers.command {
    let _ = tx.try_send(if current_modifiers.command {
      RemoteInput::KeyPress { key_code: 0x5B } // VK_LWIN (Windows Key)
    } else {
      RemoteInput::KeyRelease { key_code: 0x5B }
    });
  }

  // Cache the current frame state as the evaluation baseline for the next frame tick
  *last_modifiers = current_modifiers;
}

/// Comprehensive translation layer mapping valid egui::Key space to Windows Virtual Keys
fn map_egui_key_to_vk(key: Key) -> Option<u16> {
  match key {
    // --- Alphabetical Keys ---
    Key::A => Some(0x41),
    Key::B => Some(0x42),
    Key::C => Some(0x43),
    Key::D => Some(0x44),
    Key::E => Some(0x45),
    Key::F => Some(0x46),
    Key::G => Some(0x47),
    Key::H => Some(0x48),
    Key::I => Some(0x49),
    Key::J => Some(0x4A),
    Key::K => Some(0x4B),
    Key::L => Some(0x4C),
    Key::M => Some(0x4D),
    Key::N => Some(0x4E),
    Key::O => Some(0x4F),
    Key::P => Some(0x50),
    Key::Q => Some(0x51),
    Key::R => Some(0x52),
    Key::S => Some(0x53),
    Key::T => Some(0x54),
    Key::U => Some(0x55),
    Key::V => Some(0x56),
    Key::W => Some(0x57),
    Key::X => Some(0x58),
    Key::Y => Some(0x59),
    Key::Z => Some(0x5A),

    // --- Standard Row Number Keys ---
    Key::Num0 => Some(0x30),
    Key::Num1 => Some(0x31),
    Key::Num2 => Some(0x32),
    Key::Num3 => Some(0x33),
    Key::Num4 => Some(0x34),
    Key::Num5 => Some(0x35),
    Key::Num6 => Some(0x36),
    Key::Num7 => Some(0x37),
    Key::Num8 => Some(0x38),
    Key::Num9 => Some(0x39),

    // --- Structural UI & Navigation Controls ---
    Key::Enter => Some(0x0D),
    Key::Escape => Some(0x1B),
    Key::Space => Some(0x20),
    Key::Backspace => Some(0x08),
    Key::Tab => Some(0x09),
    Key::Insert => Some(0x2D),
    Key::Delete => Some(0x2E),
    Key::Home => Some(0x24),
    Key::End => Some(0x23),
    Key::PageUp => Some(0x21),
    Key::PageDown => Some(0x22),

    // --- Directional Arrows ---
    Key::ArrowLeft => Some(0x25),
    Key::ArrowUp => Some(0x26),
    Key::ArrowRight => Some(0x27),
    Key::ArrowDown => Some(0x28),

    // --- Function Keys ---
    Key::F1 => Some(0x70),
    Key::F2 => Some(0x71),
    Key::F3 => Some(0x72),
    Key::F4 => Some(0x73),
    Key::F5 => Some(0x74),
    Key::F6 => Some(0x75),
    Key::F7 => Some(0x76),
    Key::F8 => Some(0x77),
    Key::F9 => Some(0x78),
    Key::F10 => Some(0x79),
    Key::F11 => Some(0x7A),
    Key::F12 => Some(0x7B),
    Key::F13 => Some(0x7C),
    Key::F14 => Some(0x7D),
    Key::F15 => Some(0x7E),
    Key::F16 => Some(0x7F),
    Key::F17 => Some(0x80),
    Key::F18 => Some(0x81),
    Key::F19 => Some(0x82),
    Key::F20 => Some(0x83),

    // --- Common Symbol Layout Keys ---
    Key::Colon => Some(0xBA),
    Key::Comma => Some(0xBC),
    Key::Period => Some(0xBE),
    Key::Minus => Some(0xBD),
    Key::Plus => Some(0xBB),
    Key::Equals => Some(0xBB),

    _ => None,
  }
}
