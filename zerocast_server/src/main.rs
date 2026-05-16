use gstreamer::prelude::*;
use gstreamer::{Element, ElementFactory, MessageView, Pipeline, State};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use zerocast_core::auth::{AuthRequest, AuthResponse};

mod features;

use features::auth::interactor::AuthInteractor;
use features::auth::session::SessionStore;

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

  println!(
    "Waiting for an authorized client connection to initialize streaming..."
  );

  let client_ip = stream_signal_rx
    .recv()
    .await
    .expect("Signal channel closed unexpectedly");

  gstreamer::init().expect("Failed to initialize GStreamer!");

  let source = ElementFactory::make("d3d11screencapturesrc")
    .name("screen_source")
    .build()
    .expect("Failed to create d3d11screencapturesrc");

  let d3d11scale = ElementFactory::make("d3d11scale")
    .name("gpu_scaler")
    .build()
    .expect("Failed to create d3d11scale");

  let gpu_caps = ElementFactory::make("capsfilter")
        .name("gpu_caps")
        .property_from_str("caps", "video/x-raw(memory:D3D11Memory), width=1920, height=1080, framerate=60/1")
        .build()
        .expect("Failed to create gpu_caps");

  let d3d11download = ElementFactory::make("d3d11download")
    .name("gpu_download")
    .build()
    .expect("Failed to create d3d11download");

  let videoconvert = ElementFactory::make("videoconvert")
    .name("converter")
    .build()
    .expect("Failed to create videoconvert");

  let cpu_caps = ElementFactory::make("capsfilter")
    .name("cpu_caps")
    .property_from_str("caps", "video/x-raw, format=NV12")
    .build()
    .expect("Failed to create cpu_caps");

  let queue = ElementFactory::make("queue")
    .name("encoder_queue")
    .property("max-size-buffers", 3u32)
    .build()
    .expect("Failed to create queue");

  let encoder = ElementFactory::make("nvh264enc")
    .name("nvidia_encoder")
    .property_from_str("preset", "low-latency-hp")
    .property_from_str("gop-size", "60")
    .property("bitrate", 15000u32)
    .build()
    .expect("Failed to create nvh264enc");

  let payloader = ElementFactory::make("rtph264pay")
    .name("rtp_payloader")
    .property("config-interval", 1i32)
    .build()
    .expect("Failed to create rtph264pay");

  let sink = ElementFactory::make("udpsink")
    .name("udp_output")
    .property("host", client_ip.to_string())
    .property("port", 5000i32)
    .property("sync", false)
    .property("buffer-size", 20_971_520i32)
    .build()
    .expect("Failed to create udpsink");

  let pipeline = Pipeline::with_name("zerocast-capture-pipeline");

  pipeline
    .add_many([
      &source,
      &d3d11scale,
      &gpu_caps,
      &d3d11download,
      &videoconvert,
      &cpu_caps,
      &queue,
      &encoder,
      &payloader,
      &sink,
    ])
    .unwrap();

  Element::link_many([
    &source,
    &d3d11scale,
    &gpu_caps,
    &d3d11download,
    &videoconvert,
    &cpu_caps,
    &queue,
    &encoder,
    &payloader,
    &sink,
  ])
  .unwrap();

  println!(
    "Streaming started! Hardware-Accelerated 1080p60 NVENC pipeline active..."
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
