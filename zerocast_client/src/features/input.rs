use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

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

/// Asynchronous loop executing remote control replication and dynamic connection latency measurements
pub async fn run_input_service(
  target_host: String,
  mut input_rx: tokio::sync::mpsc::Receiver<RemoteInput>,
  latency_tx: tokio::sync::mpsc::Sender<f64>,
) {
  let connection_addr = format!("{}:8081", target_host);

  if let Ok(stream) = tokio::net::TcpStream::connect(&connection_addr).await {
    let (reader, mut writer) = stream.into_split();

    // Spawn asynchronous background task to capture incoming PONG packets from the host
    let latency_tx_clone = latency_tx.clone();
    tokio::spawn(async move {
      let mut buf_reader = tokio::io::BufReader::new(reader);
      let mut line = String::new();
      while let Ok(n) = buf_reader.read_line(&mut line).await {
        if n == 0 {
          break;
        }

        if let Ok(ServerResponse::Pong { client_time }) =
          serde_json::from_str::<ServerResponse>(&line)
        {
          let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

          // Round trip time split in half estimates network directional latency
          let rtt = now.saturating_sub(client_time);
          let _ = latency_tx_clone.try_send(rtt as f64 / 2.0);
        }
        line.clear();
      }
    });

    // Setup a strict interval ticks every 500ms to evaluate packet transmission speeds
    let mut ping_timer =
      tokio::time::interval(std::time::Duration::from_millis(500));
    loop {
      tokio::select! {
          Some(event) = input_rx.recv() => {
              if let Ok(mut json_str) = serde_json::to_string(&event) {
                  json_str.push('\n');
                  if writer.write_all(json_str.as_bytes()).await.is_err() { break; }
              }
          }
          _ = ping_timer.tick() => {
              let now = std::time::SystemTime::now()
                  .duration_since(std::time::UNIX_EPOCH)
                  .unwrap_or_default()
                  .as_millis() as u64;

              let ping_packet = RemoteInput::Ping { client_time: now };
              if let Ok(mut json_str) = serde_json::to_string(&ping_packet) {
                  json_str.push('\n');
                  if writer.write_all(json_str.as_bytes()).await.is_err() { break; }
              }
          }
      }
    }
  } else {
    eprintln!(
      "[INPUT] Failed to connect to input replication endpoint at: {}",
      connection_addr
    );
  }
}
