use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    let (mut reader, mut writer) = stream.into_split();

    // Spawn asynchronous background task to capture incoming binary PONG packets from the host
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
    });

    let mut ping_timer =
      tokio::time::interval(std::time::Duration::from_millis(500));
    loop {
      tokio::select! {
          Some(event) = input_rx.recv() => {
              if let Ok(serialized_bytes) = bincode::serialize(&event) {
                  let packet_len = serialized_bytes.len() as u32;
                  if writer.write_all(&packet_len.to_le_bytes()).await.is_err() { break; }
                  if writer.write_all(&serialized_bytes).await.is_err() { break; }
              }
          }
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
  } else {
    eprintln!(
      "[INPUT] Failed to connect to input replication endpoint at: {}",
      connection_addr
    );
  }
}
