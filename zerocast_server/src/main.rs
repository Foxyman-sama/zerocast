use gstreamer::prelude::*;
use gstreamer::{Element, ElementFactory, MessageView, Pipeline, State};
use zerocast_core::test_shared_logic;

fn main() {
  println!("Starting ZeroCast Server...");
  test_shared_logic();

  gstreamer::init().expect("Failed to initialize GStreamer!");

  let source = ElementFactory::make("gdiscreencapsrc")
    .name("screen_source")
    .build()
    .expect("Failed to create gdiscreencapsrc");

  let videoconvert = ElementFactory::make("videoconvert")
    .name("converter")
    .build()
    .expect("Failed to create videoconvert");

  let sink = ElementFactory::make("autovideosink")
    .name("video_output")
    .build()
    .expect("Failed to create autovideosink");

  let pipeline = Pipeline::with_name("zerocast-capture-pipeline");
  pipeline.add_many([&source, &videoconvert, &sink]).unwrap();

  Element::link_many([&source, &videoconvert, &sink]).unwrap();

  println!("Starting screen capture... Look for a new window popping up!");
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
