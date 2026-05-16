use eframe::egui;
use gstreamer::prelude::*;
use gstreamer::{Caps, Element, ElementFactory, MessageView, Pipeline, State};
use gstreamer_app::AppSink;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

mod features;
mod shared;

use crate::shared::events::{AuthResult, SystemEvent, UiMessage};
use features::auth::interactor::run_auth_interactor;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RemoteInput {
  MouseMove { x: f32, y: f32 },
  MouseDown { button: String },
  MouseUp { button: String },
}

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

  let (frame_tx, frame_rx) = mpsc::channel::<(Vec<u8>, i32, i32)>(2);
  let (input_tx, input_rx) = mpsc::channel::<RemoteInput>(100);

  let auth_status = Arc::new(tokio::sync::Mutex::new(AuthResult::Error(
    "Please log in to establish a secure stream link.".to_string(),
  )));

  let auth_status_clone = Arc::clone(&auth_status);
  tokio::spawn(async move {
    run_auth_interactor(ui_rx, system_tx, auth_status_clone).await;
  });

  tokio::spawn(async move {
    let mut input_rx = input_rx;
    if let Ok(mut stream) =
      tokio::net::TcpStream::connect("127.0.0.1:8081").await
    {
      while let Some(event) = input_rx.recv().await {
        if let Ok(mut json_str) = serde_json::to_string(&event) {
          json_str.push('\n');
          let _ = stream.write_all(json_str.as_bytes()).await;
        }
      }
    }
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

      Box::new(ZeroCastApp::new(cc, ui_tx, auth_status, frame_rx, input_tx))
    }),
  )
}

struct ZeroCastApp {
  login_input: String,
  password_input: String,
  ui_tx: mpsc::Sender<UiMessage>,
  auth_status: Arc<tokio::sync::Mutex<AuthResult>>,
  frame_rx: mpsc::Receiver<(Vec<u8>, i32, i32)>,
  video_texture: Option<egui::TextureHandle>,
  input_tx: mpsc::Sender<RemoteInput>,
}

impl ZeroCastApp {
  fn new(
    _cc: &eframe::CreationContext<'_>,
    ui_tx: mpsc::Sender<UiMessage>,
    auth_status: Arc<tokio::sync::Mutex<AuthResult>>,
    frame_rx: mpsc::Receiver<(Vec<u8>, i32, i32)>,
    input_tx: mpsc::Sender<RemoteInput>,
  ) -> Self {
    Self {
      login_input: String::new(),
      password_input: String::new(),
      ui_tx,
      auth_status,
      frame_rx,
      video_texture: None,
      input_tx,
    }
  }
}

impl eframe::App for ZeroCastApp {
  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    let mut latest_raw_data = None;
    while let Ok(raw_payload) = self.frame_rx.try_recv() {
      latest_raw_data = Some(raw_payload);
    }

    if let Some((data, width, height)) = latest_raw_data {
      let image = egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        &data,
      );

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
              let image_resp = ui.add(
                egui::Image::from_texture(texture)
                  .shrink_to_fit()
                  .sense(egui::Sense::click_and_drag()),
              );

              if let Some(hover_pos) = image_resp.hover_pos() {
                let rect = image_resp.rect;
                let norm_x = (hover_pos.x - rect.min.x) / rect.width();
                let norm_y = (hover_pos.y - rect.min.y) / rect.height();

                if (0.0..=1.0).contains(&norm_x)
                  && (0.0..=1.0).contains(&norm_y)
                {
                  let _ = self.input_tx.try_send(RemoteInput::MouseMove {
                    x: norm_x,
                    y: norm_y,
                  });
                }
              }

              if image_resp.hovered() || image_resp.dragged() {
                ctx.input(|i| {
                  if i.pointer.button_pressed(egui::PointerButton::Primary) {
                    let _ = self.input_tx.try_send(RemoteInput::MouseDown {
                      button: "left".to_string(),
                    });
                  }
                  if i.pointer.button_released(egui::PointerButton::Primary) {
                    let _ = self.input_tx.try_send(RemoteInput::MouseUp {
                      button: "left".to_string(),
                    });
                  }
                });
              }
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
  frame_tx: mpsc::Sender<(Vec<u8>, i32, i32)>,
  ctx: egui::Context,
) -> Result<(), String> {
  gstreamer::init().map_err(|e| format!("GStreamer init error: {:?}", e))?;

  let source = ElementFactory::make("udpsrc")
    .property("port", 5000i32)
    .property("buffer-size", 41_943_040i32)
    .property("do-timestamp", true)
    .build()
    .map_err(|e| format!("Failed to create udpsrc: {:?}", e))?;

  let caps = Caps::builder("application/x-rtp")
    .field("media", "video")
    .field("clock-rate", 90000i32)
    .field("encoding-name", "H264")
    .field("payload", 96i32)
    .build();
  source.set_property("caps", &caps);

  let queue1 = ElementFactory::make("queue")
    .property("max-size-buffers", 5u32)
    .build()
    .unwrap();

  let jitterbuffer = ElementFactory::make("rtpjitterbuffer")
    .property("latency", 33u32) // Set to 33ms (exactly 2 frames at 60 FPS) to maintain a smooth packet queue
    .property("drop-on-latency", true)
    .property("do-lost", true)
    .build()
    .unwrap();

  let depay = ElementFactory::make("rtph264depay").build().unwrap();
  let parse = ElementFactory::make("h264parse").build().unwrap();

  let queue2 = ElementFactory::make("queue")
    .property("max-size-buffers", 5u32)
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

  let pipeline = Pipeline::with_name("client-render-pipeline");

  pipeline
    .add_many([
      &source,
      &queue1,
      &jitterbuffer,
      &depay,
      &parse,
      &queue2,
      &decode,
      &gpu_convert,
      &client_gpu_caps,
      &download,
      appsink.upcast_ref(),
    ])
    .unwrap();

  Element::link_many([
    &source,
    &queue1,
    &jitterbuffer,
    &depay,
    &parse,
    &queue2,
    &decode,
    &gpu_convert,
    &client_gpu_caps,
    &download,
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

        let _ = frame_tx.try_send((map.as_slice().to_vec(), width, height));
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
