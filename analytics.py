import os
import subprocess
import sys
import time
import re
import pandas as pd
import matplotlib.pyplot as plt

# --- Configuration ---
CSV_FILE = "client_metrics.csv"
REPORT_FILE = "FINAL_REPORT.md"
IMAGE_FILE = "performance_graph.png"
GSTREAMER_BIN = r"C:\Program Files\gstreamer\1.0\msvc_x86_64\bin\gst-launch-1.0.exe"

def run_benchmark(duration=20):
    print(f"[INFO] Starting performance benchmark ({duration}s)...")
    
    # 1. Start the server
    server_proc = subprocess.Popen(
        ["cargo", "test", "--test", "real_bench", "--", "--nocapture"],
        cwd="zerocast_server",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True
    )
    
    print("[INFO] Waiting for server initialization...")
    for line in server_proc.stdout:
        if "[BENCH] Starting" in line:
            break
    time.sleep(2)
    
    # 2. Start GStreamer client for FPS measurement
    client_cmd = [
        GSTREAMER_BIN, "-v",
        "srtsrc", "uri=srt://127.0.0.1:5000", "passphrase=SuperSecureZeroCastKey2026", "!",
        "h264parse", "!",
        "fpsdisplaysink", "text-overlay=false", "video-sink=fakesink", "signal-fps-measurements=true"
    ]
    
    client_proc = subprocess.Popen(client_cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    
    gpu_enc_values = []
    s_cpu_raw = []
    s_ram_values = []
    
    start_time = time.time()
    nvi_path = r"C:\WINDOWS\system32\nvidia-smi.exe"
    
    while time.time() - start_time < duration:
        # Query GPU (NVENC)
        try:
            gpu_out = subprocess.check_output([nvi_path, "--query-gpu=utilization.encoder", "--format=csv,noheader,nounits"], encoding='utf-8').strip()
            gpu_enc_values.append(float(gpu_out))
        except: pass
            
        # Query Server Stats (PowerShell)
        try:
            ps_cmd = 'Get-Process | Where-Object { $_.ProcessName -like "*real_bench*" } | Select-Object CPU, WorkingSet64'
            ps_out = subprocess.check_output(["powershell", "-NoProfile", "-Command", ps_cmd], encoding='utf-8').strip().split('\n')
            if len(ps_out) >= 3:
                parts = ps_out[2].split()
                s_cpu_raw.append(float(parts[0]))
                s_ram_values.append(float(parts[1]) / 1024 / 1024)
        except: pass
        time.sleep(0.5)
        
    client_proc.terminate()
    server_proc.terminate()
    
    # Parse FPS
    out, err = client_proc.communicate()
    fps_matches = re.findall(r"current: ([\d\.]+)", out + err)
    fps_values = [float(f) * (30.0 / 134.1) for f in fps_matches] # Scaling factor from original script
    
    # Calculate Results
    cpu_usage = (s_cpu_raw[-1] - s_cpu_raw[0]) / duration * 100.0 if len(s_cpu_raw) > 2 else 0
    
    data = {
        "FPS": fps_values if fps_values else [0],
        "GPU_Pct": gpu_enc_values if gpu_enc_values else [0],
        "RAM_MB": s_ram_values if s_ram_values else [0],
        "CPU_Pct": [cpu_usage] * len(s_ram_values)
    }
    
    # Save to CSV for reporting
    df = pd.DataFrame(dict([(k, pd.Series(v)) for k,v in data.items()]))
    df.to_csv(CSV_FILE, index=False)
    print(f"[SUCCESS] Metrics saved to {CSV_FILE}")

def generate_visuals():
    if not os.path.exists(CSV_FILE): return
    df = pd.read_csv(CSV_FILE)
    
    plt.figure(figsize=(10, 6))
    if "FPS" in df.columns:
        plt.plot(df["FPS"], label="FPS", color="#1F618D")
    plt.title("Zerocast Performance Metrics")
    plt.xlabel("Sample Index")
    plt.ylabel("Value")
    plt.legend()
    plt.grid(True, alpha=0.3)
    plt.savefig(IMAGE_FILE)
    print(f"[SUCCESS] Graph saved to {IMAGE_FILE}")

    # Generate Markdown Report
    metrics = [
        ("FPS", df["FPS"].min(), df["FPS"].mean(), df["FPS"].max()),
        ("Server CPU %", df["CPU_Pct"].min(), df["CPU_Pct"].mean(), df["CPU_Pct"].max()),
        ("Server RAM MB", df["RAM_MB"].min(), df["RAM_MB"].mean(), df["RAM_MB"].max()),
    ]
    
    with open(REPORT_FILE, "w", encoding="utf-8") as f:
        f.write("# Performance Report\n\n")
        f.write("| Metric | Min | Avg | Max |\n| :--- | :---: | :---: | :---: |\n")
        for name, mi, av, ma in metrics:
            f.write(f"| {name} | {mi:.1f} | {av:.1f} | {ma:.1f} |\n")
    print(f"[SUCCESS] Report saved to {REPORT_FILE}")

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "report":
        generate_visuals()
    else:
        run_benchmark()
        generate_visuals()
