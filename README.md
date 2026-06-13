# ⚡ Zerocast

**Zerocast** is a high-performance, ultra-low latency remote desktop streaming engine implemented in Rust and GStreamer. Developed as part of a Diploma project, it focuses on real-time interactivity, utilizing hardware-accelerated encoding (NVENC) and a custom protocol for minimal click-to-photon delay.

---

## 🛠 Features
- **Zero-Touch Setup:** Fully automated environment preparation.
- **Ultra-Low Latency:** Optimized for LAN environments with DXGI desktop capture.
- **Hardware Accelerated:** Native NVIDIA NVENC support.
- **Secure Control:** TLS-encrypted input replication (Mouse/Keyboard).
- **Integrated Analytics:** Real-time performance monitoring and automated reporting.

---

## 🚀 Quick Start (Windows)

The project includes a **fully automated setup script** that handles the installation of all system prerequisites including Rust, GStreamer, and OpenSSL.

### 1. Automated Installation
Open your terminal (PowerShell or CMD) **as Administrator** and run:
```powershell
python quick_setup.py
```
This script will:
1. Bootstrap `winget` (if missing).
2. Install **Rust**, **GStreamer (MSVC)**, and **OpenSSL**.
3. Configure system environment variables.
4. Generate required **SSL Certificates**.
5. Install Python dependencies and build the project.

### 2. Launch the Project
Once setup is complete, you can start the components:

**Start the Server (Host):**
```bash
cargo run -p zerocast_server
```

**Start the Client (Receiver):**
```bash
cargo run -p zerocast_client
```

---

## 📊 Analytics & Benchmarking

Zerocast includes an integrated suite for performance validation.

**Run Automated Benchmark:**
```bash
python analytics.py
```
This will conduct a 20-second live test, collecting FPS, CPU, and RAM metrics, then generate a `FINAL_REPORT.md` and `performance_graph.png`.

---

## 📂 Repository Structure
- `zerocast_server/`: Screen capture, encoding, and host services.
- `zerocast_client/`: Receiver UI (`egui`) and GStreamer playback.
- `zerocast_core/`: Shared protocol and authentication logic.
- `analytics.py`: Consolidated performance measurement tool.
- `quick_setup.py`: The "Zero-Touch" installation engine.

---

## 🎓 Academic Context
This software was developed for a Diploma thesis focusing on high-performance remote rendering architectures. It demonstrates the efficacy of GStreamer in low-latency Rust applications.

## ⚖️ License
Educational use only. Part of a Diploma Thesis (2026).
