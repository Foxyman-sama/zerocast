import pandas as pd
import os

CSV_FILE = "client_metrics.csv"
REPORT_FILE = "FINAL_REPORT.md"

def generate_report():
    if not os.path.exists(CSV_FILE):
        print(f"[ERROR] CSV file {CSV_FILE} not found. Run the project first.")
        return

    print(f"[INFO] Reading telemetry data from {CSV_FILE}...")
    df = pd.read_csv(CSV_FILE)

    if df.empty:
        print("[ERROR] CSV file is empty.")
        return

    # Map CSV columns to Ukrainian labels
    metrics_map = {
        "FPS": "Частота оновлення екрана (FPS)",
        "Latency_MS": "Наскрізна затримка (Click-to-Photon), мс",
        "Srv_CPU_Pct": "Завантаження CPU сервера, %",
        "Srv_GPU_Pct": "Навантаження на ядро GPU NVENC сервера, %",
        "Srv_RAM_MB": "Обсяг оперативної пам'яті сервера (RAM), МБ",
        "Cli_RAM_MB": "Обсяг оперативної пам'яті клієнта (RAM), МБ"
    }

    results = []
    for col, label in metrics_map.items():
        if col in df.columns:
            results.append({
                "Параметр": label,
                "Min": df[col].min(),
                "Avg": df[col].mean(),
                "Max": df[col].max()
            })

    # Create statistics table
    report_df = pd.DataFrame(results)
    
    # Format to 1 decimal place
    markdown_table = "| Експериментальний параметр продуктивності | Мінімальне значення | Середнє значення | Максимальне значення |\n"
    markdown_table += "| :--- | :---: | :---: | :---: |\n"
    
    for _, row in report_df.iterrows():
        markdown_table += f"| {row['Параметр']} | {row['Min']:.1f} | {row['Avg']:.1f} | {row['Max']:.1f} |\n"

    # Save to file
    with open(REPORT_FILE, "w", encoding="utf-8") as f:
        f.write("# Звіт про результати експериментальних досліджень продуктивності\n\n")
        f.write(markdown_table)

    print(f"[SUCCESS] Beautiful report generated at {REPORT_FILE}")
    print("\n" + markdown_table)

if __name__ == "__main__":
    generate_report()
