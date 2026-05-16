use enigo::{Coordinate, Enigo, Mouse, Settings};
use gstreamer::prelude::*;
use gstreamer::{Element, ElementFactory, MessageView, Pipeline, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use zerocast_core::auth::{AuthRequest, AuthResponse};

mod features;

use features::auth::interactor::AuthInteractor;
use features::auth::session::SessionStore;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RemoteInput {
  MouseMove { x: f32, y: f32 },
  MouseDown { button: String },
  MouseUp { button: String },
}

#[tokio::main]
async fn main() {
  let store = Arc::new(SessionStore::new());
  let creds = AuthInteractor::generate_host_credentials();

  {
    let mut guard = store.current_creds.lock().await;
    *guard = Some(creds.clone());
  }

  println!("LOGIN: {} | PASSWORD: {}", creds.login, creds.password);

  let (stream_signal_tx, mut stream_signal_rx) =
    mpsc::channel::<std::net::IpAddr>(1);

  let store_clone = Arc::clone(&store);
  tokio::spawn(async move {
    let listener = TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Auth listener active on port 8080");

    loop {
      let (mut socket, peer_addr) = listener.accept().await.unwrap();
      let store_for_task = Arc::clone(&store_clone);
      let signal_tx = stream_signal_tx.clone();

      tokio::spawn(async move {
        let mut buffer = [0; 1024];
        let n = socket.read(&mut buffer).await.unwrap();
        let req: AuthRequest = serde_json::from_slice(&buffer[..n]).unwrap();

        let response =
          AuthInteractor::validate_client(store_for_task, req).await;

        let is_success = match &response {
          AuthResponse::Success { session_token } => {
            println!(
              "Client session authorized successfully with token: {}",
              session_token
            );
            true
          }
          AuthResponse::Failure { reason } => {
            println!("Client authentication failed. Reason: {}", reason);
            false
          }
        };

        let resp_bytes = serde_json::to_vec(&response).unwrap();
        socket.write_all(&resp_bytes).await.unwrap();

        if is_success {
          println!(
            "Signaling main loop to initialize video stream extraction for: {}",
            peer_addr
          );
          let _ = signal_tx.send(peer_addr.ip()).await;
        }
      });
    }
  });

  tokio::spawn(async move {
    let listener = TcpListener::bind("0.0.0.0:8081").await.unwrap();
    println!("Input replication listener active on port 8081");

    loop {
      if let Ok((socket, _)) = listener.accept().await {
        tokio::spawn(async move {
          let mut enigo = Enigo::new(&Settings::default()).unwrap();
          let mut reader = tokio::io::BufReader::new(socket);
          let mut line = String::new();

          while let Ok(bytes_read) = reader.read_line(&mut line).await {
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
              }
            }
            line.clear();
          }
        });
      }
    }
  });

  println!(
    "Waiting for an authorized client connection to initialize streaming..."
  );

  let client_ip = stream_signal_rx
    .recv()
    .await
    .expect("Signal channel closed unexpectedly");

  gstreamer::init().expect("Failed to initialize GStreamer!");

  let source = ElementFactory::make("d3d11screencapturesrc")
    .build()
    .unwrap();
  let d3d11scale = ElementFactory::make("d3d11scale").build().unwrap();
  let d3d11convert = ElementFactory::make("d3d11convert").build().unwrap();

  // Lock resolution, color layout, and 60 FPS pacing entirely inside hardware VRAM layers
  let gpu_caps = ElementFactory::make("capsfilter")
        .property_from_str(
            "caps",
            "video/x-raw(memory:D3D11Memory), width=1920, height=1080, format=NV12, framerate=60/1",
        )
        .build()
        .unwrap();

  let d3d11download = ElementFactory::make("d3d11download").build().unwrap();

  let cpu_caps = ElementFactory::make("capsfilter")
    .property_from_str(
      "caps",
      "video/x-raw, width=1920, height=1080, format=NV12, framerate=60/1",
    )
    .build()
    .unwrap();

  // Optimized NVIDIA NVENC Configuration
  let encoder = ElementFactory::make("nvh264enc")
    .property_from_str("preset", "low-latency-hp")
    .property_from_str("rc-mode", "cbr")
    .property("bitrate", 12000u32) // 12 Mbps optimizes LAN data streams without network saturation
    .property("gop-size", 60i32) // Strictly output one keyframe per second at 60 FPS
    .property("bframes", 0u32) // 0 B-frames guarantees low processing delay
    .property("rc-lookahead", 0u32) // Bypass lookahead buffering loops for real-time output stream delivery
    .property("aud", true) // Access Unit Delimiters preserve UDP frame integrity
    .build()
    .unwrap();

  let parse = ElementFactory::make("h264parse")
    .property("config-interval", 1i32)
    .build()
    .unwrap();

  let queue = ElementFactory::make("queue")
    .property("max-size-buffers", 3u32)
    .build()
    .unwrap();

  let payloader = ElementFactory::make("rtph264pay")
    .property("mtu", 1400u32)
    .build()
    .unwrap();

  let sink = ElementFactory::make("udpsink")
    .property("host", client_ip.to_string())
    .property("port", 5000i32)
    .property("sync", false)
    .property("buffer-size", 41_943_040i32) // 40MB socket allocation protects against network burst drops
    .build()
    .unwrap();

  let pipeline = Pipeline::with_name("zerocast-capture-pipeline");

  pipeline
    .add_many([
      &source,
      &d3d11scale,
      &d3d11convert,
      &gpu_caps,
      &d3d11download,
      &cpu_caps,
      &encoder,
      &parse,
      &queue,
      &payloader,
      &sink,
    ])
    .unwrap();

  Element::link_many([
    &source,
    &d3d11scale,
    &d3d11convert,
    &gpu_caps,
    &d3d11download,
    &cpu_caps,
    &encoder,
    &parse,
    &queue,
    &payloader,
    &sink,
  ])
  .unwrap();

  println!(
    "Streaming started! Balanced Hardware 1080p60 UDP Pipeline active..."
  );
  pipeline.set_state(State::Playing).unwrap();

  let bus = pipeline.bus().unwrap();
  std::thread::spawn(move || {
    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
      match msg.view() {
        MessageView::Error(err) => {
          eprintln!(
            "Pipeline runtime error: {} ({:?})",
            err.error(),
            err.debug()
          );
          break;
        }
        _ => (),
      }
    }
    pipeline.set_state(State::Null).unwrap();
  });

  tokio::signal::ctrl_c().await.unwrap();
  println!("Server shutting down safely...");
}
