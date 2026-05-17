use eframe::egui;
use std::sync::Arc;
use tokio::sync::mpsc;

mod features;
mod shared;

use crate::shared::events::{AuthResult, SystemEvent, UiMessage};
use features::auth::interactor::run_auth_interactor;
use features::input::run_input_service;
use features::media::start_gstreamer_pipeline;
use features::ui::ZeroCastApp;

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
  let (ui_frame_tx, ui_frame_rx) = mpsc::channel::<egui::ColorImage>(2);
  let (input_tx, input_rx) = mpsc::channel::<features::input::RemoteInput>(100);
  let (latency_tx, latency_rx) = mpsc::channel::<f64>(10);

  let server_ip = Arc::new(std::sync::Mutex::new("127.0.0.1".to_string()));

  let auth_status = Arc::new(tokio::sync::Mutex::new(AuthResult::Error(
    "Please log in to establish a secure stream link.".to_string(),
  )));

  // 1. Kickstart async auth interactor loop
  let auth_status_clone = Arc::clone(&auth_status);
  tokio::spawn(async move {
    run_auth_interactor(ui_rx, system_tx, auth_status_clone).await;
  });

  let server_ip_for_eframe = Arc::clone(&server_ip);

  // 2. Hand off control loop flow over to native Eframe GUI context handles
  eframe::run_native(
    "ZeroCast Client",
    native_options,
    Box::new(move |cc| {
      let ctx_clone = cc.egui_ctx.clone();
      let (raw_frame_tx, mut raw_frame_rx) =
        mpsc::channel::<(Vec<u8>, i32, i32)>(2);

      let buffer_pool = Arc::new(std::sync::Mutex::new(vec![
        vec![0u8; 1920 * 1080 * 4],
        vec![0u8; 1920 * 1080 * 4],
        vec![0u8; 1920 * 1080 * 4],
      ]));

      // Sequential pixel allocation background worker handles heavy lifting transformations
      let ui_frame_tx_clone = ui_frame_tx.clone();
      let pool_for_worker = Arc::clone(&buffer_pool);
      let ctx_for_worker = ctx_clone.clone();

      tokio::spawn(async move {
        while let Some((raw_data, width, height)) = raw_frame_rx.recv().await {
          let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            &raw_data,
          );

          {
            let mut pool = pool_for_worker.lock().unwrap();
            if pool.len() < 3 {
              pool.push(raw_data);
            }
          }

          let _ = ui_frame_tx_clone.try_send(color_image);
          ctx_for_worker.request_repaint();
        }
      });

      // System Bus Coordinator: Activates media streams and inputs only after authentication completes
      let server_ip_coordinator = Arc::clone(&server_ip_for_eframe);
      tokio::spawn(async move {
        let mut system_rx = system_rx;

        if let Some(SystemEvent::AuthSuccess) = system_rx.recv().await {
          let target_host = {
            let guard = server_ip_coordinator.lock().unwrap();
            guard.clone()
          };

          println!(
            "[CORE] Interactor confirmed success. Orchestrating links to: {}",
            target_host
          );

          // Dynamic instantiation of remote control pipeline tasks
          tokio::spawn(async move {
            run_input_service(target_host, input_rx, latency_tx).await;
          });

          // Offload synchronous blocking GStreamer bus handlers out of Tokio's primary executors
          tokio::task::spawn_blocking(move || {
            if let Err(e) = start_gstreamer_pipeline(raw_frame_tx, buffer_pool)
            {
              eprintln!("GStreamer pipeline execution failure: {}", e);
            }
          });
        }
      });

      Box::new(ZeroCastApp::new(
        ui_tx,
        auth_status,
        ui_frame_rx,
        input_tx,
        latency_rx,
        server_ip_for_eframe,
      ))
    }),
  )
}
