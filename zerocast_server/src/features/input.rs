use enigo::{Coordinate, Enigo, Mouse, Settings};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
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
      let (reader, mut writer) = socket.into_split(); // Split into independent read and write halves
      let mut buf_reader = tokio::io::BufReader::new(reader);
      let mut line = String::new();

      while let Ok(bytes_read) = buf_reader.read_line(&mut line).await {
        if bytes_read == 0 {
          break;
        }

        if let Ok(event) = serde_json::from_str::<RemoteInput>(&line) {
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
              // Immediate response for client-side latency calculation
              let response = ServerResponse::Pong { client_time };
              if let Ok(mut resp_str) = serde_json::to_string(&response) {
                resp_str.push('\n');
                let _ = writer.write_all(resp_str.as_bytes()).await;
              }
            }
          }
        }
        line.clear();
      }
    });
  }
}
