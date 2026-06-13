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

def check_and_install_pip():
    """Checks for pip and installs it if missing."""
    print("[INFO] Checking for pip...")
    try:
        # Check if pip is already available as a module
        subprocess.run([sys.executable, "-m", "pip", "--version"], capture_output=True, check=True, shell=True)
        print("[INFO] pip is already installed.")
        return True
    except:
        print("[INFO] pip not found. Attempting to install via ensurepip...")
        try:
            subprocess.run([sys.executable, "-m", "ensurepip", "--default-pip"], check=True, shell=True)
            print("[SUCCESS] pip installed via ensurepip.")
            return True
        except:
            print("[INFO] ensurepip failed. Attempting to download get-pip.py using PowerShell...")
            try:
                # Using PowerShell to download get-pip.py is more robust on Windows
                ps_download = 'Invoke-WebRequest -Uri "https://bootstrap.pypa.io/get-pip.py" -OutFile "get-pip.py"'
                subprocess.run(["powershell", "-NoProfile", "-Command", ps_download], check=True)
                
                print("[INFO] Running get-pip.py...")
                subprocess.run([sys.executable, "get-pip.py", "--user"], check=True, shell=True)
                
                if os.path.exists("get-pip.py"):
                    os.remove("get-pip.py")
                print("[SUCCESS] pip installed via get-pip.py.")
                return True
            except Exception as e:
                print(f"[ERROR] Failed to install pip: {e}")
                print("[HINT] If you are using Windows Store Python, try installing pip manually or use a different Python distribution.")
                return False

def setup_venv():
    """Creates a virtual environment and returns the path to its python executable."""
    venv_dir = ".venv"
    if not os.path.exists(venv_dir):
        print(f"[INFO] Creating virtual environment in {venv_dir}...")
        try:
            subprocess.run([sys.executable, "-m", "venv", venv_dir], check=True, shell=True)
        except Exception as e:
            print(f"[ERROR] Failed to create virtual environment: {e}")
            return sys.executable

    if os.name == 'nt':
        return os.path.abspath(os.path.join(venv_dir, "Scripts", "python.exe"))
    else:
        return os.path.abspath(os.path.join(venv_dir, "bin", "python"))

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
    print_step("Installing Python dependencies (using venv)")
    venv_python = setup_venv()
    
    # Ensure pip is in the venv
    try:
        subprocess.run([venv_python, "-m", "ensurepip", "--default-pip"], capture_output=True, shell=True)
    except:
        pass

    try:
        # We try both normal and break-system-packages (for modern PEP 668 envs)
        cmd = [venv_python, "-m", "pip", "install", "-r", "requirements.txt"]
        if subprocess.run(cmd, shell=True).returncode != 0:
            print("[INFO] Attempting with --break-system-packages...")
            subprocess.run(cmd + ["--break-system-packages"], check=True, shell=True)
        print("[SUCCESS] Python dependencies installed in virtual environment.")
    except Exception as e:
        print(f"[ERROR] Failed to install dependencies: {e}")
        sys.exit(1)

    # 5. Build project
    print_step("Building project")
    print("[INFO] Compiling Rust modules (this may take 2-5 minutes)...")
    if run_command("cargo build"):
        print("\n=== Setup Complete! ===")
        print("To start the server: cargo run -p zerocast_server")
        print("To start the client: cargo run -p zerocast_client")
        print(f"To run analytics: {os.path.relpath(venv_python)} analytics.py")
    else:
        print("\n[ERROR] Build failed. Please RESTART your terminal and run 'cargo build' manually.")

if __name__ == "__main__":
    setup()
