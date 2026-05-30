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

  // CRITICAL FIX: Monotonic Presentation Timestamps
  // Enabling "do-timestamp" = true forces the DXGI surface capture allocator to map every
  // captured frame to the absolute hardware system clock at the exact microsecond of capture.
  // This provides downstream sinks with a true real-time reference baseline, preventing
  // downstream elements from desynchronizing and hoarding un-timestamped data chunks.
  let source = ElementFactory::make("d3d11screencapturesrc")
    .property("do-timestamp", true)
    .build()
    .unwrap();

  // OPTIMIZATION: Non-blocking, lock-free raw frame queue placed PRIOR to the encoder.
  // Hardcoded to a strict capacity limit of 1 buffer (`max-size-buffers = 1`) with `leaky = downstream`.
  // If the encoder or network interface blocks, old raw textures are instantly overwritten in VRAM,
  // eradicating backlog accumulation WITHOUT corrupting downstream compressed H.264 packet frames.
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

  // Post-encoder stream queue. Must remain strictly non-leaky to preserve the
  // ordered delivery of compressed H.264 bitstream sequences (I-frames/P-frames),
  // eliminating macroblock smearing and visual ghosting artifacts.
  let queue = ElementFactory::make("queue").build().unwrap();
  let videoconvert = ElementFactory::make("videoconvert").build().unwrap();

  let sink = ElementFactory::make("srtsink")
    .property("uri", "srt://0.0.0.0:5000?mode=listener")
    .property("passphrase", "SuperSecureZeroCastKey2026")
    .property_from_str("pbkeylen", "16")
    .property("latency", 20i32) // Minimal SRT buffer timeout (20ms) optimized for high-speed LAN hops
    .property("sync", false) // Bypasses global pipeline clock synchronization to enforce immediate network flight
    .build()
    .unwrap();

  // CRITICAL FIX: Variable Framerate (VFR) Transition
  // The `framerate=60/1` limitation is completely omitted from both caps filters.
  // Hardcoding a 60 FPS cap on a high-refresh-rate host monitor (e.g., 144Hz/165Hz) forced GStreamer
  // into an interpolation mismatch, piling up "surplus" frames into an unconstrained multi-second backlog.
  // Removing the rigid framerate property converts the stream to an adaptive VFR profile, letting the pipeline
  // run natively at the system's exact rendering pace, matching processing speed to frame production.
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

  // 2. Dynamic Hardware/Software Encoder Path Resolution
  let encoder = if let Ok(nv_enc) = ElementFactory::make("nvh264enc").build() {
    println!(
      "[MEDIA] NVIDIA Discrete Core detected. Mounting NVENC pipeline..."
    );
    nv_enc.set_property_from_str("preset", "low-latency-hp");

    // CRITICAL FIX: Sub-millisecond Hardware Encoding Allocation
    // Forcing "zerolatency" = true completely disables the encoder's internal lookahead reordering
    // buffer queue and completely strips out bidirectional frames (B-frames). The NVENC ASIC core
    // compresses and ships out the NAL stream packets the exact microsecond a raw VRAM texture drops
    // into its context, destroying a major hidden source of processing delay.
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

  // Embed inline SPS/PPS configuration parameters with every keyframe interval to allow rapid hot-plug connections
  parse.set_property("config-interval", 1i32);

  // Establish rigid bounds for post-encoder queuing
  queue.set_property("max-size-buffers", 3u32);
  queue.set_property("max-size-time", 0u64);
  queue.set_property("max-size-bytes", 0u32);

  let pipeline = Pipeline::with_name("zerocast-secure-capture-pipeline");

  // 3. Structural Media Graph Assembly
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

  // Link structural steps sequentially
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

  // Poll GStreamer engine bus notifications to trap asynchronous runtime exceptions
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

  // Gracefully strip pipeline context states and release hardware Direct3D11 descriptors
  pipeline.set_state(State::Null).unwrap();
  println!("[MEDIA] Secure pipeline resource successfully released.");
  Ok(())
}
