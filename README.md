# Zerocast

**Zerocast** is a high-performance, low-latency remote desktop streaming solution built with Rust and GStreamer. It is designed for near-instant screen mirroring and remote control across a local area network (LAN), utilizing hardware-accelerated encoding (NVENC) for minimal overhead.

## 🚀 Features

- **High-Performance Streaming:** Real-time desktop capture via DXGI (D3D11).
- **Low-Latency Control:** Secure TLS-encrypted remote input replication (Mouse & Keyboard).
- **Adaptive VFR:** Variable Framerate architecture to eliminate buffering delays.
- **Hardware Acceleration:** Native support for NVIDIA NVENC with fallback to OpenH264 and x264.
- **Real-Time Debug Menu:** Built-in performance overlay (F1) showing:
  - Frame Rate (FPS).
  - Network Latency (Click-to-Photon estimation).
  - Server CPU & GPU (NVENC) utilization.
  - Memory Footprint.
- **Secure Authentication:** Dynamic credential generation for each session.

## 🛠 Prerequisites

Before running the project, ensure you have the following installed:

1. **Rust:** [Install Rust](https://rustup.rs/) (edition 2024 recommended).
2. **GStreamer:** Install GStreamer (MSVC 64-bit) for Windows. 
   - [Download GStreamer](https://gstreamer.freedesktop.org/download/#windows)
   - Ensure `bin`, `lib`, and `include` paths are added to your environment variables or use the `setup.sh` script.
3. **OpenSSL:** Required for secure TLS connections.
4. **NVIDIA GPU (Optional):** For hardware-accelerated encoding (RTX series recommended).

## 📥 Setup

1. **Clone the repository:**
   ```bash
   git clone https://github.com/your-repo/zerocast.git
   cd zerocast
   ```

2. **Generate Security Certificate:**
   The secure input service requires a PKCS#12 certificate. You can generate one using OpenSSL in the `zerocast_server` directory:
   ```bash
   openssl genrsa -out key.pem 2048; 
   openssl req -new -x509 -key key.pem -out cert.pem -days 365 -subj "/CN=zerocast"; 
   openssl pkcs12 -export -out identity.p12 -inkey key.pem -in cert.pem -password pass:zerocast; 
   rm key.pem cert.pem
   ```

## 🎮 Running the Project

### 1. Start the Server
Run the server on the machine you want to share:
```bash
cargo run -p zerocast_server
```
The server will print its LAN IP and temporary credentials (Login/Password).

### 2. Start the Client
Run the client on your local machine:
```bash
cargo run -p zerocast_client
```
- Enter the **Server IP**.
- Use the **Login** and **Password** provided by the server console.
- **Press F1** once connected to toggle the performance debug menu.

## 📊 Performance Testing

The project includes an automated benchmarking suite to generate performance reports.

### Run Benchmarks:
```bash
# Generate the experimental report (Markdown)
cargo test -p zerocast_server --test performance_metrics -- --nocapture
```
The results will be updated in `zerocast_server/REPORT.md`.

### Real Data Collection:
You can use the provided Python script to collect metrics from a live session:
```bash
python collect_bench_data.py
```

## 📂 Project Structure

- `zerocast_server/`: Host-side capture and encoding engine.
- `zerocast_client/`: Receiver side with `egui` interface and GStreamer playback.
- `zerocast_core/`: Shared protocol definitions for auth and input.

## ⚖️ License
This project is part of a Diploma work and is provided for educational purposes.
