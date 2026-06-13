# ⚡ Zerocast

**Zerocast** is a high-performance, ultra-low latency remote desktop streaming engine implemented in Rust and GStreamer. Developed as part of a Diploma project, it focuses on real-time interactivity, utilizing hardware-accelerated encoding (NVENC) and a custom protocol for minimal click-to-photon delay.

---

## 🛠 Features
- **Zero-Touch Setup:** Fully automated environment preparation for both PowerShell and Bash.
- **Ultra-Low Latency:** Optimized for LAN environments with DXGI desktop capture.
- **Hardware Accelerated:** Native NVIDIA NVENC support.
- **Secure Control:** TLS-encrypted input replication (Mouse/Keyboard).
- **Integrated Analytics:** Real-time performance monitoring and automated reporting.

---

## 🚀 Quick Start (Windows)

The project includes an **Ultimate Setup Engine** that handles the installation of all system prerequisites including Rust, GStreamer, and OpenSSL.

### 1. Automated Installation
Open your terminal (PowerShell or Bash) **as Administrator** and run:
```powershell
python quick_setup.py
```
This script will:
- Bootstrap `winget`, **Rust**, **GStreamer**, and **OpenSSL**.
- Configure system environment variables for your specific shell.
- Generate required **SSL Certificates**.
- Prepare a local **Virtual Environment** and build the project.

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
This will conduct a 20-second live test and generate a `FINAL_REPORT.md` and `performance_graph.png`.

---

## 📂 Repository Structure
- `zerocast_server/`: Host services (DXGI/NVENC).
- `zerocast_client/`: Receiver UI (`egui`) and GStreamer playback.
- `zerocast_core/`: Shared protocol and security logic.
- `analytics.py`: Consolidated performance measurement tool.
- `quick_setup.py`: The "Ultimate Setup" installation engine.

---

## 🎓 Academic Context
This software was developed for a Diploma thesis focusing on high-performance remote rendering architectures.

## ⚖️ License
Educational use only. Part of a Diploma Thesis (2026).
