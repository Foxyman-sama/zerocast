import os
import subprocess
import sys
import ctypes
import time

# --- Configuration ---
DEFAULT_GSTREAMER_PATH = r"C:\Program Files\gstreamer\1.0\msvc_x86_64\lib\pkgconfig"
SERVER_DIR = "zerocast_server"
CERT_FILE = os.path.join(SERVER_DIR, "identity.p12")
CERT_PASSWORD = "zerocast"

def is_admin():
    try:
        return ctypes.windll.shell32.IsUserAnAdmin()
    except:
        return False

def print_step(msg):
    print(f"\n[STEP] {msg}...")

def check_command(cmd):
    try:
        subprocess.run([cmd, "--version"], capture_output=True, check=True, shell=True)
        return True
    except:
        return False

def install_winget():
    """Attempts to install winget via PowerShell if it's missing."""
    print("[INFO] Winget not found. Attempting to install Microsoft.DesktopAppInstaller...")
    # This script downloads and installs the App Installer (winget)
    ps_script = """
    $progressPreference = 'silentlyContinue'
    Write-Host "Downloading winget..."
    Invoke-WebRequest -Uri "https://github.com/microsoft/winget-cli/releases/latest/download/Microsoft.DesktopAppInstaller_8wekyb3d8bbwe.msixbundle" -OutFile "winget.msixbundle"
    Write-Host "Installing winget..."
    Add-AppxPackage -Path ".\winget.msixbundle"
    Remove-Item ".\winget.msixbundle"
    """
    try:
        subprocess.run(["powershell", "-NoProfile", "-Command", ps_script], check=True)
        print("[SUCCESS] Winget installation command sent. It might take a minute to initialize.")
        time.sleep(5)
        return True
    except subprocess.CalledProcessError:
        print("[ERROR] Failed to auto-install winget.")
        return False

def install_via_winget(package_id, name):
    print(f"[INFO] Installing {name} via winget...")
    try:
        # Use --disable-interactivity to be as silent as possible
        subprocess.run([
            "winget", "install", "--id", package_id, 
            "--silent", "--accept-package-agreements", "--accept-source-agreements"
        ], check=True, shell=True)
        print(f"[SUCCESS] {name} installed.")
        return True
    except subprocess.CalledProcessError:
        print(f"[ERROR] Failed to install {name} via winget.")
        return False

def run_command(cmd, cwd=None, env=None):
    try:
        subprocess.run(cmd, cwd=cwd, env=env, check=True, shell=True)
        return True
    except subprocess.CalledProcessError as e:
        print(f"[ERROR] Command failed: {cmd}")
        return False

def refresh_env():
    import winreg
    try:
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"System\CurrentControlSet\Control\Session Manager\Environment") as key:
            path, _ = winreg.QueryValueEx(key, "Path")
            os.environ["PATH"] = path
    except:
        pass

def setup():
    print("=== Zerocast Fully Automated Quick Setup ===")

    if not is_admin():
        print("[ERROR] This script MUST be run as Administrator to install system components.")
        print("Please restart your terminal (PowerShell/CMD) as Administrator.")
        sys.exit(1)

    # 0. Check/Install Winget
    if not check_command("winget"):
        if not install_winget():
            print("[ERROR] Winget is required and could not be installed automatically.")
            sys.exit(1)

    # 1. Install Prerequisites
    print_step("Checking and installing prerequisites")
    
    # Rust
    if not check_command("cargo"):
        print("[INFO] Rust not found.")
        install_via_winget("Rustlang.Rustup", "Rust/Cargo")
    else:
        print("[INFO] Rust/Cargo is already installed.")

    # OpenSSL
    if not check_command("openssl"):
        print("[INFO] OpenSSL not found.")
        install_via_winget("OpenSSL.OpenSSL", "OpenSSL")
    else:
        print("[INFO] OpenSSL is already installed.")

    # GStreamer
    if not os.path.exists(DEFAULT_GSTREAMER_PATH):
        print("[INFO] GStreamer not found at expected path.")
        install_via_winget("gstreamerproject.gstreamer", "GStreamer")
    else:
        print(f"[INFO] GStreamer found at {DEFAULT_GSTREAMER_PATH}")

    # 2. Setup Environment Variables
    print_step("Setting up environment variables")
    # Force set PKG_CONFIG_PATH for this session
    os.environ["PKG_CONFIG_PATH"] = DEFAULT_GSTREAMER_PATH
    print(f"[INFO] PKG_CONFIG_PATH set to {DEFAULT_GSTREAMER_PATH}")

    # 3. Generate Security Certificate
    print_step("Generating security certificate")
    if os.path.exists(CERT_FILE):
        print(f"[INFO] {CERT_FILE} already exists. Skipping generation.")
    else:
        try:
            refresh_env()
            subprocess.run(["openssl", "genrsa", "-out", "key.pem", "2048"], cwd=SERVER_DIR, check=True, shell=True)
            subprocess.run([
                "openssl", "req", "-new", "-x509", "-key", "key.pem", "-out", "cert.pem", 
                "-days", "365", "-subj", "/CN=zerocast"
            ], cwd=SERVER_DIR, check=True, shell=True)
            subprocess.run([
                "openssl", "pkcs12", "-export", "-out", "identity.p12", "-inkey", "key.pem", 
                "-in", "cert.pem", "-password", f"pass:{CERT_PASSWORD}"
            ], cwd=SERVER_DIR, check=True, shell=True)
            os.remove(os.path.join(SERVER_DIR, "key.pem"))
            os.remove(os.path.join(SERVER_DIR, "cert.pem"))
            print(f"[SUCCESS] Generated {CERT_FILE}")
        except Exception as e:
            print(f"[ERROR] Failed to generate certificate. You may need to restart your terminal.")

    # 4. Install Python dependencies
    print_step("Installing Python dependencies")
    run_command([sys.executable, "-m", "pip", "install", "-r", "requirements.txt"])

    # 5. Build project
    print_step("Building project")
    print("[INFO] Compiling Rust modules (this may take 2-5 minutes)...")
    if run_command("cargo build"):
        print("\n=== Setup Complete! ===")
        print("To start the server: cargo run -p zerocast_server")
        print("To start the client: cargo run -p zerocast_client")
    else:
        print("\n[ERROR] Build failed. Please RESTART your terminal and run 'cargo build' manually.")

if __name__ == "__main__":
    setup()
