use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;
use tokio::time::sleep;
use zerocast_server::features::media::run_media_pipeline;

#[tokio::test]
async fn run_real_pipeline_for_benchmarking() {
  let local_target_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

  println!("[BENCH] Starting real GStreamer pipeline on 127.0.0.1:5000...");
  
  // Run the pipeline in a blocking thread
  let _ = tokio::task::spawn_blocking(move || {
    run_media_pipeline(local_target_ip)
  });

  // Keep it alive for 30 seconds to allow for external benchmarking
  sleep(Duration::from_secs(30)).await;
  println!("[BENCH] Pipeline test window closed.");
}
