use eframe::egui;
use gstreamer::prelude::*;
use gstreamer::{Caps, Element, ElementFactory, MessageView, Pipeline, State};
use gstreamer_app::AppSink;
use std::sync::Arc;
use tokio::sync::mpsc;

mod features;
mod shared;

use crate::shared::events::{AuthResult, SystemEvent, UiMessage};
use features::auth::interactor::run_auth_interactor;

#[tokio::main]
async fn main() -> eframe::Result<()> {
  let native_options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default()
      .with_inner_size([1280.0, 720.0])
      .with_title("ZeroCast Client"),
    ..Default::default()
  };

  let (ui_tx, ui_rx) = mpsc::channel::<UiMessage>(10);
  let (system_tx, system_rx) = mpsc::channel::<SystemEvent>(1);
  let (frame_tx, frame_rx) = mpsc::channel::<egui::ColorImage>(2);

  let auth_status = Arc::new(tokio::sync::Mutex::new(AuthResult::Error(
    "Please log in to establish a secure stream link.".to_string(),
  )));

  let auth_status_clone = Arc::clone(&auth_status);
  tokio::spawn(async move {
    run_auth_interactor(ui_rx, system_tx, auth_status_clone).await;
  });

  eframe::run_native(
    "ZeroCast Client",
    native_options,
    Box::new(|cc| {
      let ctx_clone = cc.egui_ctx.clone();

      tokio::spawn(async move {
        let mut system_rx = system_rx;
        if let Some(SystemEvent::AuthSuccess) = system_rx.recv().await {
          tokio::task::spawn_blocking(move || {
            if let Err(e) = start_gstreamer_pipeline(frame_tx, ctx_clone) {
              eprintln!("GStreamer pipeline execution failure: {}", e);
            }
          });
        }
      });

      Box::new(ZeroCastApp::new(cc, ui_tx, auth_status, frame_rx))
    }),
  )
}

struct ZeroCastApp {
  login_input: String,
  password_input: String,
  ui_tx: mpsc::Sender<UiMessage>,
  auth_status: Arc<tokio::sync::Mutex<AuthResult>>,
  frame_rx: mpsc::Receiver<egui::ColorImage>,
  video_texture: Option<egui::TextureHandle>,
}

impl ZeroCastApp {
  fn new(
    _cc: &eframe::CreationContext<'_>,
    ui_tx: mpsc::Sender<UiMessage>,
    auth_status: Arc<tokio::sync::Mutex<AuthResult>>,
    frame_rx: mpsc::Receiver<egui::ColorImage>,
  ) -> Self {
    Self {
      login_input: String::new(),
      password_input: String::new(),
      ui_tx,
      auth_status,
      frame_rx,
      video_texture: None,
    }
  }
}

impl eframe::App for ZeroCastApp {
  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    let mut latest_frame = None;
    while let Ok(frame) = self.frame_rx.try_recv() {
      latest_frame = Some(frame);
    }

    if let Some(image) = latest_frame {
      if let Some(texture) = &mut self.video_texture {
        texture.set(image, egui::TextureOptions::LINEAR);
      } else {
        self.video_texture = Some(ctx.load_texture(
          "remote-video-frame",
          image,
          egui::TextureOptions::LINEAR,
        ));
      }
    }

    let current_state = if let Ok(guard) = self.auth_status.try_lock() {
      guard.clone()
    } else {
      AuthResult::Pending
    };

    egui::CentralPanel::default().show(ctx, |ui| match current_state {
      AuthResult::Success => {
        if let Some(texture) = &self.video_texture {
          ui.with_layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
              ui.add(egui::Image::from_texture(texture).shrink_to_fit());
            },
          );
        } else {
          ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
              ui.add(egui::Spinner::new().size(40.0));
              ui.add(egui::Label::new("Connecting to media stream..."));
            });
          });
        }
      }
      _ => {
        ui.centered_and_justified(|ui| {
          ui.set_max_width(320.0);
          ui.vertical_centered(|ui| {
            ui.heading("ZeroCast Remote Authorization");
            ui.add_space(20.0);

            ui.horizontal(|ui| {
              ui.label("Login:    ");
              ui.text_edit_singleline(&mut self.login_input);
            });
            ui.add_space(8.0);

            ui.horizontal(|ui| {
              ui.label("Password: ");
              ui.add(
                egui::TextEdit::singleline(&mut self.password_input)
                  .password(true),
              );
            });
            ui.add_space(15.0);

            match &current_state {
              AuthResult::Pending => {
                ui.add(egui::Spinner::new());
                ui.label("Verifying security parameters...");
              }
              AuthResult::Error(reason) => {
                ui.colored_label(egui::Color32::LIGHT_RED, reason);
                ui.add_space(10.0);
                if ui.button("Establish Connection").clicked() {
                  let _ = self.ui_tx.try_send(UiMessage::AuthRequest(
                    self.login_input.clone(),
                    self.password_input.clone(),
                  ));
                }
              }
              _ => {}
            }
          });
        });
      }
    });
  }
}

fn start_gstreamer_pipeline(
  frame_tx: mpsc::Sender<egui::ColorImage>,
  ctx: egui::Context,
) -> Result<(), String> {
  gstreamer::init().map_err(|e| format!("GStreamer init error: {:?}", e))?;

  let source = ElementFactory::make("udpsrc")
    .property("port", 5000i32)
    .property("buffer-size", 20_971_520i32)
    .build()
    .map_err(|e| format!("Failed to create udpsrc: {:?}", e))?;

  let caps = Caps::builder("application/x-rtp")
    .field("media", "video")
    .field("clock-rate", 90000i32)
    .field("encoding-name", "H264")
    .field("payload", 96i32)
    .build();
  source.set_property("caps", &caps);

  let jitterbuffer = ElementFactory::make("rtpjitterbuffer")
    .property("latency", 60u32)
    .property("drop-on-latency", true)
    .property("do-lost", true)
    .build()
    .unwrap();

  let depay = ElementFactory::make("rtph264depay").build().unwrap();
  let parse = ElementFactory::make("h264parse").build().unwrap();

  let decode = ElementFactory::make("d3d11h264dec").build().unwrap();
  let gpu_convert = ElementFactory::make("d3d11convert").build().unwrap();
  let download = ElementFactory::make("d3d11download").build().unwrap();

  let convert = ElementFactory::make("videoconvert")
    .property("n-threads", 4u32)
    .build()
    .unwrap();

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

  let pipeline = Pipeline::with_name("client-render-pipeline");

  pipeline
    .add_many([
      &source,
      &jitterbuffer,
      &depay,
      &parse,
      &decode,
      &gpu_convert,
      &download,
      &convert,
      appsink.upcast_ref(),
    ])
    .unwrap();

  Element::link_many([
    &source,
    &jitterbuffer,
    &depay,
    &parse,
    &decode,
    &gpu_convert,
    &download,
    &convert,
    appsink.upcast_ref(),
  ])
  .unwrap();

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

        let color_image = egui::ColorImage::from_rgba_unmultiplied(
          [width as usize, height as usize],
          map.as_slice(),
        );

        let _ = frame_tx.try_send(color_image);
        ctx.request_repaint();

        Ok(gstreamer::FlowSuccess::Ok)
      })
      .build(),
  );

  pipeline
    .set_state(State::Playing)
    .map_err(|e| format!("{:?}", e))?;

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
