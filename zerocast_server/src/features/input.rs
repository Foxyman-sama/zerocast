use enigo::{Coordinate, Enigo, Mouse, Settings};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

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

/// Asynchronous service handling input replication and Ping-Pong latency measurement (TCP Port 8081)
pub async fn run_input_replication_service()
-> Result<(), Box<dyn std::error::Error>> {
  let listener = TcpListener::bind("0.0.0.0:8081").await?;
  println!("[INPUT] Service successfully bound to port 8081");

  loop {
    let (socket, _) = listener.accept().await?;

    tokio::spawn(async move {
      let mut enigo = Enigo::new(&Settings::default())
        .expect("Failed to link OS Input driver");
      let (mut reader, mut writer) = socket.into_split();

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

        if let Ok(event) = bincode::deserialize::<RemoteInput>(&payload_buf) {
          match event {
            RemoteInput::MouseMove { x, y } => {
              let target_x = (x * 1920.0) as i32;
              let target_y = (y * 1080.0) as i32;
              let _ = enigo.move_mouse(target_x, target_y, Coordinate::Abs);
            }
            RemoteInput::MouseDown { button } => {
              if button == "left" {
                let _ =
                  enigo.button(enigo::Button::Left, enigo::Direction::Press);
              }
            }
            RemoteInput::MouseUp { button } => {
              if button == "left" {
                let _ =
                  enigo.button(enigo::Button::Left, enigo::Direction::Release);
              }
            }
            RemoteInput::Ping { client_time } => {
              let response = ServerResponse::Pong { client_time };
              if let Ok(serialized_resp) = bincode::serialize(&response) {
                let resp_len = serialized_resp.len() as u32;
                if writer.write_all(&resp_len.to_le_bytes()).await.is_ok() {
                  let _ = writer.write_all(&serialized_resp).await;
                }
              }
            }
          }
        }
      }
    });
  }
}
