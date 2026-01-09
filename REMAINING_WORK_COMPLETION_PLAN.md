# CADalytix Unified Installer — Remaining Work Completion Plan

> **Created:** 2026-01-08  
> **Status:** Active  
> **Scope:** Linux/Docker Installation + UI Completion  

---

## Executive Summary

This document defines the phased plan to complete ALL remaining work for the CADalytix Unified Installer. Work is organized into 6 phases across an estimated 4-6 weeks.

| Phase | Name | Duration | Focus |
|-------|------|----------|-------|
| **Phase 0** | Critical Fixes | 1 day | Unblock existing functionality |
| **Phase 1** | Linux Preflight & Core | 3-4 days | Linux system checks, disk space, distro detection |
| **Phase 2** | Linux Native Installation | 3-4 days | systemd service management, file permissions |
| **Phase 3** | Docker Integration | 4-5 days | Compose templates, image loading, end-to-end flow |
| **Phase 4** | UI Refactoring & Polish | 5-7 days | Component extraction, step indicator, accessibility |
| **Phase 5** | Cross-Platform Build & Test | 3-4 days | Linux binary, TUI testing, integration validation |
| **Phase 6** | Final Validation | 2-3 days | End-to-end testing, documentation, release prep |

**Total Estimated Effort:** 21-28 days

---

## Current State Summary

### ✅ COMPLETE
- Windows GUI wizard (all 15 pages)
- Windows service installation (`sc.exe`)
- Windows preflight checks (.NET, disk space, WebView2)
- TUI wizard (all 15 pages, 4,300 lines)
- Database connection, migrations, schema mapping
- License verification API
- Progress events and installation orchestration
- **Phase 9: Create NEW Database Provisioning** (SQL Server + PostgreSQL)
- **Phase 0: Critical Fixes** (CSS classes, runtime structure)
- **Phase 1: Linux Preflight & Core** (distro detection, disk/memory checks, Docker status)
- **Phase 2: Linux Native Installation** (systemd service, file permissions, install_linux_native())
- **Phase 3: Docker Integration** (compose templates, image loading, install_docker_mode())

### ⚠️ PARTIALLY COMPLETE
- Linux GUI (code exists, no binary built)
- UI architecture (functional but monolithic)

### 🔴 NOT STARTED
- UI component extraction (Phase 4)
- Step progress indicator (Phase 4)
- Splash screen (Phase 4)
- Cross-platform build & test (Phase 5)
- Final validation (Phase 6)

---

## Phase 0: Critical Fixes (Day 1)

**Objective:** Fix blocking issues that break existing functionality.

### P0-1: Add Missing CSS Class
**File:** `frontend/src/App.css`  
**Issue:** `.wizard-success` class is referenced at line 1688 of App.tsx but does not exist.

```css
/* Add to App.css */
.wizard-success {
  color: #107c10;
  font-weight: 600;
  margin-top: 8px;
}
```

### P0-2: Fix Primary Button Styling
**File:** `frontend/src/App.css`  
**Issue:** Primary button only changes border, not background.

```css
.wizard-button.primary {
  background: #0078d4;
  color: #ffffff;
  border-color: #0078d4;
}
.wizard-button.primary:hover:not(:disabled) {
  background: #106ebe;
  border-color: #106ebe;
}
```

### P0-3: Validate Runtime Folder Structure
**Locations:**
- `runtime/linux/` — Currently empty
- `runtime/windows/` — Currently empty  
- `runtime/shared/` — Currently empty

**Action:** Create `.gitkeep` files and document expected structure.

### Deliverables — Phase 0 ✅ COMPLETE
- [x] `.wizard-success` CSS class added
- [x] Primary button styling fixed
- [x] Runtime folder structure documented

---

## Phase 1: Linux Preflight & Core (Days 2-5)

**Objective:** Implement Linux system detection and preflight checks.

### P1-1: Linux Distro Detection
**File:** `src-tauri/src/installation/linux.rs`

```rust
pub async fn detect_linux_distro() -> Result<LinuxDistro> {
    // Read /etc/os-release
    // Parse ID, VERSION_ID, PRETTY_NAME
    // Return struct with distro info
}

pub struct LinuxDistro {
    pub id: String,           // "ubuntu", "rhel", "debian"
    pub version_id: String,   // "22.04", "9"
    pub pretty_name: String,  // "Ubuntu 22.04.3 LTS"
    pub id_like: Vec<String>, // ["debian"], ["fedora"]
}
```

### P1-2: Linux Disk Space Check
**File:** `src-tauri/src/installation/linux.rs`

```rust
pub async fn get_free_space_bytes_linux(path: &Path) -> Result<u64> {
    // Use nix::sys::statvfs or libc::statvfs
    // Return available bytes
}
```

### P1-3: Linux Memory Check
**File:** `src-tauri/src/installation/linux.rs`

```rust
pub async fn get_available_memory_mb() -> Result<u64> {
    // Read /proc/meminfo
    // Parse MemAvailable or MemFree + Buffers + Cached
}
```

### P1-4: Linux Preflight Integration
**File:** `src-tauri/src/api/preflight.rs`

Add `#[cfg(target_os = "linux")]` block with checks:
- Distro detection
- Disk space (minimum 1 GB)
- Memory (minimum 512 MB)
- Docker installed (if docker mode)
- Docker daemon running
- User permissions (groups, sudo)

### P1-5: Docker Status Checks
**File:** `src-tauri/src/installation/docker.rs`

```rust
pub async fn is_docker_daemon_running() -> Result<bool> {
    // Run: docker info
    // Return true if exit code 0
}

pub async fn get_docker_version() -> Result<DockerVersion> {
    // Parse docker --version output
    // Return major, minor, patch
}
```

### Deliverables — Phase 1 ✅ COMPLETE
- [x] `LinuxDistro` struct and `detect_linux_distro()` — implemented in `linux_parsers.rs`
- [x] `get_free_space_bytes_linux()` using statvfs — implemented in `linux.rs`
- [x] `get_available_memory_mb()` from /proc/meminfo — implemented in `linux_parsers.rs`
- [x] Linux preflight checks in `preflight_host()` command — integrated
- [x] Docker daemon status check — `is_docker_daemon_running()` in `docker.rs`
- [x] Unit tests for Linux detection functions — 10 tests in `linux_parsers.rs`

---

## Phase 2: Linux Native Installation (Days 6-9)

**Objective:** Implement systemd service installation and management for native Linux deployment.

### P2-1: Service Installation Function
**File:** `src-tauri/src/installation/service.rs`

```rust
#[cfg(target_os = "linux")]
pub async fn install_and_start_linux_service(
    service_name: &str,
    exec_path: &Path,
    working_dir: &Path,
    user: Option<&str>,
) -> Result<()> {
    // 1. Generate service unit file
    // 2. Copy to /etc/systemd/system/{service_name}.service
    // 3. Run: systemctl daemon-reload
    // 4. Run: systemctl enable {service_name}
    // 5. Run: systemctl start {service_name}
    // 6. Verify with is_linux_service_running()
}
```

### P2-2: Service Status Check
**File:** `src-tauri/src/installation/service.rs`

```rust
#[cfg(target_os = "linux")]
pub async fn is_linux_service_running(service_name: &str) -> Result<bool> {
    // Run: systemctl is-active {service_name}
    // Return true if output is "active"
}

#[cfg(target_os = "linux")]
pub async fn get_linux_service_status(service_name: &str) -> Result<ServiceStatus> {
    // Run: systemctl status {service_name}
    // Parse and return structured status
}
```

### P2-3: File Permission Handling
**File:** `src-tauri/src/installation/linux.rs`

```rust
pub async fn set_executable_permissions(path: &Path) -> Result<()> {
    // chmod +x
    use std::os::unix::fs::PermissionsExt;
    let mut perms = tokio::fs::metadata(path).await?.permissions();
    perms.set_mode(0o755);
    tokio::fs::set_permissions(path, perms).await?;
    Ok(())
}

pub async fn set_service_user_ownership(
    path: &Path,
    user: &str,
    group: &str,
) -> Result<()> {
    // Run: chown {user}:{group} {path}
}
```

### P2-4: Linux Installation Flow
**File:** `src-tauri/src/installation/linux.rs`

Replace placeholder with full implementation:

```rust
pub async fn install_linux_native(
    req: &StartInstallRequest,
    emit_progress: &ProgressEmitter,
    correlation_id: &str,
) -> Result<InstallArtifacts> {
    // 1. Validate prerequisites (preflight)
    // 2. Create destination directory
    // 3. Copy runtime files from runtime/linux/
    // 4. Set executable permissions
    // 5. Generate configuration files
    // 6. Install systemd service
    // 7. Start service
    // 8. Verify service running
    // 9. Return artifacts
}
```

### P2-5: Root/Sudo Detection
**File:** `src-tauri/src/installation/linux.rs`

```rust
pub fn is_running_as_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

pub async fn check_sudo_available() -> Result<bool> {
    // Run: sudo -n true
    // Return true if exit code 0 (passwordless sudo)
}
```

### Deliverables — Phase 2 ✅ COMPLETE
- [x] `install_and_start_linux_service()` function — implemented in `service.rs`
- [x] `is_linux_service_running()` function — implemented in `service.rs`
- [x] `set_executable_permissions()` helper — implemented in `linux.rs`
- [x] `set_service_user_ownership()` helper — implemented in `linux.rs`
- [x] Full `install_linux_native()` flow — implemented in `linux.rs`
- [x] Root/sudo detection — `is_running_as_root()`, `check_sudo_available()`, `require_root_or_passwordless_sudo()`
- [x] Integration with `start_install` command — wired in `installer.rs`
- [x] Unit tests for systemd unit generation — 5 tests in `service.rs`

---

## Phase 3: Docker Integration (Days 10-14)

**Objective:** Complete Docker installation mode with real compose templates and image handling.

### P3-1: Docker Compose Template
**File:** `runtime/linux/docker/compose/docker-compose.template.yml`

```yaml
# CADalytix Docker Compose Template
# Variables: {{DB_CONNECTION_STRING}}, {{DATA_PATH}}, {{PORT}}, etc.

version: "3.8"

services:
  cadalytix-web:
    image: cadalytix-web:latest
    container_name: cadalytix-web
    restart: unless-stopped
    ports:
      - "{{WEB_PORT}}:8080"
    environment:
      - ConnectionStrings__ConfigDb={{DB_CONNECTION_STRING}}
      - ASPNETCORE_ENVIRONMENT=Production
    volumes:
      - {{DATA_PATH}}/logs:/app/logs
      - {{DATA_PATH}}/data:/app/data
    depends_on:
      - cadalytix-worker
    networks:
      - cadalytix-net

  cadalytix-worker:
    image: cadalytix-worker:latest
    container_name: cadalytix-worker
    restart: unless-stopped
    environment:
      - ConnectionStrings__ConfigDb={{DB_CONNECTION_STRING}}
    volumes:
      - {{DATA_PATH}}/logs:/app/logs
      - {{DATA_PATH}}/data:/app/data
    networks:
      - cadalytix-net

networks:
  cadalytix-net:
    driver: bridge

volumes:
  cadalytix-data:
```

### P3-2: Template Substitution Engine
**File:** `src-tauri/src/installation/docker.rs`

```rust
pub async fn generate_compose_file(
    template_path: &Path,
    output_path: &Path,
    variables: &HashMap<String, String>,
) -> Result<()> {
    // 1. Read template
    // 2. Replace {{VAR_NAME}} placeholders
    // 3. Write to output path
    // 4. Validate YAML syntax
}
```

### P3-3: Docker Image Loading
**File:** `src-tauri/src/installation/docker.rs`

```rust
pub async fn load_docker_images(
    images_dir: &Path,
    emit_progress: &ProgressEmitter,
) -> Result<Vec<String>> {
    // 1. Find all .tar files in images_dir
    // 2. For each tar file:
    //    a. Emit progress
    //    b. Run: docker load -i {tar_file}
    //    c. Parse loaded image name from output
    // 3. Return list of loaded image names
}
```

### P3-4: Docker Installation Flow
**File:** `src-tauri/src/installation/docker.rs`

```rust
pub async fn install_docker_mode(
    req: &StartInstallRequest,
    emit_progress: &ProgressEmitter,
    correlation_id: &str,
) -> Result<InstallArtifacts> {
    // 1. Check Docker installed and running
    // 2. Load Docker images from runtime/linux/docker/images/
    // 3. Generate docker-compose.yml from template
    // 4. Create data directories
    // 5. Run docker-compose up -d
    // 6. Wait for containers healthy
    // 7. Verify with docker-compose ps
    // 8. Return artifacts
}
```

### P3-5: Container Health Check
**File:** `src-tauri/src/installation/docker.rs`

```rust
pub async fn wait_for_containers_healthy(
    compose_path: &Path,
    timeout_secs: u64,
) -> Result<()> {
    // Poll docker-compose ps until all containers are "Up"
    // Or timeout
}

pub async fn get_container_logs(
    container_name: &str,
    lines: u32,
) -> Result<String> {
    // Run: docker logs --tail {lines} {container_name}
}
```

### P3-6: Dockerfile Creation (If Needed)
**File:** `runtime/linux/docker/build/Dockerfile.web`

```dockerfile
FROM mcr.microsoft.com/dotnet/aspnet:8.0-alpine AS runtime

WORKDIR /app
COPY publish/ .

ENV ASPNETCORE_URLS=http://+:8080
EXPOSE 8080

ENTRYPOINT ["dotnet", "Cadalytix.Web.dll"]
```

### P3-7: Docker Build Script
**File:** `runtime/linux/docker/build/build-images.sh`

```bash
#!/bin/bash
set -e

# Build .NET projects
dotnet publish ../../../src/Cadalytix.Web -c Release -o ./publish/web
dotnet publish ../../../src/Cadalytix.Worker -c Release -o ./publish/worker

# Build Docker images
docker build -f Dockerfile.web -t cadalytix-web:latest ./publish/web
docker build -f Dockerfile.worker -t cadalytix-worker:latest ./publish/worker

# Export to tar
docker save cadalytix-web:latest -o ../images/cadalytix-web.tar
docker save cadalytix-worker:latest -o ../images/cadalytix-worker.tar

echo "Docker images built and exported successfully."
```

### Deliverables — Phase 3 ✅ COMPLETE
- [x] `docker-compose.template.yml` with all placeholders — created in `runtime/linux/docker/compose/`
- [x] `generate_compose_file()` template engine — implemented in `docker.rs`
- [x] `load_docker_images()` for .tar loading — implemented in `docker.rs`
- [x] `install_docker_mode()` full flow — implemented in `docker.rs`
- [x] `wait_for_containers_healthy()` health check — implemented in `docker.rs`
- [x] Dockerfile for web and worker services — created in `runtime/linux/docker/build/`
- [x] `build-images.sh` build script — created in `runtime/linux/docker/build/`
- [x] Integration with `start_install` command — wired in `installer.rs`
- [x] Unit tests for Docker functions — 23 tests in `docker.rs`

---

## Phase 4: UI Refactoring & Polish (Days 15-21)

**Objective:** Extract components, add step indicator, improve accessibility and UX.

### P4-1: Extract WizardFrame Component
**File:** `frontend/src/components/WizardFrame.tsx`

Extract lines 108-135 from App.tsx into reusable component.

```tsx
interface WizardFrameProps {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
  currentStep: number;
  totalSteps: number;
  backDisabled: boolean;
  nextDisabled: boolean;
  nextLabel: string;
  cancelDisabled?: boolean;
  onBack: () => void;
  onNext: () => void;
  onCancel: () => void;
}

export function WizardFrame(props: WizardFrameProps) {
  return (
    <div className="wizard-root">
      <div className="wizard-window">
        <div className="wizard-header">
          <h2 className="wizard-title">{props.title}</h2>
          {props.subtitle && <p className="wizard-subtitle">{props.subtitle}</p>}
          <StepIndicator current={props.currentStep} total={props.totalSteps} />
        </div>
        <div className="wizard-content">{props.children}</div>
        <div className="wizard-footer">
          {/* buttons */}
        </div>
      </div>
    </div>
  );
}
```

### P4-2: Create Step Indicator Component
**File:** `frontend/src/components/StepIndicator.tsx`

```tsx
interface StepIndicatorProps {
  current: number;
  total: number;
  labels?: string[];
}

export function StepIndicator({ current, total, labels }: StepIndicatorProps) {
  return (
    <div className="step-indicator" role="progressbar"
         aria-valuenow={current} aria-valuemin={1} aria-valuemax={total}>
      <span className="step-text">Step {current} of {total}</span>
      <div className="step-bar">
        <div className="step-fill" style={{ width: `${(current / total) * 100}%` }} />
      </div>
    </div>
  );
}
```

**CSS additions:**
```css
.step-indicator {
  margin-top: 8px;
}
.step-text {
  font-size: 12px;
  color: #666;
}
.step-bar {
  height: 4px;
  background: #e0e0e0;
  border-radius: 2px;
  margin-top: 4px;
}
.step-fill {
  height: 100%;
  background: #0078d4;
  border-radius: 2px;
  transition: width 0.3s ease;
}
```

### P4-3: Extract Modal Component
**File:** `frontend/src/components/Modal.tsx`

Extract lines 52-106 from App.tsx.

### P4-4: Extract Step Components
Create individual files for each wizard page:

| File | Source Lines | Page |
|------|--------------|------|
| `components/steps/PlatformStep.tsx` | ~50 lines | Platform selection |
| `components/steps/WelcomeStep.tsx` | ~30 lines | Welcome |
| `components/steps/LicenseStep.tsx` | ~60 lines | License |
| `components/steps/InstallTypeStep.tsx` | ~50 lines | Install type |
| `components/steps/DestinationStep.tsx` | ~70 lines | Destination |
| `components/steps/DataSourceStep.tsx` | ~100 lines | Data source |
| `components/steps/DatabaseStep.tsx` | ~200 lines | Database (largest) |
| `components/steps/StorageStep.tsx` | ~80 lines | Storage |
| `components/steps/RetentionStep.tsx` | ~50 lines | Retention |
| `components/steps/ArchiveStep.tsx` | ~100 lines | Archive |
| `components/steps/ConsentStep.tsx` | ~60 lines | Consent |
| `components/steps/MappingStep.tsx` | ~150 lines | Mapping |
| `components/steps/ReadyStep.tsx` | ~80 lines | Ready |
| `components/steps/InstallingStep.tsx` | ~80 lines | Installing |
| `components/steps/CompleteStep.tsx` | ~60 lines | Complete |

### P4-5: Add Splash Screen
**File:** `frontend/src/components/SplashScreen.tsx`

```tsx
export function SplashScreen({ progress }: { progress: number }) {
  return (
    <div className="splash-screen">
      <div className="splash-logo">CADalytix</div>
      <div className="splash-text">Initializing...</div>
      <progress value={progress} max={100} />
    </div>
  );
}
```

### P4-6: Add Password Toggle
**File:** `frontend/src/components/PasswordInput.tsx`

```tsx
interface PasswordInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  id?: string;
}

export function PasswordInput({ value, onChange, placeholder, id }: PasswordInputProps) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="password-input-wrapper">
      <input
        id={id}
        type={visible ? 'text' : 'password'}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="wizard-input"
      />
      <button
        type="button"
        className="password-toggle"
        onClick={() => setVisible(!visible)}
        aria-label={visible ? 'Hide password' : 'Show password'}
      >
        {visible ? '🙈' : '👁️'}
      </button>
    </div>
  );
}
```

### P4-7: Accessibility Improvements
**File:** `frontend/src/App.css`

```css
/* Focus indicators */
.wizard-button:focus-visible,
.wizard-input:focus-visible,
.wizard-select:focus-visible {
  outline: 2px solid #0078d4;
  outline-offset: 2px;
}

/* Skip link */
.skip-link {
  position: absolute;
  left: -9999px;
  top: 0;
  z-index: 100;
}
.skip-link:focus {
  left: 8px;
  top: 8px;
  background: #0078d4;
  color: white;
  padding: 8px 16px;
}

/* Platform card keyboard nav */
.platform-card:focus-visible {
  outline: 3px solid #0078d4;
  outline-offset: 2px;
}
.platform-card[aria-selected="true"] {
  border-color: #0078d4;
  background: #f0f7ff;
}
```

### P4-8: Error Display Improvements
**File:** `frontend/src/components/ErrorBanner.tsx`

```tsx
interface ErrorBannerProps {
  message: string;
  onDismiss?: () => void;
  onRetry?: () => void;
}

export function ErrorBanner({ message, onDismiss, onRetry }: ErrorBannerProps) {
  return (
    <div className="error-banner" role="alert">
      <span className="error-icon">⚠️</span>
      <span className="error-message">{message}</span>
      {onRetry && <button onClick={onRetry} className="error-retry">Retry</button>}
      {onDismiss && <button onClick={onDismiss} className="error-dismiss">×</button>}
    </div>
  );
}
```

### P4-9: Validation on Blur
Update input handlers to validate on blur, not just on Next click.

```tsx
const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});

const handleBlur = (field: string, value: string) => {
  const error = validateField(field, value);
  setFieldErrors(prev => ({ ...prev, [field]: error }));
};
```

### Deliverables — Phase 4
- [ ] `WizardFrame.tsx` extracted
- [ ] `StepIndicator.tsx` with progress bar
- [ ] `Modal.tsx` extracted
- [ ] 15 step components extracted
- [ ] `SplashScreen.tsx` component
- [ ] `PasswordInput.tsx` with show/hide toggle
- [ ] Accessibility CSS improvements
- [ ] `ErrorBanner.tsx` with retry button
- [ ] Validation on blur implementation
- [ ] App.tsx reduced from 2340 to ~300 lines

---

## Phase 5: Cross-Platform Build & Test (Days 22-25)

**Objective:** Build Linux binary, test TUI, validate end-to-end flows.

### P5-1: Linux Build Configuration
**File:** `src-tauri/tauri.conf.json`

Verify Linux bundle configuration:
```json
{
  "bundle": {
    "targets": ["deb", "rpm", "appimage"],
    "linux": {
      "deb": {
        "depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0"]
      }
    }
  }
}
```

### P5-2: Build Linux Binary
**Commands:**
```bash
# On Linux (or WSL2 with cross-compilation)
cd Prod_Install_Wizard_Deployment/installer-unified/src-tauri

# Install Linux target
rustup target add x86_64-unknown-linux-gnu

# Build
cargo tauri build --target x86_64-unknown-linux-gnu
```

### P5-3: TUI Testing Matrix
| Terminal | OS | Test Status |
|----------|-----|-------------|
| GNOME Terminal | Ubuntu 22.04 | [ ] |
| Konsole | Ubuntu 22.04 | [ ] |
| xterm | Ubuntu 22.04 | [ ] |
| SSH (PuTTY) | Windows → Linux | [ ] |
| SSH (Windows Terminal) | Windows → Linux | [ ] |
| WSL2 Terminal | Windows 11 | [ ] |
| macOS Terminal | macOS 14 | [ ] |
| iTerm2 | macOS 14 | [ ] |

### P5-4: Integration Test Scenarios
| Scenario | Mode | Expected Result |
|----------|------|-----------------|
| Fresh Windows install | GUI | Service running |
| Fresh Linux install (native) | TUI | systemd service running |
| Fresh Docker install | TUI | Containers healthy |
| Upgrade existing Windows | GUI | Data preserved |
| Import config | GUI/TUI | Settings loaded |
| Invalid DB connection | GUI/TUI | Graceful error |
| Disk full | GUI/TUI | Preflight fails |
| Cancel mid-install | GUI/TUI | Rollback clean |

### P5-5: Smoke Test Script
**File:** `scripts/smoke-test.sh`

```bash
#!/bin/bash
set -e

# Test TUI smoke mode
./INSTALL --tui-smoke=platform
./INSTALL --tui-smoke=database
./INSTALL --tui-smoke=mapping
./INSTALL --tui-smoke=complete

echo "All smoke tests passed."
```

### Deliverables — Phase 5
- [ ] Linux binary built (.deb, .rpm, .appimage)
- [ ] TUI tested on all terminal emulators
- [ ] Integration tests passing
- [ ] Smoke test script working
- [ ] Cross-platform CI/CD pipeline (optional)

---

## Phase 6: Final Validation (Days 26-28)

**Objective:** End-to-end validation, documentation, release preparation.

### P6-1: End-to-End Test Checklist
- [ ] Windows GUI: Full install flow with SQL Server
- [ ] Windows GUI: Full install flow with PostgreSQL
- [ ] Linux TUI: Full install flow with native systemd
- [ ] Linux TUI: Full install flow with Docker
- [ ] Import existing config file
- [ ] Upgrade from previous version
- [ ] Uninstall and clean reinstall

### P6-2: Documentation Updates
- [ ] Update README.md with Linux instructions
- [ ] Update UNIFIED_CROSS_PLATFORM_INSTALLER_PLAN.md with completion status
- [ ] Create INSTALL_LINUX.md guide
- [ ] Create DOCKER_DEPLOYMENT.md guide
- [ ] Update CHANGELOG.md

### P6-3: Release Artifacts
- [ ] Windows: `CADalytix-Installer-x64.exe`
- [ ] Linux: `cadalytix-installer_1.0.0_amd64.deb`
- [ ] Linux: `cadalytix-installer-1.0.0.x86_64.rpm`
- [ ] Linux: `CADalytix-Installer.AppImage`
- [ ] Docker: `docker-compose.yml` template
- [ ] Docker: Pre-built images (if distributing)

### P6-4: Known Issues Documentation
Document any remaining issues that won't be addressed in v1.0:
- [ ] Firewall configuration (manual)
- [ ] SELinux policy (manual)
- [ ] Dark mode (future)
- [ ] Keyboard shortcuts (future)

### Deliverables — Phase 6
- [ ] All E2E tests passing
- [ ] Documentation complete
- [ ] Release artifacts built
- [ ] Known issues documented
- [ ] Release notes written

---

## File Inventory: All Changes Required

### New Files to Create

| Path | Phase | Lines (Est.) |
|------|-------|--------------|
| `runtime/linux/docker/compose/docker-compose.template.yml` | P3 | 50 |
| `runtime/linux/docker/build/Dockerfile.web` | P3 | 15 |
| `runtime/linux/docker/build/Dockerfile.worker` | P3 | 15 |
| `runtime/linux/docker/build/build-images.sh` | P3 | 30 |
| `frontend/src/components/WizardFrame.tsx` | P4 | 60 |
| `frontend/src/components/StepIndicator.tsx` | P4 | 30 |
| `frontend/src/components/Modal.tsx` | P4 | 60 |
| `frontend/src/components/SplashScreen.tsx` | P4 | 25 |
| `frontend/src/components/PasswordInput.tsx` | P4 | 35 |
| `frontend/src/components/ErrorBanner.tsx` | P4 | 25 |
| `frontend/src/components/steps/PlatformStep.tsx` | P4 | 50 |
| `frontend/src/components/steps/WelcomeStep.tsx` | P4 | 30 |
| `frontend/src/components/steps/LicenseStep.tsx` | P4 | 60 |
| `frontend/src/components/steps/InstallTypeStep.tsx` | P4 | 50 |
| `frontend/src/components/steps/DestinationStep.tsx` | P4 | 70 |
| `frontend/src/components/steps/DataSourceStep.tsx` | P4 | 100 |
| `frontend/src/components/steps/DatabaseStep.tsx` | P4 | 200 |
| `frontend/src/components/steps/StorageStep.tsx` | P4 | 80 |
| `frontend/src/components/steps/RetentionStep.tsx` | P4 | 50 |
| `frontend/src/components/steps/ArchiveStep.tsx` | P4 | 100 |
| `frontend/src/components/steps/ConsentStep.tsx` | P4 | 60 |
| `frontend/src/components/steps/MappingStep.tsx` | P4 | 150 |
| `frontend/src/components/steps/ReadyStep.tsx` | P4 | 80 |
| `frontend/src/components/steps/InstallingStep.tsx` | P4 | 80 |
| `frontend/src/components/steps/CompleteStep.tsx` | P4 | 60 |
| `scripts/smoke-test.sh` | P5 | 20 |
| `docs/INSTALL_LINUX.md` | P6 | 100 |
| `docs/DOCKER_DEPLOYMENT.md` | P6 | 100 |

### Files to Modify

| Path | Phase | Changes |
|------|-------|---------|
| `frontend/src/App.css` | P0, P4 | Add missing classes, accessibility |
| `frontend/src/App.tsx` | P4 | Extract components, reduce to ~300 lines |
| `src-tauri/src/installation/linux.rs` | P1, P2 | Full implementation (~300 lines) |
| `src-tauri/src/installation/docker.rs` | P3 | Template engine, health checks (~150 lines) |
| `src-tauri/src/installation/service.rs` | P2 | Linux systemd functions (~100 lines) |
| `src-tauri/src/api/preflight.rs` | P1 | Linux preflight checks (~80 lines) |
| `src-tauri/src/api/installer.rs` | P2, P3 | Wire Linux/Docker flows |
| `src-tauri/Cargo.toml` | P1 | Add `nix` or `libc` for statvfs |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Linux cross-compile fails | Medium | High | Use native Linux build machine or WSL2 |
| Docker images too large | Medium | Medium | Use Alpine base, multi-stage builds |
| TUI rendering issues | Low | Medium | Test on multiple terminals early |
| systemd permission denied | High | High | Document sudo requirement, detect early |
| UI refactor breaks functionality | Medium | High | Incremental extraction, test each step |

---

## Success Criteria

### Phase 0 Complete When:
- [ ] No CSS errors in browser console
- [ ] Primary button is visually distinct

### Phase 1 Complete When:
- [ ] `preflight_host` returns Linux-specific checks on Linux
- [ ] Disk space and memory checks work

### Phase 2 Complete When:
- [ ] `./INSTALL --tui` on Linux creates running systemd service
- [ ] `systemctl status cadalytix` shows "active (running)"

### Phase 3 Complete When:
- [ ] `./INSTALL --tui` with Docker mode starts containers
- [ ] `docker-compose ps` shows all containers "Up"

### Phase 4 Complete When:
- [ ] App.tsx is under 400 lines
- [ ] Step indicator shows progress
- [ ] All 15 step components exist

### Phase 5 Complete When:
- [ ] Linux .deb/.rpm/.appimage builds successfully
- [ ] All smoke tests pass

### Phase 6 Complete When:
- [ ] All E2E tests pass on Windows and Linux
- [ ] Documentation is complete
- [ ] Release artifacts are built

---

## Appendix A: Command Reference

### Build Commands
```bash
# Windows (current)
cd src-tauri && cargo tauri build

# Linux (on Linux machine)
cd src-tauri && cargo tauri build --target x86_64-unknown-linux-gnu

# TUI smoke test
./INSTALL --tui-smoke=database
```

### Docker Commands
```bash
# Build images
cd runtime/linux/docker/build && ./build-images.sh

# Manual compose test
docker-compose -f runtime/linux/docker/compose/docker-compose.yml up -d
docker-compose ps
docker-compose logs -f
```

### systemd Commands
```bash
# After installation
sudo systemctl status cadalytix
sudo journalctl -u cadalytix -f

# Manual service management
sudo systemctl stop cadalytix
sudo systemctl start cadalytix
sudo systemctl restart cadalytix
```

---

## Appendix B: Dependency Additions

### Cargo.toml (src-tauri)
```toml
[target.'cfg(target_os = "linux")'.dependencies]
nix = { version = "0.27", features = ["fs"] }  # For statvfs
```

### package.json (frontend)
No new dependencies required for UI refactor.

---

## Appendix C: Available Image Assets

**Location:** `frontend/src/assets/` *(already in place)*

These assets are already available in the frontend and should be integrated during Phase 4.

| File | Purpose | Use In |
|------|---------|--------|
| `CADalytix_No_Background_Large.png` | Main logo (large) | Splash screen, Welcome page header |
| `CADalytix_No_Background_Small.png` | Main logo (small) | Wizard header, window title bar |
| `CADalytix_White_Background_Large.jpg` | Logo with white bg (large) | Alt contexts, documentation |
| `CADalytix_White_Background_Small.jpg` | Logo with white bg (small) | Alt contexts, documentation |
| `Windows_Icon_No_Background.png` | Windows platform icon | Platform selection card (Windows) |
| `Docker-Linux_Icon.png` | Docker/Linux platform icon | Platform selection card (Docker/Linux) |
| `SQL_Icon.png` | SQL Server icon | Database page (SQL Server option) |
| `PostgreSQL_Logo_No_Background.png` | PostgreSQL icon | Database page (PostgreSQL option) |
| `Rust_Icon_No_Background.png` | Rust icon | About/Credits (optional) |

### Integration Tasks (Phase 4)

#### P4-10: Import Assets in Components
Assets are already in `frontend/src/assets/`. Import them directly in React components:

```tsx
// Example import in a component
import windowsIcon from '../assets/Windows_Icon_No_Background.png';
import dockerIcon from '../assets/Docker-Linux_Icon.png';
import logoLarge from '../assets/CADalytix_No_Background_Large.png';
import logoSmall from '../assets/CADalytix_No_Background_Small.png';
import sqlIcon from '../assets/SQL_Icon.png';
import postgresIcon from '../assets/PostgreSQL_Logo_No_Background.png';
```

#### P4-11: Platform Card Icons
**File:** `frontend/src/components/steps/PlatformStep.tsx`

```tsx
import windowsIcon from '../../assets/Windows_Icon_No_Background.png';
import dockerIcon from '../../assets/Docker-Linux_Icon.png';

// In component:
<div className="platform-grid">
  <button
    className={`platform-card ${installMode === 'windows' ? 'selected' : ''}`}
    onClick={() => setInstallMode('windows')}
  >
    <img src={windowsIcon} alt="" className="platform-icon" />
    <h3 className="platform-card-title">Windows</h3>
    <p className="platform-card-body">Install as Windows Service with IIS hosting</p>
  </button>

  <button
    className={`platform-card ${installMode === 'docker' ? 'selected' : ''}`}
    onClick={() => setInstallMode('docker')}
  >
    <img src={dockerIcon} alt="" className="platform-icon" />
    <h3 className="platform-card-title">Docker / Linux</h3>
    <p className="platform-card-body">Deploy as Docker containers or native Linux service</p>
  </button>
</div>
```

#### P4-12: Database Option Icons
**File:** `frontend/src/components/steps/DatabaseStep.tsx`

```tsx
import sqlIcon from '../../assets/SQL_Icon.png';
import postgresIcon from '../../assets/PostgreSQL_Logo_No_Background.png';

// In component:
<div className="db-type-selector">
  <label className="db-type-option">
    <input type="radio" name="dbType" value="sqlserver" />
    <img src={sqlIcon} alt="" className="db-icon" />
    <span>SQL Server</span>
  </label>
  <label className="db-type-option">
    <input type="radio" name="dbType" value="postgresql" />
    <img src={postgresIcon} alt="" className="db-icon" />
    <span>PostgreSQL</span>
  </label>
</div>
```

#### P4-13: Splash Screen Logo
**File:** `frontend/src/components/SplashScreen.tsx`

```tsx
import logoLarge from '../assets/CADalytix_No_Background_Large.png';

export function SplashScreen({ progress }: { progress: number }) {
  return (
    <div className="splash-screen">
      <img
        src={logoLarge}
        alt="CADalytix"
        className="splash-logo-img"
      />
      <div className="splash-text">Initializing...</div>
      <progress value={progress} max={100} />
    </div>
  );
}
```

#### P4-14: Wizard Header Logo
**File:** `frontend/src/components/WizardFrame.tsx`

```tsx
import logoSmall from '../assets/CADalytix_No_Background_Small.png';

// In component:
<div className="wizard-header">
  <div className="wizard-header-row">
    <img
      src={logoSmall}
      alt=""
      className="wizard-logo"
    />
    <h2 className="wizard-title">{props.title}</h2>
  </div>
  {props.subtitle && <p className="wizard-subtitle">{props.subtitle}</p>}
  <StepIndicator current={props.currentStep} total={props.totalSteps} />
</div>
```

### CSS for Image Integration
**File:** `frontend/src/App.css`

```css
/* Platform card icons */
.platform-icon {
  width: 48px;
  height: 48px;
  margin-bottom: 12px;
  object-fit: contain;
}

/* Database type icons */
.db-icon {
  width: 24px;
  height: 24px;
  margin-right: 8px;
  object-fit: contain;
  vertical-align: middle;
}

/* Splash screen logo */
.splash-logo-img {
  width: 200px;
  height: auto;
  margin-bottom: 24px;
}

/* Wizard header logo */
.wizard-logo {
  width: 32px;
  height: 32px;
  margin-right: 12px;
  object-fit: contain;
}

.wizard-header-row {
  display: flex;
  align-items: center;
}
```

### Updated Deliverables — Phase 4 (with Images)
- [x] Image assets already in `frontend/src/assets/` *(no copy needed)*
- [ ] Platform cards show Windows/Docker icons (`Windows_Icon_No_Background.png`, `Docker-Linux_Icon.png`)
- [ ] Database options show SQL Server/PostgreSQL icons (`SQL_Icon.png`, `PostgreSQL_Logo_No_Background.png`)
- [ ] Splash screen displays CADalytix logo (`CADalytix_No_Background_Large.png`)
- [ ] Wizard header includes small logo (`CADalytix_No_Background_Small.png`)
- [ ] All images have proper `alt` attributes for accessibility
- [ ] TypeScript declarations for `.png`/`.jpg` imports if needed

---

*End of Document*


