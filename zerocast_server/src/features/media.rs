use gstreamer::prelude::*;
use gstreamer::{Element, ElementFactory, MessageView, Pipeline, State};

/// Native GStreamer pipeline for hardware accelerated screen capture (D3D11),
/// supporting dynamic cross-vendor GPU/CPU encoding fallback for non-NVIDIA laptops.
pub fn run_media_pipeline(_client_ip: std::net::IpAddr) -> Result<(), String> {
  gstreamer::init()
    .map_err(|e| format!("GStreamer initialization error: {:?}", e))?;

  // 1. Instantiate core capture and translation filters
  let source = ElementFactory::make("d3d11screencapturesrc")
    .property("do-timestamp", true)
    .build()
    .unwrap();

  // OPTIMIZATION: Raw frame leaky queue placed BEFORE the encoder compression layer.
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

  // OPTIMIZATION: Non-leaky queue placed AFTER encoder.
  let queue = ElementFactory::make("queue").build().unwrap();

  // Dynamically bridges format gaps (e.g., NV12 to I420) for software encoders
  let videoconvert = ElementFactory::make("videoconvert").build().unwrap();

  let sink = ElementFactory::make("srtsink")
    .property("uri", "srt://0.0.0.0:5000?mode=listener")
    .property("passphrase", "SuperSecureZeroCastKey2026")
    .property_from_str("pbkeylen", "16")
    .property("latency", 20i32)
    .property("sync", false)
    .build()
    .unwrap();

  // Forcing a strict framerate on a variable desktop capture causes GStreamer to queue frames if the monitor refreshes faster than 60Hz.
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

  // 2. DYNAMIC ENCODER RESOLUTION
  let encoder = if let Ok(nv_enc) = ElementFactory::make("nvh264enc").build() {
    println!(
      "[MEDIA] NVIDIA Discrete Core detected. Mounting NVENC pipeline..."
    );
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
    println!(
      "[MEDIA] NVIDIA hardware absent. Initializing low-overhead OpenH264 fallback context..."
    );
    openh264_enc.set_property_from_str("usage-type", "screen");
    openh264_enc.set_property_from_str("rate-control", "bitrate");
    openh264_enc.set_property("bitrate", 4000u32);
    openh264_enc.set_property("gop-size", 30u32);
    openh264_enc
  } else if let Ok(x264_enc) = ElementFactory::make("x264enc").build() {
    println!(
      "[MEDIA] OpenH264 unavailable. Initializing standard x264 software context..."
    );
    x264_enc.set_property_from_str("tune", "zerolatency");
    x264_enc.set_property_from_str("speed-preset", "ultrafast");
    x264_enc.set_property("bitrate", 4000u32);
    x264_enc.set_property("key-int-max", 30u32);
    x264_enc
  } else {
    return Err("Fatal: No compatible H.264 hardware or software codec detected on this system setup.".to_string());
  };

  parse.set_property("config-interval", 1i32);

  // Reconfigure post-encoder stream queue to a small non-leaky cushion
  queue.set_property("max-size-buffers", 3u32);
  queue.set_property("max-size-time", 0u64);
  queue.set_property("max-size-bytes", 0u32);

  let pipeline = Pipeline::with_name("zerocast-secure-capture-pipeline");

  // 3. Assemble structural layout elements
  pipeline
    .add_many([
      &source,
      &raw_queue,
      &d3d11scale,
      &d3d11convert,
      &gpu_caps,
      &d3d11download,
      &cpu_caps,
      &videoconvert,
      &encoder,
      &parse,
      &queue,
      &sink,
    ])
    .unwrap();

  Element::link_many([
    &source,
    &raw_queue,
    &d3d11scale,
    &d3d11convert,
    &gpu_caps,
    &d3d11download,
    &cpu_caps,
    &videoconvert,
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
