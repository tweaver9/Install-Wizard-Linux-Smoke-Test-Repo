# Linux Build & Installation Guide

This document covers building and installing CADalytix Installer on Linux.

## Build Dependencies

### Ubuntu 22.04+ / Debian 12+

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config curl git file \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libsoup-3.0-dev \
  patchelf
```

### Fedora 38+ / RHEL 9+

```bash
sudo dnf groupinstall -y "Development Tools"
sudo dnf install -y \
  pkg-config curl git file \
  webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
  librsvg2-devel libsoup3-devel \
  patchelf
```

### Arch Linux

```bash
sudo pacman -S --needed \
  base-devel pkg-config curl git file \
  webkit2gtk-4.1 gtk3 libappindicator-gtk3 \
  librsvg libsoup3 \
  patchelf
```

## Install Rust

```bash
curl https://sh.rustup.rs -sSf | sh
source "$HOME/.cargo/env"
rustup update
```

## Install Node.js

Recommend Node 18 LTS or later:

```bash
# Using nvm (recommended)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
source ~/.bashrc
nvm install 18
nvm use 18

# Or using package manager (Ubuntu)
curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
sudo apt install -y nodejs
```

## Build Steps

```bash
cd Prod_Install_Wizard_Deployment/installer-unified

# Build frontend
cd frontend
npm install
npm run build
cd ..

# Build Tauri app
cd src-tauri
cargo tauri build
```

## Expected Outputs

After a successful build, artifacts are located in:

```
src-tauri/target/release/bundle/
├── deb/
│   └── cadalytix-installer_*.deb
├── rpm/
│   └── cadalytix-installer-*.rpm
└── appimage/
    └── cadalytix-installer_*.AppImage
```

## Installation

### Debian/Ubuntu (.deb)

```bash
sudo dpkg -i src-tauri/target/release/bundle/deb/cadalytix-installer_*.deb
```

### Fedora/RHEL (.rpm)

```bash
sudo rpm -i src-tauri/target/release/bundle/rpm/cadalytix-installer-*.rpm
```

### AppImage (universal)

```bash
chmod +x src-tauri/target/release/bundle/appimage/cadalytix-installer_*.AppImage
./cadalytix-installer_*.AppImage
```

## Runtime Dependencies (for end-users)

End-users installing the built `.deb` or `.rpm` packages need runtime libraries.
The Tauri bundle typically declares these as dependencies, but if manual install
is needed:

### Ubuntu/Debian Runtime

```bash
sudo apt install -y libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1
```

### Fedora/RHEL Runtime

```bash
sudo dnf install -y webkit2gtk4.1 gtk3 libappindicator-gtk3
```

> **Note**: The exact package names may vary based on your Tauri bundle configuration.
> Check `src-tauri/tauri.conf.json` for the declared dependencies.

## Notes on WSL2

**AppImage builds do not work reliably in WSL2** due to FUSE limitations.

For reliable Linux artifact generation:
- Use a native Linux machine or VM
- Use GitHub Actions with `ubuntu-latest`
- Use Docker with `--privileged` for FUSE support (complex)

The `.deb` and `.rpm` builds generally work in WSL2.

## Headless / TUI Mode

For servers without a display, use TUI mode:

```bash
./INSTALL --tui
```

See `docs/SMOKE_TESTS.md` for TUI smoke test commands.

