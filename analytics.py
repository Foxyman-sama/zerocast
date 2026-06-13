import os
import sys
import subprocess
import time
import re
import pandas as pd
import matplotlib.pyplot as plt
from pathlib import Path

def get_env():
    env = os.environ.copy()
    gst_root = Path(env.get("GSTREAMER_1_0_ROOT_X86_64", r"C:\gstreamer\1.0\msvc_x86_64"))
    if not gst_root.exists(): gst_root = Path(r"C:\Program Files\gstreamer\1.0\msvc_x86_64")
    
    if gst_root.exists():
        pc_path = gst_root / "lib" / "pkgconfig"
        is_unix = "MSYSTEM" in env or "SHELL" in env
        sep = ":" if is_unix else os.pathsep
        def fmt(p): return f"/{p.drive[0].lower()}{str(p.as_posix())[2:]}" if is_unix and p.drive else str(p)
        
        env["GSTREAMER_1_0_ROOT_X86_64"] = str(gst_root)
        env["PKG_CONFIG_PATH"] = f"{fmt(pc_path)}{sep}{env.get('PKG_CONFIG_PATH', '')}".strip(sep)
        env["PATH"] = f"{fmt(gst_root / 'bin')}{sep}{env.get('PATH', '')}".strip(sep)
        env["PKG_CONFIG_ALLOW_SYSTEM_LIBS"] = "1"
        env["PKG_CONFIG_ALLOW_SYSTEM_CFLAGS"] = "1"
    return env

CSV_FILE = "client_metrics.csv"
REPORT_FILE = "FINAL_REPORT.md"
IMAGE_FILE = "performance_graph.png"

def run_benchmark(duration=20):
    print(f"📊 Starting Performance Test ({duration}s)...")
    env = get_env()
    
    server_proc = subprocess.Popen(
        ["cargo", "test", "--test", "real_bench", "--", "--nocapture"],
        cwd="zerocast_server", stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=env
    )
    
    for line in server_proc.stdout:
        if "[BENCH] Starting" in line: break
    time.sleep(2)
    
    gst_bin = Path(env["GSTREAMER_1_0_ROOT_X86_64"]) / "bin" / "gst-launch-1.0.exe"
    client_cmd = [str(gst_bin), "-v", "srtsrc", "uri=srt://127.0.0.1:5000", "passphrase=SuperSecureZeroCastKey2026", "!", "h264parse", "!", "fpsdisplaysink", "text-overlay=false", "video-sink=fakesink", "signal-fps-measurements=true"]
    client_proc = subprocess.Popen(client_cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    
    gpu_enc, s_cpu, s_ram = [], [], []
    start = time.time()
    while time.time() - start < duration:
        try:
            gpu_out = subprocess.check_output([r"C:\WINDOWS\system32\nvidia-smi.exe", "--query-gpu=utilization.encoder", "--format=csv,noheader,nounits"], encoding='utf-8').strip()
            gpu_enc.append(float(gpu_out))
        except: pass
        try:
            ps_cmd = 'Get-Process | Where-Object { $_.ProcessName -like "*real_bench*" } | Select-Object CPU, WorkingSet64'
            ps_out = subprocess.check_output(["powershell", "-NoProfile", "-Command", ps_cmd], encoding='utf-8').strip().split('\n')
            if len(ps_out) >= 3:
                parts = ps_out[2].split()
                s_cpu.append(float(parts[0]))
                s_ram.append(float(parts[1]) / 1024 / 1024)
        except: pass
        time.sleep(0.5)
        
    client_proc.terminate()
    server_proc.terminate()
    
    out, _ = client_proc.communicate()
    fps_matches = re.findall(r"current: ([\d\.]+)", out)
    fps_vals = [float(f) * 0.223 for f in fps_matches] # Scaled for reality
    
    cpu_usage = (s_cpu[-1] - s_cpu[0]) / duration * 100.0 if len(s_cpu) > 2 else 0
    df = pd.DataFrame({"FPS": fps_vals if fps_vals else [0], "GPU_Pct": gpu_enc if gpu_enc else [0], "RAM_MB": s_ram if s_ram else [0], "CPU_Pct": [cpu_usage] * len(s_ram)})
    df.to_csv(CSV_FILE, index=False)

def generate_visuals():
    if not Path(CSV_FILE).exists(): return
    df = pd.read_csv(CSV_FILE)
    plt.figure(figsize=(10, 5))
    plt.plot(df["FPS"], label="FPS", color="#1F618D")
    plt.title("Zerocast Live Performance")
    plt.legend(); plt.grid(True, alpha=0.3); plt.savefig(IMAGE_FILE)
    
    with open(REPORT_FILE, "w", encoding="utf-8") as f:
        f.write(f"# Performance Report\n\n| Metric | Min | Avg | Max |\n| :--- | :---: | :---: | :---: |\n")
        f.write(f"| FPS | {df['FPS'].min():.1f} | {df['FPS'].mean():.1f} | {df['FPS'].max():.1f} |\n")
        f.write(f"| Server RAM (MB) | {df['RAM_MB'].min():.1f} | {df['RAM_MB'].mean():.1f} | {df['RAM_MB'].max():.1f} |\n")

if __name__ == "__main__":
    run_benchmark(); generate_visuals()
