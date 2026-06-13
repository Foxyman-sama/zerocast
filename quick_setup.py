import os
import sys
import subprocess
import ctypes
import time
from pathlib import Path

# --- Configuration ---
GSTREAMER_DIR = Path(os.environ.get("GSTREAMER_1_0_ROOT_X86_64", r"C:\gstreamer\1.0\msvc_x86_64"))
if not GSTREAMER_DIR.exists():
    GSTREAMER_DIR = Path(r"C:\Program Files\gstreamer\1.0\msvc_x86_64")

SERVER_DIR = Path("zerocast_server")
CERT_FILE = SERVER_DIR / "identity.p12"
VENV_DIR = Path(".venv")

def is_admin():
    try: return ctypes.windll.shell32.IsUserAnAdmin()
    except: return False

def run(cmd, shell=True, env=None, cwd=None, capture=False):
    """Silent-ish execution helper."""
    try:
        res = subprocess.run(cmd, shell=shell, env=env, cwd=cwd, 
                             capture_output=capture, text=True, check=True)
        return res.stdout.strip() if capture else True
    except subprocess.CalledProcessError:
        return False

def get_env():
    """Builds a clean environment using standard Windows formats."""
    env = os.environ.copy()
    sep = os.pathsep # Always ';' on Windows
    
    if GSTREAMER_DIR.exists():
        pc_path = str(GSTREAMER_DIR / "lib" / "pkgconfig")
        bin_path = str(GSTREAMER_DIR / "bin")
        
        env["GSTREAMER_1_0_ROOT_X86_64"] = str(GSTREAMER_DIR)
        
        # Helper to clean and prepend paths
        def prepend_path(key, new_val):
            current = env.get(key, "")
            # Only split by the OS-native separator to avoid breaking drive colons
            parts = [p.strip() for p in current.split(os.pathsep) if p.strip()]
            if new_val not in parts:
                parts.insert(0, new_val)
            return sep.join(parts)

        import re
        env["PKG_CONFIG_PATH"] = prepend_path("PKG_CONFIG_PATH", pc_path)
        env["PATH"] = prepend_path("PATH", bin_path)

        env["PKG_CONFIG_ALLOW_SYSTEM_LIBS"] = "1"
        env["PKG_CONFIG_ALLOW_SYSTEM_CFLAGS"] = "1"
        
    return env

def setup():
    print("🚀 Zerocast Ultimate Setup")
    if not is_admin():
        print("❌ Error: Please run as Administrator.")
        sys.exit(1)

    # 1. System Dependencies (Winget)
    print("\n📦 [1/4] Checking System Dependencies...")
    env = get_env()
    needed = []
    if not run("cargo --version", capture=True): needed.append("Rustlang.Rustup")
    if not run("openssl version", capture=True): needed.append("OpenSSL.OpenSSL")
    if not GSTREAMER_DIR.exists(): needed.append("gstreamerproject.gstreamer")

    for pkg in needed:
        print(f"  -> Installing {pkg}...")
        args = '--override "/quiet ADDLOCAL=ALL"' if "gstreamer" in pkg.lower() else ""
        run(f"winget install --id {pkg} --silent --accept-package-agreements --accept-source-agreements {args}")
    
    # 2. Security
    print("\n🔐 [2/4] Configuring Security...")
    if not CERT_FILE.exists():
        SERVER_DIR.mkdir(exist_ok=True)
        ps_cert = f'openssl genrsa -out key.pem 2048; openssl req -new -x509 -key key.pem -out cert.pem -days 365 -subj "/CN=zerocast"; openssl pkcs12 -export -out identity.p12 -inkey key.pem -in cert.pem -password pass:zerocast; rm key.pem cert.pem'
        run(ps_cert, cwd=SERVER_DIR, env=env)
        print("  ✅ Certificate generated.")
    else:
        print("  ✅ Certificate already exists.")

    # 3. Python & Venv
    print("\n🐍 [3/4] Preparing Python Environment...")
    if not VENV_DIR.exists():
        run(f'"{sys.executable}" -m venv {VENV_DIR}')
    
    v_python = VENV_DIR / ("Scripts" if os.name == "nt" else "bin") / "python"
    v_exe = f'"{v_python}"'
    
    # Ensure pip is ready and install requirements
    run(f"{v_exe} -m ensurepip", capture=True)
    run(f"{v_exe} -m pip install --upgrade pip", capture=True)
    run(f"{v_exe} -m pip install -r requirements.txt --break-system-packages")
    print("  ✅ Dependencies installed.")

    # 4. Final Build
    print("\n🦀 [4/4] Building Rust Project...")
    env = get_env() # Re-fetch in case GStreamer was just installed
    if run("cargo build", env=env):
        print("\n✨ SETUP COMPLETE!")
        print("-" * 30)
        is_bash = "SHELL" in os.environ
        if is_bash:
            print("Run in Bash/MINGW64:")
            print(f"  export PKG_CONFIG_PATH=\"{env['PKG_CONFIG_PATH']}\"")
            print(f"  export PATH=\"{env['PATH']}\"")
        else:
            print("Run in PowerShell:")
            print(f"  $env:PKG_CONFIG_PATH=\"{env['PKG_CONFIG_PATH']}\"")
            print(f"  $env:PATH=\"{env['PATH']}\"")
        print("\nCommands:")
        print("  cargo run -p zerocast_server")
        print("  cargo run -p zerocast_client")
    else:
        print("\n❌ Build failed. Please restart your terminal and try again.")

if __name__ == "__main__":
    setup()
