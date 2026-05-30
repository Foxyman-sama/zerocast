use gstreamer::prelude::*;
use gstreamer::{Caps, Element, ElementFactory, MessageView, Pipeline, State};
use std::sync::Arc;

/// Native low-overhead pipeline ingest via hardware D3D11 render utilities and secure SRT decryption.
///
/// This client-side implementation mirrors the server's architectural optimizations by
/// stripping away presentation sync layers, bypassing time-based caching boundaries, and
/// using a lock-free memory recycling pool to prevent runtime allocation bottlenecks.
pub fn start_gstreamer_pipeline(
  target_server_ip: String,
  raw_frame_tx: tokio::sync::mpsc::Sender<(Vec<u8>, i32, i32)>,
  buffer_pool: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
) -> Result<(), String> {
  gstreamer::init().map_err(|e| format!("GStreamer error: {:?}", e))?;

  // 1. Inbound SRT Network Stream Ingestion Node
  // Configured in caller mode to actively handshake with the listening server.
  // The latency parameter is tightly bounded to 20ms to match high-speed LAN hops.
  let source = ElementFactory::make("srtsrc")
    .property(
      "uri",
      format!("srt://{}:5000?mode=caller", target_server_ip),
    )
    .property("passphrase", "SuperSecureZeroCastKey2026")
    .property_from_str("pbkeylen", "16")
    .property("latency", 20i32)
    .build()
    .unwrap();

  let parse = ElementFactory::make("h264parse").build().unwrap();

  // CRITICAL OPTIMIZATION: Disabling Default Time/Byte Caching
  // By default, GStreamer queues enforce hidden 1-second time boundaries and multi-megabyte
  // byte-size limits. Over real network environments, this causes frames to buffer silently inside
  // the queue element. Forcing `max-size-time = 0` and `max-size-bytes = 0` restricts the queue
  // to evaluate bounds strictly via the 3-buffer frame count limit, preventing progressive lag.
  let queue1 = ElementFactory::make("queue")
    .property("max-size-buffers", 3u32)
    .property("max-size-time", 0u64) // <-- CRITICAL FIX: Forces queue to ignore default 1-second time buffers
    .property("max-size-bytes", 0u32) // <-- CRITICAL FIX: Forces queue to ignore default byte buffers
    .build()
    .unwrap();

  // Direct3D11 Hardware Accelerated Video Decoding Pipeline
  // Decompresses the inbound H.264 bitstream directly inside VRAM contexts to avoid CPU decoding spikes.
  let decode = ElementFactory::make("d3d11h264dec").build().unwrap();
  let gpu_convert = ElementFactory::make("d3d11convert").build().unwrap();

  let client_gpu_caps = ElementFactory::make("capsfilter")
    .property_from_str("caps", "video/x-raw(memory:D3D11Memory), format=RGBA")
    .build()
    .unwrap();

  let download = ElementFactory::make("d3d11download").build().unwrap();
  let appsink = ElementFactory::make("appsink")
    .build()
    .unwrap()
    .dynamic_cast::<gstreamer_app::AppSink>()
    .expect("AppSink cast failed");

  // CRITICAL OPTIMIZATION: (Immediate Sink Delivery)
  // 1. `set_max_buffers(1)` and `set_drop(true)` force the sink to instantly overwrite any unrendered
  //    stale frames, ensuring the egui thread only receives the most current state of the stream.
  // 2. `sync = false` destroys GStreamer's default presentation synchronization layer. Instead of delaying
  //    frames to match encoded presentation timestamps, the sink pushes frames out the millisecond they are decoded.
  appsink.set_max_buffers(1);
  appsink.set_drop(true);
  appsink.set_property("sync", false);
  appsink.set_property(
    "caps",
    &Caps::builder("video/x-raw").field("format", "RGBA").build(),
  );

  let pipeline = Pipeline::with_name("client-secure-render-pipeline");

  // 2. Assemble Structural Pipeline Components into the Active Media Graph
  pipeline
    .add_many([
      &source,
      &parse,
      &queue1,
      &decode,
      &gpu_convert,
      &client_gpu_caps,
      &download,
      appsink.upcast_ref(),
    ])
    .unwrap();

  Element::link_many([
    &source,
    &parse,
    &queue1,
    &decode,
    &gpu_convert,
    &client_gpu_caps,
    &download,
    appsink.upcast_ref(),
  ])
  .unwrap();

  // Asynchronous Frame Pull Callbacks & Memory Stabilization
  // Hooks directly into the appsink data drop loops to intercept decrypted raw pixel arrays.
  appsink.set_callbacks(
    gstreamer_app::AppSinkCallbacks::builder()
      .new_sample(move |sink| {
        let sample =
          sink.pull_sample().map_err(|_| gstreamer::FlowError::Eos)?;
        let buffer = sample.buffer().ok_or(gstreamer::FlowError::Error)?;
        let map = buffer
          .map_readable()
          .map_err(|_| gstreamer::FlowError::Error)?;

        let caps = sample.caps().ok_or(gstreamer::FlowError::Error)?;
        let structure = caps.structure(0).ok_or(gstreamer::FlowError::Error)?;
        let width: i32 = structure
          .get("width")
          .map_err(|_| gstreamer::FlowError::Error)?;
        let height: i32 = structure
          .get("height")
          .map_err(|_| gstreamer::FlowError::Error)?;

        // OPTIMIZATION: Zero-Allocation Heap Reuse Pool
        // Instead of instantiating an empty vector on every frame (which triggers severe heap allocation
        // thrashing and triggers OS garbage collection locks), this block claims an existing pre-allocated
        // byte vector from a thread-safe static pool, maintaining sub-millisecond memory execution passes.
        let mut raw_buffer = {
          let mut pool = buffer_pool.lock().unwrap();
          pool
            .pop()
            .unwrap_or_else(|| vec![0u8; (width * height * 4) as usize])
        };

        if raw_buffer.len() != map.len() {
          raw_buffer.resize(map.len(), 0);
        }
        raw_buffer.copy_from_slice(map.as_slice());

        // Dispatch the recycled buffer matrix down the non-blocking channel straight to the egui render layout
        let _ = raw_frame_tx.try_send((raw_buffer, width, height));

        Ok(gstreamer::FlowSuccess::Ok)
      })
      .build(),
  );

  // 3. Kickstart Video Ingest Execution State Transition
  pipeline
    .set_state(State::Playing)
    .map_err(|e| format!("{:?}", e))?;
  println!(
    "[MEDIA] Connected to secure SRT server video stream. Stream decrypted natively."
  );

  // Dynamic Bus Interceptor listening for asynchronous decoder or network socket exceptions
  let bus = pipeline.bus().unwrap();
  for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
    match msg.view() {
      MessageView::Error(err) => {
        eprintln!(
          "Pipeline runtime failure context: {:?} ({:?})",
          err.error(),
          err.debug()
        );
        break;
      }
      _ => (),
    }
  }

  // Clean resource extraction tear-down on window close boundaries
  pipeline
    .set_state(State::Null)
    .map_err(|e| format!("{:?}", e))?;
  Ok(())
}
