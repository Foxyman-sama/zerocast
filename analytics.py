import os
import sys
import pandas as pd
import matplotlib.pyplot as plt
from pathlib import Path

# Ensure console output handles UTF-8 safely on Windows
if sys.stdout and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
    except Exception:
        pass

CSV_FILE = "client_metrics.csv"
REPORT_FILE = "FINAL_REPORT.md"
IMAGE_FILE = "performance_graph.png"

def generate_report():
    print(f"Analyzing data from {CSV_FILE}...")
    csv_path = Path(CSV_FILE)
    if not csv_path.exists():
        print(f"Error: {CSV_FILE} not found. Please run zerocast_server and zerocast_client first to generate metrics data.")
        sys.exit(1)
        
    try:
        df = pd.read_csv(csv_path)
    except Exception as e:
        print(f"Error reading {CSV_FILE}: {e}")
        sys.exit(1)
        
    if df.empty:
        print(f"Error: {CSV_FILE} is empty.")
        sys.exit(1)
        
    print(f"Loaded {len(df)} telemetry snapshots.")
    
    # Map the columns in CSV to display names
    # CSV fields: Latency_MS,FPS,Srv_CPU_Pct,Srv_GPU_Pct,Srv_RAM_MB,Cli_RAM_MB
    metrics_map = {
        "FPS": {
            "en": "Frame Rate (FPS)",
            "ua": "Частота оновлення екрана (FPS)"
        },
        "Latency_MS": {
            "en": "End-to-End Latency (Click-to-Photon), ms",
            "ua": "Наскрізна затримка (Click-to-Photon), мс"
        },
        "Srv_CPU_Pct": {
            "en": "Server CPU Usage, %",
            "ua": "Завантаження CPU сервера, %"
        },
        "Srv_GPU_Pct": {
            "en": "Server GPU NVENC Core Usage, %",
            "ua": "Навантаження на ядро GPU NVENC сервера, %"
        },
        "Srv_RAM_MB": {
            "en": "Server RAM Usage, MB",
            "ua": "Обсяг оперативної пам'яті сервера (RAM), МБ"
        },
        "Cli_RAM_MB": {
            "en": "Client RAM Usage, MB",
            "ua": "Обсяг оперативної пам'яті клієнта (RAM), МБ"
        }
    }
    
    # Calculate stats
    stats = {}
    for col in metrics_map.keys():
        if col in df.columns:
            series = df[col].dropna()
            if not series.empty:
                stats[col] = {
                    "min": float(series.min()),
                    "avg": float(series.mean()),
                    "max": float(series.max())
                }
            else:
                stats[col] = {"min": 0.0, "avg": 0.0, "max": 0.0}
        else:
            stats[col] = {"min": 0.0, "avg": 0.0, "max": 0.0}
            
    # Generate the Markdown tables
    report_content = []
    report_content.append("# Zerocast Performance Analysis Report")
    report_content.append(f"\n*Generated from {len(df)} telemetry samples in `{CSV_FILE}`.*\n")
    
    # Ukrainian Table
    report_content.append("## Експериментальні показники продуктивності (Ukrainian)")
    report_content.append("| Експериментальний параметр продуктивності | Мінімальне значення | Середнє значення | Максимальне значення |")
    report_content.append("| :--- | :---: | :---: | :---: |")
    for col, names in metrics_map.items():
        s = stats[col]
        report_content.append(f"| {names['ua']} | {s['min']:.1f} | {s['avg']:.1f} | {s['max']:.1f} |")
        
    # English Table
    report_content.append("\n## Experimental Performance Metrics (English)")
    report_content.append("| Experimental Performance Metric | Minimum Value | Average Value | Maximum Value |")
    report_content.append("| :--- | :---: | :---: | :---: |")
    for col, names in metrics_map.items():
        s = stats[col]
        report_content.append(f"| {names['en']} | {s['min']:.1f} | {s['avg']:.1f} | {s['max']:.1f} |")
        
    # Write report
    with open(REPORT_FILE, "w", encoding="utf-8") as f:
        f.write("\n".join(report_content) + "\n")
    print(f"Report saved to {REPORT_FILE}")
    
    # Print the Ukrainian table to console
    print("\n=== EXPERIMENTAL BENCHMARK REPORT ===")
    print("| Експериментальний параметр продуктивності | Мінімальне значення | Середнє значення | Максимальне значення |")
    print("| :--- | :---: | :---: | :---: |")
    for col, names in metrics_map.items():
        s = stats[col]
        print(f"| {names['ua']} | {s['min']:.1f} | {s['avg']:.1f} | {s['max']:.1f} |")
    print("=====================================\n")
    
    # Generate visuals
    generate_visuals(df)

def generate_visuals(df):
    print(f"Generating performance graphs -> {IMAGE_FILE}...")
    
    # Apply a clean modern style
    plt.style.use('seaborn-v0_8-whitegrid' if 'seaborn-v0_8-whitegrid' in plt.style.available else 'default')
    
    fig, axes = plt.subplots(3, 1, figsize=(11, 12), sharex=False)
    fig.suptitle("Zerocast Live Performance Metrics", fontsize=16, fontweight='bold', color='#2C3E50')
    
    # 1. Network & Display (FPS & Latency)
    ax1 = axes[0]
    ax1_twin = ax1.twinx()
    
    fps_data = df["FPS"].ffill().bfill() if "FPS" in df.columns else pd.Series(dtype=float)
    lat_data = df["Latency_MS"].ffill().bfill() if "Latency_MS" in df.columns else pd.Series(dtype=float)
    
    p1, = ax1.plot(df.index, fps_data, label="FPS", color="#1F618D", linewidth=2)
    p2, = ax1_twin.plot(df.index, lat_data, label="Latency (ms)", color="#E74C3C", linewidth=1.5, linestyle="--")
    
    ax1.set_title("Network & Display (FPS & Latency)", fontsize=12, fontweight='bold', color='#34495E', loc='left')
    ax1.set_ylabel("Frame Rate (FPS)", color="#1F618D", fontweight='bold')
    ax1_twin.set_ylabel("Latency (ms)", color="#E74C3C", fontweight='bold')
    ax1.tick_params(axis='y', labelcolor="#1F618D")
    ax1_twin.tick_params(axis='y', labelcolor="#E74C3C")
    ax1.legend(handles=[p1, p2], loc="upper right")
    
    # 2. Compute Utilization (Server CPU & GPU %)
    ax2 = axes[1]
    cpu_data = df["Srv_CPU_Pct"].ffill().bfill() if "Srv_CPU_Pct" in df.columns else pd.Series(dtype=float)
    gpu_data = df["Srv_GPU_Pct"].ffill().bfill() if "Srv_GPU_Pct" in df.columns else pd.Series(dtype=float)
    
    ax2.plot(df.index, cpu_data, label="Server CPU %", color="#27AE60", linewidth=1.8)
    ax2.plot(df.index, gpu_data, label="Server GPU %", color="#F39C12", linewidth=1.8)
    ax2.set_title("Compute Utilization (CPU & GPU)", fontsize=12, fontweight='bold', color='#34495E', loc='left')
    ax2.set_ylabel("Utilization %", fontweight='bold')
    ax2.set_ylim(0, 105)
    ax2.legend(loc="upper right")
    
    # 3. Memory Allocation (Server & Client RAM in MB)
    ax3 = axes[2]
    srv_ram = df["Srv_RAM_MB"].ffill().bfill() if "Srv_RAM_MB" in df.columns else pd.Series(dtype=float)
    cli_ram = df["Cli_RAM_MB"].ffill().bfill() if "Cli_RAM_MB" in df.columns else pd.Series(dtype=float)
    
    ax3.plot(df.index, srv_ram, label="Server RAM (MB)", color="#8E44AD", linewidth=1.8)
    ax3.plot(df.index, cli_ram, label="Client RAM (MB)", color="#16A085", linewidth=1.8)
    ax3.set_title("Memory Allocation (RAM)", fontsize=12, fontweight='bold', color='#34495E', loc='left')
    ax3.set_ylabel("Memory (MB)", fontweight='bold')
    ax3.set_xlabel("Sample Index", fontweight='bold')
    ax3.legend(loc="upper right")
    
    plt.tight_layout(rect=[0, 0, 1, 0.96])
    plt.savefig(IMAGE_FILE, dpi=120)
    plt.close()
    print("Performance graphs generated successfully.")

if __name__ == "__main__":
    generate_report()
