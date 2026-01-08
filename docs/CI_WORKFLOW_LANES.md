# CI Workflow Lanes

> Phase 6 deliverable: Fast lane vs Full lane workflow documentation.
> Choose the appropriate lane based on change scope and time constraints.

## Overview

| Lane | Duration | When to Use |
|------|----------|-------------|
| **Fast Lane** | ~30 seconds | Quick validation, smoke tests only |
| **Full Lane** | ~5 minutes | Pre-merge, release candidates |
| **Release Lane** | ~10 minutes | Production releases |

---

## Fast Lane (Smoke Only)

**Use when:** Quick iteration, local development, minor changes.

### Windows (PowerShell)
```powershell
cd Prod_Install_Wizard_Deployment/tools
.\smoke-test-unified-installer.ps1 -NoBuild
```

### Linux (Bash)
```bash
cd Prod_Install_Wizard_Deployment/tools
./smoke-test-unified-installer.sh --no-build
```

### What it validates:
- [x] All proof modes exit 0
- [x] All TUI smoke targets render
- [x] No runtime panics

### What it skips:
- [ ] Compilation (assumes pre-built binary)
- [ ] Unit tests
- [ ] Linting/formatting
- [ ] Clippy warnings

---

## Full Lane (Pre-Merge)

**Use when:** Before merging PRs, after significant changes.

### Windows (PowerShell)
```powershell
cd Prod_Install_Wizard_Deployment/installer-unified/src-tauri

# 1. Format check
cargo fmt --check

# 2. Type check
cargo check

# 3. Unit tests
cargo test --lib

# 4. Build release
cargo build --release

# 5. Smoke tests
cd ../../tools
.\smoke-test-unified-installer.ps1 -NoBuild

# 6. Clippy (optional but recommended)
cd ../installer-unified/src-tauri
cargo clippy -- -D warnings
```

### Linux (Bash)
```bash
cd Prod_Install_Wizard_Deployment/installer-unified/src-tauri

# 1. Format check
cargo fmt --check

# 2. Type check
cargo check

# 3. Unit tests
cargo test --lib

# 4. Build release
cargo build --release

# 5. Smoke tests
cd ../../tools
./smoke-test-unified-installer.sh --no-build

# 6. Clippy (optional but recommended)
cd ../installer-unified/src-tauri
cargo clippy -- -D warnings
```

### What it validates:
- [x] Code formatting
- [x] Type safety
- [x] All unit tests pass
- [x] Release build succeeds
- [x] All smoke tests pass
- [x] No clippy warnings (optional)

---

## Release Lane (Production)

**Use when:** Preparing a release, final validation.

### Additional steps beyond Full Lane:

1. **Version bump**
   ```bash
   # Update Cargo.toml version
   # Update package.json version (if applicable)
   ```

2. **Full test suite with coverage**
   ```bash
   cargo tarpaulin --out Html
   ```

3. **Cross-platform builds**
   ```bash
   # Windows
   cargo build --release --target x86_64-pc-windows-msvc
   
   # Linux
   cargo build --release --target x86_64-unknown-linux-gnu
   ```

4. **E2E scenario validation**
   - Run at least one scenario from E2E_SCENARIO_CHECKLISTS.md
   - Collect evidence files

5. **Generate release artifacts**
   ```bash
   # Create installer package
   # Sign binaries (if applicable)
   # Generate checksums
   ```

---

## Exit Codes

All scripts use consistent exit codes:

| Code | Meaning |
|------|---------|
| 0 | All checks passed |
| 1 | One or more checks failed |
| 2 | Script error (missing dependencies, etc.) |

---

## Proof Logs

Each lane produces logs in `Prod_Wizard_Log/`:

| Lane | Log File |
|------|----------|
| Fast | `P6_smoke_windows.log` or `P6_smoke_linux.log` |
| Full | Above + `P6_unit_tests.log` |
| Release | Above + `P6_release_validation.log` |

---

## Quick Reference

```
# Fast lane (30s)
.\smoke-test-unified-installer.ps1 -NoBuild

# Full lane (5min)
cargo fmt --check && cargo check && cargo test --lib && cargo build --release && .\smoke-test-unified-installer.ps1 -NoBuild

# One-liner for CI
cargo fmt --check && cargo check && cargo test --lib && cargo build --release
```

---

## GitHub Actions CI Workflow

The Linux smoke test runs automatically via GitHub Actions:

**Workflow file:** `.github/workflows/linux-smoke.yml`

**Triggers:**
- Push to `main` or `develop` branches (when `installer-unified/**` changes)
- Pull requests to `main`
- Manual dispatch (workflow_dispatch)

**Artifacts produced:**
- `P6_smoke_linux.log` — Linux smoke test results
- `P6_unit_tests_linux.log` — Linux unit test results

**Done condition for Phase 6 RELEASE Lane:**
P6_smoke_linux.log artifact exists with `ExitCode=0`.

---

## WSL Fallback (Manual Linux Smoke)

If CI is unavailable or you need local Linux validation:

### Prerequisites
1. WSL2 installed with Ubuntu 22.04+
2. Rust toolchain installed in WSL
3. Required dependencies:
   ```bash
   sudo apt-get update
   sudo apt-get install -y \
     libwebkit2gtk-4.1-dev \
     libappindicator3-dev \
     librsvg2-dev \
     patchelf \
     libssl-dev \
     libgtk-3-dev
   ```

### Run Linux Smoke in WSL

```bash
# From Windows, open WSL
wsl

# Navigate to repo (adjust path as needed)
cd /mnt/e/CADalytix/Prod_Install_Wizard_Deployment

# Build and run smoke
cd installer-unified/src-tauri
cargo build --release

cd ../tools
chmod +x smoke-test-unified-installer.sh
./smoke-test-unified-installer.sh 2>&1 | tee /mnt/e/CADalytix/Prod_Wizard_Log/P6_smoke_linux.log
echo "ExitCode=$?" >> /mnt/e/CADalytix/Prod_Wizard_Log/P6_smoke_linux.log
```

### Verify Success
```bash
# Must contain ExitCode=0
grep "ExitCode=0" /mnt/e/CADalytix/Prod_Wizard_Log/P6_smoke_linux.log
```

### Done Condition
- `Prod_Wizard_Log/P6_smoke_linux.log` exists
- Contains `ExitCode=0` at the end
- All proof modes show PASS

---

## Phase 7 — Build + Packaging

Phase 7 build scripts orchestrate the build, run smoke tests as a gate, and produce the `CADALYTIX_INSTALLER/` bundle.

### Windows Build

```powershell
# Full build (build + smoke gate + bundle + manifest)
cd Prod_Install_Wizard_Deployment\tools
.\build-unified-installer.ps1

# Fast lane (no rebuild, assumes binary exists)
.\build-unified-installer.ps1 -NoBuild

# Skip smoke gate (for debugging)
.\build-unified-installer.ps1 -NoBuild -NoSmoke
```

### Linux Build

```bash
# Full build
cd Prod_Install_Wizard_Deployment/tools
./build-unified-installer.sh

# Fast lane (no rebuild)
./build-unified-installer.sh --no-build

# Skip smoke gate
./build-unified-installer.sh --no-build --no-smoke
```

### Build Outputs

| Artifact | Location |
|----------|----------|
| Bundle | `RepoRoot/CADALYTIX_INSTALLER/` |
| Build log | `Prod_Wizard_Log/P7_build_windows.log` or `P7_build_linux.log` |
| Versions | `CADALYTIX_INSTALLER/VERIFY/VERSIONS.txt` |
| Manifest | `CADALYTIX_INSTALLER/VERIFY/MANIFEST.sha256` |
| Proof copies | `CADALYTIX_INSTALLER/VERIFY/PROOFS/` |

### Bundle Structure

```
CADALYTIX_INSTALLER/
  INSTALLER/
    windows/installer-unified.exe
    linux/installer-unified
  TOOLS/
    smoke-test-unified-installer.ps1
    smoke-test-unified-installer.sh
  DOCS/
    README.md
    QUICK_START.md
    SYSTEM_REQUIREMENTS.md
    TROUBLESHOOTING.md
  VERIFY/
    VERSIONS.txt
    MANIFEST.sha256
    PROOFS/
```

### Verify Manifest

```bash
cd CADALYTIX_INSTALLER/VERIFY
sha256sum -c MANIFEST.sha256
```

### Phase 7 Done Condition

- `CADALYTIX_INSTALLER/` exists with expected structure
- `P7_build_*.log` ends with `ExitCode=0`
- `MANIFEST.sha256` contains all files and verifies correctly

---

## Release Lane (CI)

Phase 8 Release Bundle workflow automates the full release packaging pipeline on GitHub Actions.

**Workflow file:** `.github/workflows/release-bundle.yml`

**Triggers:**
- Manual dispatch (workflow_dispatch)
- Push tags matching `v*` (e.g., `v1.0.0`, `v2.3.1`)

### Jobs

| Job | Runner | Description |
|-----|--------|-------------|
| `windows-release` | `windows-latest` | Build Windows binary, run smoke gate, create bundle ZIP |
| `linux-release` | `ubuntu-latest` | Build Linux binary, run smoke gate, create bundle TAR.GZ |

### Workflow Steps (per platform)

1. **Checkout** - Clone repository
2. **Setup Rust** - Install stable toolchain
3. **Setup Node.js** - Install Node 20
4. **Install dependencies** - npm ci + (Linux: system libs)
5. **Build frontend** - npm run build
6. **Build Rust release** - cargo build --release --locked
7. **Phase 6 Smoke Gate** - Run smoke-test-unified-installer script
8. **Phase 7 Bundle** - Run build-unified-installer script
9. **Verify Manifest** - Run verify-manifest script
10. **Create Archive** - ZIP (Windows) or TAR.GZ (Linux)
11. **Write Proof Log** - `P8_release_bundle_{os}.log` with `ExitCode=0`
12. **Upload Artifacts** - Bundle archive + proof logs

### Artifacts Produced

| Artifact Name | Contents |
|---------------|----------|
| `CADALYTIX_INSTALLER_windows` | `CADALYTIX_INSTALLER_windows.zip` |
| `CADALYTIX_INSTALLER_linux` | `CADALYTIX_INSTALLER_linux.tar.gz` |
| `windows-release-proof-logs` | `P8_*.log` files from Windows run |
| `linux-release-proof-logs` | `P8_*.log` files from Linux run |

### Running Manually

1. Go to GitHub Actions tab
2. Select "Release Bundle" workflow
3. Click "Run workflow"
4. Select branch (usually `main`)
5. Click "Run workflow"

### Creating a Tagged Release

```bash
# Create and push a tag
git tag v1.0.0
git push origin v1.0.0
# Workflow will trigger automatically
```

### Done Condition

- Workflow completes successfully (green checkmark)
- `CADALYTIX_INSTALLER_{os}` artifacts are downloadable
- Proof logs contain `ExitCode=0`

