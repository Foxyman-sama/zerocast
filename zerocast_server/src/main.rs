// Import
use gstreamer::prelude::*;
use gstreamer::{Element, ElementFactory, MessageView, Pipeline, State};
use zerocast_core::test_shared_logic;

fn main() {
  println!("Starting ZeroCast Server...");
  test_shared_logic();

  gstreamer::init().expect("Failed to initialize GStreamer!");

  let source = ElementFactory::make("d3d11screencapturesrc")
    .name("screen_source")
    .build()
    .expect("Failed to create d3d11screencapturesrc");

  let videoconvert = ElementFactory::make("videoconvert")
    .name("converter")
    .build()
    .expect("Failed to create videoconvert");

  let encoder = ElementFactory::make("x264enc")
    .name("h264_encoder")
    .property_from_str("tune", "zerolatency")
    .property_from_str("speed-preset", "ultrafast")
    .build()
    .expect("Failed to create x264enc");

  let payloader = ElementFactory::make("rtph264pay")
    .name("rtp_payloader")
    .build()
    .expect("Failed to create rtph264pay");

  let sink = ElementFactory::make("udpsink")
    .name("udp_output")
    .property("host", "127.0.0.1")
    .property("port", 5000i32)
    .build()
    .expect("Failed to create udpsink");

  let pipeline = Pipeline::with_name("zerocast-capture-pipeline");
  pipeline
    .add_many([&source, &videoconvert, &encoder, &payloader, &sink])
    .unwrap();

  Element::link_many([&source, &videoconvert, &encoder, &payloader, &sink])
    .unwrap();

  println!(
    "Streaming started! Sending H.264 RTP packets to UDP 127.0.0.1:5000..."
  );
  pipeline.set_state(State::Playing).unwrap();

  let bus = pipeline.bus().unwrap();
  for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
    match msg.view() {
      MessageView::Error(err) => {
        eprintln!("Error: {} ({:?})", err.error(), err.debug());
        break;
      }
      MessageView::Eos(..) => break,
      _ => (),
    }
  }

  pipeline.set_state(State::Null).unwrap();
}
