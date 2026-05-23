use gstreamer::prelude::*;
use gstreamer::{Element, ElementFactory, MessageView, Pipeline, State};

/// Native GStreamer pipeline for hardware accelerated screen capture (D3D11),
/// NVENC video encoding, and secure SRT streaming wrapped in AES-128 encryption.
pub fn run_media_pipeline(client_ip: std::net::IpAddr) -> Result<(), String> {
  gstreamer::init()
    .map_err(|e| format!("GStreamer initialization error: {:?}", e))?;

  // Instantiate capture and processing elements within VRAM contexts
  let source = ElementFactory::make("d3d11screencapturesrc")
    .build()
    .unwrap();
  let d3d11scale = ElementFactory::make("d3d11scale").build().unwrap();
  let d3d11convert = ElementFactory::make("d3d11convert").build().unwrap();
  let d3d11download = ElementFactory::make("d3d11download").build().unwrap();
  let parse = ElementFactory::make("h264parse").build().unwrap();
  let queue = ElementFactory::make("queue").build().unwrap();

  // internal C-enum (GstSRTKeyLength) mapping instead of casting as standard gint.
  let sink = ElementFactory::make("srtsink")
    .property("uri", "srt://0.0.0.0:5000?mode=listener")
    .property("passphrase", "SuperSecureZeroCastKey2026")
    .property_from_str("pbkeylen", "16") // "16" string directly resolves into GstSRTKeyLength::Aes128
    .property("latency", 20i32)
    .property("sync", false)
    .build()
    .unwrap();

  // Parameterize hardware filters within GPU memory contexts
  let gpu_caps = ElementFactory::make("capsfilter")
        .property_from_str("caps", "video/x-raw(memory:D3D11Memory), width=1920, height=1080, format=NV12, framerate=60/1")
        .build().unwrap();

  let cpu_caps = ElementFactory::make("capsfilter")
    .property_from_str(
      "caps",
      "video/x-raw, width=1920, height=1080, format=NV12, framerate=60/1",
    )
    .build()
    .unwrap();

  // Configure NVIDIA NVENC encoder for ultra-low latency (Zero-Latency mode)
  let encoder = ElementFactory::make("nvh264enc")
    .property_from_str("preset", "low-latency-hp")
    .property_from_str("rc-mode", "cbr")
    .property("bitrate", 12000u32) // 12 Mbps optimizes LAN streams without saturating socket loops
    .property("gop-size", 60i32) // Force a keyframe exactly once per second at 60 FPS
    .property("bframes", 0u32) // 0 B-frames guarantees zero structural reordering latency
    .property("rc-lookahead", 0u32) // Bypass lookahead queuing loops for immediate delivery
    .property("aud", true) // Access Unit Delimiters protect frame integrity against network packet loss
    .build()
    .unwrap();

  parse.set_property("config-interval", 1i32);
  queue.set_property("max-size-buffers", 3u32);

  let pipeline = Pipeline::with_name("zerocast-secure-capture-pipeline");

  // Register elements into the pipeline container (omitting RTP payloaders)
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
      &sink,
    ])
    .unwrap();

  // Establish structural link layout between elements
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
    &sink,
  ])
  .unwrap();

  println!(
    "[MEDIA] Starting GStreamer zero-copy pipeline layout over Encrypted SRT (AES-128) at 1080p60..."
  );
  pipeline
    .set_state(State::Playing)
    .map_err(|e| format!("{:?}", e))?;

  // Monitor the media bus for runtime exceptions (runs in a dedicated OS thread context)
  let bus = pipeline.bus().unwrap();
  for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
    match msg.view() {
      MessageView::Error(err) => {
        eprintln!(
          "[MEDIA] Pipeline runtime error: {} ({:?})",
          err.error(),
          err.debug()
        );
        break;
      }
      _ => (),
    }
  }

  pipeline.set_state(State::Null).unwrap();
  println!("[MEDIA] Secure pipeline resource successfully released.");
  Ok(())
}
