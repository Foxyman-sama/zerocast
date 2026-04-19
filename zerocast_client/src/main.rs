use gstreamer::prelude::*;
use gstreamer::{Caps, Element, ElementFactory, MessageView, Pipeline, State};

fn main() {
  println!("Starting ZeroCast Client...");

  gstreamer::init().expect("Failed to initialize GStreamer!");

  let source = ElementFactory::make("udpsrc")
    .name("udp_input")
    .property("port", 5000i32)
    .build()
    .expect("Failed to create udpsrc");

  let caps = Caps::builder("application/x-rtp")
    .field("media", "video")
    .field("clock-rate", 90000i32)
    .field("encoding-name", "H264")
    .build();
  source.set_property("caps", &caps);

  let depayloader = ElementFactory::make("rtph264depay")
    .name("rtp_depayloader")
    .build()
    .expect("Failed to create rtph264depay");

  let decoder = ElementFactory::make("avdec_h264")
    .name("h264_decoder")
    .build()
    .expect("Failed to create avdec_h264");

  let videoconvert = ElementFactory::make("videoconvert")
    .build()
    .expect("Failed to create videoconvert");

  let sink = ElementFactory::make("autovideosink")
    .build()
    .expect("Failed to create autovideosink");

  let pipeline = Pipeline::with_name("zerocast-receive-pipeline");
  pipeline
    .add_many([&source, &depayloader, &decoder, &videoconvert, &sink])
    .unwrap();
  Element::link_many([&source, &depayloader, &decoder, &videoconvert, &sink])
    .unwrap();

  println!("Listening for stream on UDP 5000... Waiting for server!");
  pipeline.set_state(State::Playing).unwrap();

  let bus = pipeline.bus().unwrap();
  for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
    match msg.view() {
      MessageView::Error(err) => {
        eprintln!("Error: {} ({:?})", err.error(), err.debug());
        break;
      }
      MessageView::Eos(..) => {
        println!("Stream ended.");
        break;
      }
      _ => (),
    }
  }

  pipeline.set_state(State::Null).unwrap();
}
