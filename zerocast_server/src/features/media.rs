use gstreamer::prelude::*;
use gstreamer::{Element, ElementFactory, MessageView, Pipeline, State};

/// Native GStreamer pipeline for hardware-accelerated screen capture (D3D11),
/// supporting dynamic cross-vendor GPU/CPU encoding fallback.
///
/// This implementation optimizes real-time data flow to eliminate progressive
/// buffering delays (Time-Drift Logjams) by shifting to a Variable Framerate (VFR)
/// architecture and bypassing internal encoder lookup caches.
pub fn run_media_pipeline(_client_ip: std::net::IpAddr) -> Result<(), String> {
  gstreamer::init()
    .map_err(|e| format!("GStreamer initialization error: {:?}", e))?;

  // 1. Core Ingestion and Filtering Component Instantiation
  let source = ElementFactory::make("d3d11screencapturesrc")
    .property("do-timestamp", true)
    .build()
    .unwrap();

  let raw_queue = ElementFactory::make("queue")
    .property("max-size-buffers", 1u32)
    .property("max-size-time", 0u64)
    .property("max-size-bytes", 0u32)
    .property_from_str("leaky", "downstream")
    .build()
    .unwrap();

  let d3d11scale = ElementFactory::make("d3d11scale").build().unwrap();
  let d3d11convert = ElementFactory::make("d3d11convert").build().unwrap();
  let d3d11download = ElementFactory::make("d3d11download").build().unwrap();
  let parse = ElementFactory::make("h264parse").build().unwrap();

  let queue = ElementFactory::make("queue").build().unwrap();
  let videoconvert = ElementFactory::make("videoconvert").build().unwrap();

  let sink = ElementFactory::make("srtsink")
    .property("uri", "srt://0.0.0.0:5000?mode=listener")
    .property("passphrase", "SuperSecureZeroCastKey2026")
    .property_from_str("pbkeylen", "16")
    .property("latency", 20i32)
    .property("sync", false) // FAST PATH: Bypasses clock sync for minimum latency
    .build()
    .unwrap();

  // Optimized VFR Caps (no hard framerate limit to avoid interpolation delays)
  let gpu_caps = ElementFactory::make("capsfilter")
    .property_from_str(
      "caps",
      "video/x-raw(memory:D3D11Memory), width=1920, height=1080, format=NV12",
    )
    .build()
    .unwrap();

  let cpu_caps = ElementFactory::make("capsfilter")
    .property_from_str(
      "caps",
      "video/x-raw, width=1920, height=1080, format=NV12",
    )
    .build()
    .unwrap();

  let encoder = if let Ok(nv_enc) = ElementFactory::make("nvh264enc").build() {
    println!("[MEDIA] NVIDIA Discrete Core detected. Mounting NVENC pipeline...");
    nv_enc.set_property_from_str("preset", "low-latency-hp");
    nv_enc.set_property("zerolatency", true);
    nv_enc.set_property_from_str("rc-mode", "cbr");
    nv_enc.set_property("bitrate", 4000u32);
    nv_enc.set_property("gop-size", 30i32);
    nv_enc.set_property("bframes", 0u32);
    nv_enc.set_property("rc-lookahead", 0u32);
    nv_enc.set_property("aud", true);
    nv_enc
  } else if let Ok(openh264_enc) = ElementFactory::make("openh264enc").build() {
    println!("[MEDIA] NVIDIA hardware absent. Initializing fallback...");
    openh264_enc.set_property_from_str("usage-type", "screen");
    openh264_enc.set_property_from_str("rate-control", "bitrate");
    openh264_enc.set_property("bitrate", 4000u32);
    openh264_enc.set_property("gop-size", 30u32);
    openh264_enc
  } else {
    ElementFactory::make("x264enc").build().unwrap()
  };

  parse.set_property("config-interval", 1i32);
  queue.set_property("max-size-buffers", 3u32);

  let pipeline = Pipeline::with_name("zerocast-vfr-speed-pipeline");

  pipeline.add_many([
    &source, &raw_queue, &d3d11scale, &d3d11convert, &gpu_caps, &d3d11download, 
    &cpu_caps, &videoconvert, &encoder, &parse, &queue, &sink
  ]).unwrap();

  Element::link_many([
    &source, &raw_queue, &d3d11scale, &d3d11convert, &gpu_caps, &d3d11download, 
    &cpu_caps, &videoconvert, &encoder, &parse, &queue, &sink
  ]).unwrap();

  println!("[MEDIA] Launching high-speed VFR pipeline...");
  pipeline.set_state(State::Playing).map_err(|e| format!("{:?}", e))?;

  let bus = pipeline.bus().unwrap();
  for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
    match msg.view() {
      MessageView::Error(err) => {
        eprintln!("[MEDIA] Pipeline error: {}", err.error());
        break;
      }
      _ => (),
    }
  }

  pipeline.set_state(State::Null).unwrap();
  Ok(())
}
