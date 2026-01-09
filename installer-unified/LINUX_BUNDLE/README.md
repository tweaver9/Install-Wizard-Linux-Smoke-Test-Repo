# CADalytix Installer — Linux Bundle

## Quick Start

1. **Extract** the archive:
   ```bash
   tar -xzf CADalytix_Linux_Bundle_*.tar.gz
   cd LINUX_BUNDLE
   ```

2. **Run the installer**:
   ```bash
   ./INSTALL
   ```

That's it! The installer automatically detects your Linux distribution and installs the correct package.

---

## What Happens

- **Ubuntu, Debian, Linux Mint, Pop!_OS** → Installs `.deb` package
- **Fedora, RHEL, Rocky, AlmaLinux, CentOS** → Installs `.rpm` package
- **Other distributions** → Uses portable `.AppImage`

After installation, the CADalytix Installer GUI launches automatically.

---

## Optional Flags

| Flag | Description |
|------|-------------|
| `--dry-run` | Show what would happen without making changes |
| `--verbose` | Show detailed output during installation |
| `--force-deb` | Force install using .deb package |
| `--force-rpm` | Force install using .rpm package |
| `--force-appimage` | Force using the portable AppImage |
| `--no-launch` | Install only, don't launch the GUI |
| `--tui` | Use text-based installer (if available) |

### Examples

```bash
# Preview what will happen
./INSTALL --dry-run

# Verbose installation
./INSTALL --verbose

# Install but don't launch
./INSTALL --no-launch

# Force AppImage on any distro
./INSTALL --force-appimage
```

---

## Troubleshooting

### AppImage won't run (FUSE error)

If you see a FUSE-related error, install the required library:

**Ubuntu/Debian:**
```bash
sudo apt install -y libfuse2
```

**Fedora/RHEL:**
```bash
sudo dnf install -y fuse fuse-libs
```

**openSUSE:**
```bash
sudo zypper install -y fuse libfuse2
```

### No GUI available (headless server)

If running on a headless server without a display:
```bash
./INSTALL --tui
```

This uses the text-based installer if available in the bundle.

---

## Bundle Contents

```
LINUX_BUNDLE/
├── INSTALL              # This smart installer script
├── README.md            # This file
├── VERSION.txt          # Bundle version
├── artifacts/           # .deb, .rpm, .AppImage files
├── checksums/           # SHA256 checksums for verification
├── logs/                # Installation logs
└── tui/                 # Optional text-based installer
```

---

## Logs

Installation logs are saved to `LINUX_BUNDLE/logs/` with timestamps.
Check these if you encounter issues.

---

## Requirements

- Linux x86_64 (64-bit)
- For .deb/.rpm: `sudo` access for system installation
- For AppImage: FUSE support (or use `--appimage-extract` fallback)

---

## Support

For issues, contact CADalytix support or check the installation logs.

