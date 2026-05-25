use gstreamer::prelude::*;
use gstreamer::{Element, ElementFactory, MessageView, Pipeline, State};

/// Native GStreamer pipeline for hardware accelerated screen capture (D3D11),
/// supporting dynamic cross-vendor GPU/CPU encoding fallback for non-NVIDIA laptops.
pub fn run_media_pipeline(_client_ip: std::net::IpAddr) -> Result<(), String> {
  gstreamer::init()
    .map_err(|e| format!("GStreamer initialization error: {:?}", e))?;

  // 1. Instantiate core capture and translation filters
  let source = ElementFactory::make("d3d11screencapturesrc")
    .build()
    .unwrap();
  let d3d11scale = ElementFactory::make("d3d11scale").build().unwrap();
  let d3d11convert = ElementFactory::make("d3d11convert").build().unwrap();
  let d3d11download = ElementFactory::make("d3d11download").build().unwrap();
  let parse = ElementFactory::make("h264parse").build().unwrap();
  let queue = ElementFactory::make("queue").build().unwrap();

  let sink = ElementFactory::make("srtsink")
    .property("uri", "srt://0.0.0.0:5000?mode=listener")
    .property("passphrase", "SuperSecureZeroCastKey2026")
    .property_from_str("pbkeylen", "16")
    .property("latency", 20i32)
    .property("sync", false)
    .build()
    .unwrap();

  // Parameterize hardware filters within GPU and Host memory contexts
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

  // 2. DYNAMIC ENCODER RESOLUTION (Bypasses hard NVIDIA hardware restrictions)
  let encoder = if let Ok(nv_enc) = ElementFactory::make("nvh264enc").build() {
    println!(
      "[MEDIA] NVIDIA Discrete Core detected. Mounting NVENC pipeline..."
    );
    nv_enc.set_property_from_str("preset", "low-latency-hp");
    nv_enc.set_property_from_str("rc-mode", "cbr");
    nv_enc.set_property("bitrate", 12000u32); // High-fidelity 12 Mbps stream 
    nv_enc.set_property("gop-size", 60i32);
    nv_enc.set_property("bframes", 0u32);
    nv_enc.set_property("rc-lookahead", 0u32);
    nv_enc.set_property("aud", true);
    nv_enc
  } else if let Ok(openh264_enc) = ElementFactory::make("openh264enc").build() {
    println!(
      "[MEDIA] NVIDIA hardware absent. Initializing low-overhead OpenH264 fallback context..."
    );
    openh264_enc.set_property_from_str("usage-type", "screen"); // Optimizes block matching for static UI text
    openh264_enc.set_property_from_str("rate-control", "cbr");
    openh264_enc.set_property("bitrate", 6000u32); // 6 Mbps balances network throughput with laptop CPU usage
    openh264_enc.set_property("gop-size", 60u32);
    openh264_enc
  } else if let Ok(x264_enc) = ElementFactory::make("x264enc").build() {
    println!(
      "[MEDIA] OpenH264 unavailable. Initializing standard x264 software context..."
    );
    x264_enc.set_property_from_str("tune", "zerolatency");
    x264_enc.set_property_from_str("speed-preset", "ultrafast"); // Minimizes encoding thread barriers on mobile CPUs
    x264_enc.set_property("bitrate", 6000u32);
    x264_enc.set_property("key-int-max", 60u32);
    x264_enc
  } else {
    return Err("Fatal: No compatible H.264 hardware or software codec detected on this system setup.".to_string());
  };

  parse.set_property("config-interval", 1i32);
  queue.set_property("max-size-buffers", 3u32);

  let pipeline = Pipeline::with_name("zerocast-secure-capture-pipeline");

  // 3. Assemble structural layout elements (Remains fully compatible regardless of encoder choice)
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

  println!("[MEDIA] Launching adaptive streaming pipeline interface safely...");
  pipeline
    .set_state(State::Playing)
    .map_err(|e| format!("{:?}", e))?;

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
