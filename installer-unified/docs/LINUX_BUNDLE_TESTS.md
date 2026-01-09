# Linux Bundle Manual Test Cases

This document describes manual tests for the Smart Linux Bundle installer.

---

## Test 1: Ubuntu/Debian System — DEB Selection

**Setup:** Ubuntu 22.04 or Debian 12

**Steps:**
1. Extract the bundle: `tar -xzf CADalytix_Linux_Bundle_*.tar.gz`
2. Run dry-run: `./LINUX_BUNDLE/INSTALL --dry-run`

**Expected:**
```
Detecting system...
Preparing installer for ubuntu (deb)...
[DRY-RUN] Would install: /path/to/artifacts/CADalytix_Installer_*.deb
[DRY-RUN] Command: sudo apt install -y "/path/to/artifacts/CADalytix_Installer_*.deb"
[DRY-RUN] Would launch GUI
Done.
```

**Steps (full install):**
3. Run: `./LINUX_BUNDLE/INSTALL`

**Expected:**
- Installs .deb package via apt
- Launches CADalytix Installer GUI
- Log created in `LINUX_BUNDLE/logs/`

---

## Test 2: Fedora/RHEL System — RPM Selection

**Setup:** Fedora 39 or Rocky Linux 9

**Steps:**
1. Extract and run: `./LINUX_BUNDLE/INSTALL --dry-run`

**Expected:**
```
Detecting system...
Preparing installer for fedora (rpm)...
[DRY-RUN] Would install: /path/to/artifacts/CADalytix_Installer_*.rpm
[DRY-RUN] Command: sudo dnf install -y "/path/to/artifacts/CADalytix_Installer_*.rpm"
```

---

## Test 3: Unknown Distro — AppImage Fallback

**Setup:** Arch Linux, Gentoo, or any non-deb/rpm distro

**Steps:**
1. Run: `./LINUX_BUNDLE/INSTALL --dry-run`

**Expected:**
```
Detecting system...
Preparing installer for arch (appimage)...
[DRY-RUN] Would run: /path/to/artifacts/CADalytix_Installer_*.AppImage
```

---

## Test 4: AppImage Missing FUSE

**Setup:** System without libfuse2 installed

**Steps:**
1. Run: `./LINUX_BUNDLE/INSTALL --force-appimage`

**Expected (if FUSE missing):**
```
Detecting system...
Preparing installer for ubuntu (appimage)...
Installing...

This portable build needs FUSE support. Install it with:

  sudo apt install -y libfuse2

Alternative: Extract and run without FUSE:
  /path/to/artifacts/CADalytix_Installer_*.AppImage --appimage-extract
  ./squashfs-root/AppRun
```

---

## Test 5: Headless/No GUI

**Setup:** Server without DISPLAY set (SSH session without X forwarding)

**Steps:**
1. Unset DISPLAY: `unset DISPLAY`
2. Run: `./LINUX_BUNDLE/INSTALL`

**Expected (no TUI bundle):**
```
Detecting system...
No GUI detected and no TUI bundle present.
Use the headless bundle or run with GUI.
```

**Expected (with TUI bundle in tui/):**
- Routes to `LINUX_BUNDLE/tui/INSTALL`

---

## Test 6: --tui Flag

**Steps:**
1. Run: `./LINUX_BUNDLE/INSTALL --tui`

**Expected:**
- Routes to TUI installer if present
- Shows error message if not present

---

## Test 7: Checksum Verification — Valid

**Setup:** Bundle with valid checksums

**Steps:**
1. Run: `./LINUX_BUNDLE/INSTALL --verbose`

**Expected:**
- No checksum errors
- Verbose output shows "Checksum verified OK"

---

## Test 8: Checksum Verification — Invalid (Corrupt File)

**Setup:** Corrupt an artifact manually

**Steps:**
1. Modify artifact: `echo "corrupt" >> LINUX_BUNDLE/artifacts/*.deb`
2. Run: `./LINUX_BUNDLE/INSTALL`

**Expected:**
```
ERROR: Checksum mismatch for CADalytix_Installer_*.deb
```

---

## Test 9: Force Flags

**Test 9a: --force-deb on non-Debian system**
```bash
./LINUX_BUNDLE/INSTALL --force-deb --dry-run
# Should select .deb regardless of distro
```

**Test 9b: --force-rpm on non-RHEL system**
```bash
./LINUX_BUNDLE/INSTALL --force-rpm --dry-run
# Should select .rpm regardless of distro
```

**Test 9c: --force-appimage on Debian system**
```bash
./LINUX_BUNDLE/INSTALL --force-appimage --dry-run
# Should select .AppImage regardless of distro
```

---

## Test 10: --no-launch Flag

**Steps:**
1. Run: `./LINUX_BUNDLE/INSTALL --no-launch`

**Expected:**
- Installs package
- Does NOT launch GUI
- Outputs: "Done." (no "Launching...")

---

## Test 11: WSL Detection

**Setup:** Windows Subsystem for Linux

**Steps:**
1. Run: `./LINUX_BUNDLE/INSTALL --dry-run`

**Expected:**
- Detects WSL
- Prefers .deb over .AppImage (AppImage often fails in WSL)

---

## Test 12: Idempotent Re-run

**Steps:**
1. Run: `./LINUX_BUNDLE/INSTALL` (first time)
2. Run: `./LINUX_BUNDLE/INSTALL` again

**Expected:**
- Second run should not break anything
- Package manager handles reinstall gracefully
- GUI launches again

---

## Log Verification

After each test, verify:
1. Log file created: `LINUX_BUNDLE/logs/install-*.log`
2. Log contains all detection info
3. Log contains commands run
4. Log contains any errors encountered

