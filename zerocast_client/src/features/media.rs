use gstreamer::prelude::*;
use gstreamer::{Caps, Element, ElementFactory, MessageView, Pipeline, State};
use std::sync::Arc;

/// Native low-overhead pipeline ingest via hardware D3D11 render utilities and secure SRT decryption.
pub fn start_gstreamer_pipeline(
  target_server_ip: String,
  raw_frame_tx: tokio::sync::mpsc::Sender<(Vec<u8>, i32, i32)>,
  buffer_pool: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
) -> Result<(), String> {
  gstreamer::init().map_err(|e| format!("GStreamer error: {:?}", e))?;

  // internal C-enum (GstSRTKeyLength) mapping instead of casting as standard gint.
  let source = ElementFactory::make("srtsrc")
    .property(
      "uri",
      format!("srt://{}:5000?mode=caller", target_server_ip),
    )
    .property("passphrase", "SuperSecureZeroCastKey2026")
    .property_from_str("pbkeylen", "16") // "16" string directly resolves into GstSRTKeyLength::Aes128
    .build()
    .unwrap();

  // Because SRT natively handles packet ordering and dropouts, rtpjitterbuffer and rtph264depay are completely bypassed.
  // Incoming network data flows directly into the h264parse element.
  let parse = ElementFactory::make("h264parse").build().unwrap();
  let queue1 = ElementFactory::make("queue")
    .property("max-size-buffers", 3u32)
    .build()
    .unwrap();
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

  appsink.set_max_buffers(1);
  appsink.set_drop(true);
  appsink.set_property("sync", false);
  appsink.set_property(
    "caps",
    &Caps::builder("video/x-raw").field("format", "RGBA").build(),
  );

  let pipeline = Pipeline::with_name("client-secure-render-pipeline");

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

  // Hook appsink callbacks directly into our lock-free buffer recycling loop to extract decrypted RGBA pixels
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

        // Recoup an existing heap vector slice from the memory reuse pool to prevent heap allocation thrashing
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

        // Send the raw RGBA frame texture back to the main UI loop thread context
        let _ = raw_frame_tx.try_send((raw_buffer, width, height));

        Ok(gstreamer::FlowSuccess::Ok)
      })
      .build(),
  );

  pipeline
    .set_state(State::Playing)
    .map_err(|e| format!("{:?}", e))?;
  println!(
    "[MEDIA] Connected to secure SRT server video stream. Stream decrypted natively."
  );

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

  pipeline
    .set_state(State::Null)
    .map_err(|e| format!("{:?}", e))?;
  Ok(())
}
