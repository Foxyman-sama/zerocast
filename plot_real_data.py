import pandas as pd
import matplotlib.pyplot as plt

CSV_FILE = "live_latency_records.csv"
IMAGE_FILE = "real_latency_graph.png"

print(f"[INFO] Reading real live logs from: {CSV_FILE}")

# Read the live CSV data recorded by the Rust client app layer
data = pd.read_csv(CSV_FILE)

# Calculate real aggregated statistics directly from the target dataset
avg_latency = data["Latency_MS"].mean()
min_latency = data["Latency_MS"].min()
max_latency = data["Latency_MS"].max()

print(f"[STATS] Min: {min_latency:.2f}ms | Avg: {avg_latency:.2f}ms | Max: {max_latency:.2f}ms")

# Initialize the academic chart frame context
plt.figure(figsize=(10, 5), dpi=300)

# Use data.index dynamically as the X-axis sequence tracker
plt.plot(data.index, data["Latency_MS"], color='#1F618D', linewidth=1, label='Поточна затримка (G2G)')
plt.axhline(y=avg_latency, color='#CB4335', linestyle='--', linewidth=1.5, label=f'Середня затримка ({avg_latency:.2f} мс)')

# Standard DNU/DSTU thesis formatting rules
plt.xlabel('Номер вимірювання (Timeline)', fontsize=11, labelpad=8)
plt.ylabel('Затримка (мілісекунди)', fontsize=11, labelpad=8)

# Set rendering bounds based on your real localhost metrics (0.0ms to 1.0ms bounds)
plt.ylim(-0.2, max_latency + 0.5) 
plt.xlim(0, len(data) - 1)

plt.grid(True, linestyle=':', alpha=0.6, color='#BDC3C7')
plt.legend(loc='upper right', fontsize=10)

# Export high-resolution asset
plt.tight_layout()
plt.savefig(IMAGE_FILE, dpi=300)
print(f"[SUCCESS] Real performance chart exported to: {IMAGE_FILE}")