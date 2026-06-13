use super::input::{RemoteInput, ServerTelemetry};
use crate::shared::events::{AuthResult, UiMessage};
use eframe::egui;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;

struct MetricTracker {
  min: f32,
  max: f32,
  sum: f32,
  count: usize,
}

impl MetricTracker {
  fn new() -> Self {
    Self {
      min: f32::MAX,
      max: f32::MIN,
      sum: 0.0,
      count: 0,
    }
  }
  fn update(&mut self, val: f32) {
    if val < self.min {
      self.min = val;
    }
    if val > self.max {
      self.max = val;
    }
    self.sum += val;
    self.count += 1;
  }
  fn avg(&self) -> f32 {
    if self.count == 0 {
      0.0
    } else {
      self.sum / self.count as f32
    }
  }
}

pub struct ZeroCastApp {
  pub ip_input: String,
  pub login_input: String,
  pub password_input: String,
  ui_tx: tokio::sync::mpsc::Sender<UiMessage>,
  auth_status: Arc<tokio::sync::Mutex<AuthResult>>,
  frame_rx: tokio::sync::mpsc::Receiver<egui::ColorImage>,
  video_texture: Option<egui::TextureHandle>,
  input_tx: tokio::sync::mpsc::Sender<RemoteInput>,

  latency_rx: tokio::sync::mpsc::Receiver<f64>,
  telemetry_rx: tokio::sync::mpsc::Receiver<ServerTelemetry>,

  show_debug: bool,
  latency_track: MetricTracker,
  cpu_track: MetricTracker,
  gpu_track: MetricTracker,
  ram_track: MetricTracker,

  current_latency: f64,
  fps_counter: usize,
  fps_timer: std::time::Instant,
  current_fps: usize,
  shared_ip: Arc<std::sync::Mutex<String>>,
  last_modifiers: egui::Modifiers,
}

impl ZeroCastApp {
  pub fn new(
    ui_tx: tokio::sync::mpsc::Sender<UiMessage>,
    auth_status: Arc<tokio::sync::Mutex<AuthResult>>,
    frame_rx: tokio::sync::mpsc::Receiver<egui::ColorImage>,
    input_tx: tokio::sync::mpsc::Sender<RemoteInput>,
    latency_rx: tokio::sync::mpsc::Receiver<f64>,
    telemetry_rx: tokio::sync::mpsc::Receiver<ServerTelemetry>,
    shared_ip: Arc<std::sync::Mutex<String>>,
  ) -> Self {
    Self {
      ip_input: "127.0.0.1".to_string(),
      login_input: String::new(),
      password_input: String::new(),
      ui_tx,
      auth_status,
      frame_rx,
      video_texture: None,
      input_tx,
      latency_rx,
      telemetry_rx,
      show_debug: false,
      latency_track: MetricTracker::new(),
      cpu_track: MetricTracker::new(),
      gpu_track: MetricTracker::new(),
      ram_track: MetricTracker::new(),
      current_latency: 0.0,
      fps_counter: 0,
      fps_timer: std::time::Instant::now(),
      current_fps: 0,
      shared_ip,
      last_modifiers: egui::Modifiers::default(),
    }
  }
}

impl eframe::App for ZeroCastApp {
  fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
    println!(
      "[CLIENT] Window close detected. Terminating background threads safely."
    );
    // Terminate the process instantly to clean up asynchronous worker channels
    std::process::exit(0);
  }

  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    super::input::handle_client_keyboard_input(
      ctx,
      &self.input_tx,
      &mut self.last_modifiers,
    );

    if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
      self.show_debug = !self.show_debug;
    }

    // 1. Process inbound telemetry signals from network background jobs
    while let Ok(latency) = self.latency_rx.try_recv() {
      self.current_latency = latency;
      self.latency_track.update(latency as f32);
    }

    while let Ok(telemetry) = self.telemetry_rx.try_recv() {
      self.cpu_track.update(telemetry.cpu_usage);
      self.gpu_track.update(telemetry.gpu_usage);
      self.ram_track.update(telemetry.ram_usage_mb);

      let client_ram = get_client_ram();
      append_telemetry_to_csv(
        self.current_latency,
        self.current_fps,
        telemetry.cpu_usage,
        telemetry.gpu_usage,
        telemetry.ram_usage_mb,
        client_ram,
      );
    }

    let mut latest_frame = None;
    while let Ok(frame) = self.frame_rx.try_recv() {
      latest_frame = Some(frame);
      self.fps_counter += 1;
    }

    if self.fps_timer.elapsed().as_secs() >= 1 {
      self.current_fps = self.fps_counter;
      self.fps_counter = 0;
      self.fps_timer = std::time::Instant::now();
    }

    // 2. Commit the active color matrix straight into VRAM handles
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

    // 3. Render View Panels based on authorization status
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

              // Capture relative coordinates and translate into global input packets
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

          // Transparent Heads Up Display Overlay
          egui::Area::new(egui::Id::new("telemetry_hud"))
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(15.0, 15.0))
            .show(ctx, |ui| {
              egui::Frame::none()
                .fill(egui::Color32::from_black_alpha(160))
                .rounding(5.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                  ui.colored_label(
                    egui::Color32::LIGHT_GREEN,
                    format!("FPS: {}", self.current_fps),
                  );
                  ui.colored_label(
                    egui::Color32::LIGHT_BLUE,
                    format!("NET: {:.1} ms", self.current_latency),
                  );
                  ui.add_space(4.0);
                  ui.small("Press F1 for Debug Menu");
                });
            });

          if self.show_debug {
            egui::Window::new("🗠 System Performance Debug")
              .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-15.0, 15.0))
              .resizable(false)
              .collapsible(false)
              .title_bar(true)
              .show(ctx, |ui| {
                ui.set_min_width(260.0);

                egui::Grid::new("debug_grid")
                  .num_columns(4)
                  .spacing([10.0, 4.0])
                  .striped(true)
                  .show(ui, |ui| {
                    ui.label("");
                    ui.label("MIN");
                    ui.label("AVG");
                    ui.label("MAX");
                    ui.end_row();

                    fn show_row(
                      ui: &mut egui::Ui,
                      label: &str,
                      track: &MetricTracker,
                      unit: &str,
                    ) {
                      ui.label(label);
                      if track.count > 0 {
                        ui.label(format!("{:.1}{}", track.min, unit));
                        ui.label(format!("{:.1}{}", track.avg(), unit));
                        ui.label(format!("{:.1}{}", track.max, unit));
                      } else {
                        ui.label("-");
                        ui.label("-");
                        ui.label("-");
                      }
                      ui.end_row();
                    }

                    show_row(ui, "Latency", &self.latency_track, "ms");
                    show_row(ui, "Srv CPU", &self.cpu_track, "%");
                    show_row(ui, "Srv GPU", &self.gpu_track, "%");
                    show_row(ui, "Srv RAM", &self.ram_track, "MB");
                  });

                ui.add_space(10.0);
                if ui.button("Reset Statistics").clicked() {
                  self.latency_track = MetricTracker::new();
                  self.cpu_track = MetricTracker::new();
                  self.gpu_track = MetricTracker::new();
                  self.ram_track = MetricTracker::new();
                }
              });
          }
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
              ui.label("Server IP: ");
              ui.text_edit_singleline(&mut self.ip_input);
            });
            ui.add_space(8.0);

            ui.horizontal(|ui| {
              ui.label("Login:     ");
              ui.text_edit_singleline(&mut self.login_input);
            });
            ui.add_space(8.0);

            ui.horizontal(|ui| {
              ui.label("Password:  ");
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
                  if let Ok(mut guard) = self.shared_ip.lock() {
                    *guard = self.ip_input.trim().to_string();
                  }

                  let _ = self.ui_tx.try_send(UiMessage::AuthRequest(
                    self.ip_input.trim().to_string(),
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

fn get_client_ram() -> f32 {
  let mut sys = sysinfo::System::new_all();
  sys.refresh_all();
  let pid = sysinfo::Pid::from(std::process::id() as usize);
  if let Some(process) = sys.process(pid) {
    process.memory() as f32 / 1024.0 / 1024.0
  } else {
    0.0
  }
}

/// Appends raw live telemetry fields directly into a local CSV storage tract
fn append_telemetry_to_csv(
  latency: f64,
  fps: usize,
  cpu: f32,
  gpu: f32,
  srv_ram: f32,
  cli_ram: f32,
) {
  if let Ok(mut file) = OpenOptions::new()
    .create(true)
    .append(true)
    .open("client_metrics.csv")
  {
    // Check if the file is new to dynamically write the headers
    if file.metadata().map(|m| m.len() == 0).unwrap_or(false) {
      let _ = writeln!(
        file,
        "Latency_MS,FPS,Srv_CPU_Pct,Srv_GPU_Pct,Srv_RAM_MB,Cli_RAM_MB"
      );
    }
    // Save the execution data snapshot
    let _ = writeln!(
      file,
      "{:.2},{},{:.1},{:.1},{:.1},{:.1}",
      latency, fps, cpu, gpu, srv_ram, cli_ram
    );
  }
}
