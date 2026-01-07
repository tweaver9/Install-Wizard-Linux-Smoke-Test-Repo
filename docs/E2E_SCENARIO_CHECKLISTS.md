# E2E Scenario Checklists

> Phase 6 deliverable: Human-run checklists for end-to-end validation.
> Each scenario produces evidence files with standardized naming.

## Evidence Naming Convention

All evidence files follow this pattern:
```
E2E_{SCENARIO}_{YYYYMMDD}_{HHMMSS}_{RESULT}.{ext}
```

Examples:
- `E2E_WIN_GUI_20260107_143022_PASS.log`
- `E2E_LINUX_DOCKER_20260107_150512_FAIL.log`
- `E2E_UPGRADE_20260107_161234_PASS.log`

---

## Scenario 1: Windows GUI Installation

**Scenario ID:** `WIN_GUI`  
**Prerequisites:**
- [ ] Windows 10/11 or Windows Server 2019+
- [ ] SQL Server instance accessible (or Postgres)
- [ ] Admin privileges

**Steps:**

1. [ ] Launch `installer-unified.exe`
2. [ ] Verify Welcome screen displays version and copyright
3. [ ] Accept license agreement
4. [ ] Select destination folder (default or custom)
5. [ ] Choose "Use EXISTING Database"
6. [ ] Select hosting provider (on-prem, AWS RDS, Azure SQL, etc.)
7. [ ] Enter connection details and click "Test Connection"
   - [ ] Verify success message on valid credentials
   - [ ] Verify user-friendly error on invalid credentials
8. [ ] Configure storage settings
9. [ ] Set hot retention period (months)
10. [ ] Configure archive policy (format, destination, schedule)
11. [ ] Review consent checkbox
12. [ ] Complete field mapping (auto-detect or manual)
13. [ ] Click "Install" on Ready screen
14. [ ] Verify progress bar advances without stalling
15. [ ] Verify completion message with artifacts path
16. [ ] Check `Prod_Wizard_Log/` for proof logs

**Evidence to collect:**
- Screenshot of completion screen
- `install-manifest.json` from artifacts
- `Prod_Wizard_Log/*.log` files

---

## Scenario 2: Linux Docker Installation

**Scenario ID:** `LINUX_DOCKER`  
**Prerequisites:**
- [ ] Linux host with Docker installed
- [ ] Postgres instance accessible
- [ ] Network access to container registry

**Steps:**

1. [ ] Pull installer image: `docker pull cadalytix/installer:latest`
2. [ ] Run TUI installer: `docker run -it cadalytix/installer --tui`
3. [ ] Navigate through wizard screens using keyboard
4. [ ] Enter Postgres connection string
5. [ ] Verify "Test Connection" succeeds
6. [ ] Complete all configuration screens
7. [ ] Verify installation completes
8. [ ] Check container logs for proof artifacts

**Evidence to collect:**
- Terminal session recording (asciinema or script)
- Container logs: `docker logs <container_id>`
- Proof logs from mounted volume

---

## Scenario 3: Linux Native Installation

**Scenario ID:** `LINUX_NATIVE`  
**Prerequisites:**
- [ ] Ubuntu 22.04+ or RHEL 8+
- [ ] Postgres instance accessible
- [ ] Rust toolchain (for building from source)

**Steps:**

1. [ ] Build: `cargo build --release`
2. [ ] Run TUI: `./target/release/installer-unified --tui`
3. [ ] Complete wizard flow
4. [ ] Verify systemd service unit generated
5. [ ] Check proof logs in `Prod_Wizard_Log/`

**Evidence to collect:**
- Build output log
- TUI session transcript
- Generated service unit file

---

## Scenario 4: Upgrade from Previous Version

**Scenario ID:** `UPGRADE`  
**Prerequisites:**
- [ ] Existing CADalytix installation (v1.x or v2.x)
- [ ] Backup of existing database
- [ ] Backup of existing config files

**Steps:**

1. [ ] Stop existing CADalytix service
2. [ ] Backup `appsettings.json` and database
3. [ ] Run new installer with "Import" installation type
4. [ ] Verify existing settings are detected
5. [ ] Verify migrations are applied (if any)
6. [ ] Verify service restarts successfully
7. [ ] Verify data integrity post-upgrade

**Evidence to collect:**
- Pre-upgrade config backup
- Migration log
- Post-upgrade verification log

---

## Failure Scenarios (Negative Testing)

### F1: Invalid Database Credentials
- [ ] Enter wrong password
- [ ] Verify user-friendly error (no stack trace)
- [ ] Verify password not logged

### F2: Network Unreachable
- [ ] Use non-routable IP (e.g., 10.255.255.1)
- [ ] Verify timeout message within 30 seconds
- [ ] Verify graceful failure

### F3: Insufficient Disk Space
- [ ] Set destination to nearly-full drive
- [ ] Verify pre-flight check catches issue
- [ ] Verify clear error message

### F4: Cancel Mid-Installation
- [ ] Start installation
- [ ] Click Cancel at 50% progress
- [ ] Verify clean rollback
- [ ] Verify re-entry works

---

## Evidence Submission

After completing a scenario, submit evidence to:
```
Prod_Wizard_Log/E2E_EVIDENCE/
```

Include:
1. Checklist with timestamps
2. Screenshots (if GUI)
3. Log files
4. `install-manifest.json`

