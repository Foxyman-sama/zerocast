use enigo::{Coordinate, Enigo, Mouse, Settings};
use native_tls::Identity;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_native_tls::TlsAcceptor;

use windows::Win32::UI::Input::KeyboardAndMouse::{
  INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
};

use std::sync::Arc;

pub use zerocast_core::input::RemoteInput;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerTelemetry {
  pub cpu_usage: f32,
  pub gpu_usage: f32,
  pub ram_usage_mb: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerResponse {
  Pong {
    client_time: u64,
    telemetry: Option<ServerTelemetry>,
  },
}

/// Gathers real-time performance metrics from the host operating system
fn get_server_telemetry(sys: &mut sysinfo::System) -> ServerTelemetry {
  sys.refresh_all();
  let current_pid = sysinfo::Pid::from(std::process::id() as usize);
  
  let (cpu, ram) = if let Some(process) = sys.process(current_pid) {
    (process.cpu_usage(), process.memory() as f32 / 1024.0 / 1024.0)
  } else {
    (0.0, 0.0)
  };

  // Attempt to query NVENC utilization via nvidia-smi if available
  let gpu = std::process::Command::new("nvidia-smi")
    .args(["--query-gpu=utilization.encoder", "--format=csv,noheader,nounits"])
    .output()
    .ok()
    .and_then(|out| {
      String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f32>()
        .ok()
    })
    .unwrap_or(0.0);

  ServerTelemetry {
    cpu_usage: cpu,
    gpu_usage: gpu,
    ram_usage_mb: ram,
  }
}

/// Asynchronous service handling input replication and Ping-Pong latency measurement over TLS
pub async fn run_input_replication_service()
-> Result<(), Box<dyn std::error::Error>> {
  let listener = TcpListener::bind("0.0.0.0:8081").await?;
  println!("[INPUT] Secured Service successfully bound to port 8081");
  
  let sys = Arc::new(tokio::sync::Mutex::new(sysinfo::System::new_all()));

  // 1. Initialize local cryptographic identity profile from PKCS#12 store
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
    let sys_clone = Arc::clone(&sys);

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
                RemoteInput::KeyPress { key_code } => {
                  // Call native Win32 SendInput subsystem directly
                  inject_windows_key(key_code, false);
                }
                RemoteInput::KeyRelease { key_code } => {
                  // Call native Win32 SendInput subsystem directly
                  inject_windows_key(key_code, true);
                }
                RemoteInput::Ping { client_time } => {
                  let telemetry = {
                    let mut guard = sys_clone.lock().await;
                    Some(get_server_telemetry(&mut guard))
                  };

                  let response = ServerResponse::Pong { client_time, telemetry };
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

/// Entry point dispatched from the asynchronous TLS socket read loop coordinator
pub fn process_remote_input_event(event: RemoteInput) {
  match event {
    RemoteInput::KeyPress { key_code } => {
      inject_windows_key(key_code, false);
    }
    RemoteInput::KeyRelease { key_code } => {
      inject_windows_key(key_code, true);
    }
    RemoteInput::Ping { .. } => {
      // Echo back or update internal RTT tracking if needed
    }
    _ => {} // Mouse handling logic goes here
  }
}

/// Dispatches raw synthetic keyboard strokes straight into the OS kernel event subsystem
fn inject_windows_key(vk_code: u16, is_key_up: bool) {
  unsafe {
    // If it's a release event, assign the appropriate Windows flag wrapper; otherwise 0 (press)
    let dw_flags = if is_key_up {
      KEYEVENTF_KEYUP
    } else {
      Default::default()
    };

    let input_element = INPUT {
      r#type: INPUT_KEYBOARD,
      Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
        ki: KEYBDINPUT {
          wVk: VIRTUAL_KEY(vk_code),
          wScan: 0, // 0 defaults tracking to the Virtual Key code mapping channel
          dwFlags: dw_flags,
          time: 0, // 0 lets the system assign its own sequential timestamp ticks
          dwExtraInfo: 0,
        },
      },
    };

    // Inject the structure directly into the input queue stream
    let inputs = [input_element];
    let num_sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);

    if num_sent == 0 {
      eprintln!(
        "[INPUT ERROR] OS refused kernel injection for VK code: 0x{:X}",
        vk_code
      );
    }
  }
}
