### CADalytix Unified Installer - Phase 5 Execution Playbook (Single Reference)

This file exists so that **any time something is confusing**, we can point to **one authoritative checklist** for Phase 5: what "done" means, what proof artifacts must exist, and which commands/logs validate it.

---

## Goals (Phase 5)

- **Classic Windows installer wizard UX** implemented in:
  - **Windows GUI** (Tauri/React)
  - **Linux GUI** (same UI, same flow)
  - **Linux headless TUI** (installer-window style)
- **Event-driven install contract**:
  - UI becomes a **pure renderer** driven by backend events.
  - Exactly one terminal event per install run: `install-complete` OR `install-error`.
- **Mapping is real**:
  - Header scan returns **real headers** (or explicitly labeled demo mode).
  - Duplicates are disambiguated with **stable source IDs**.
  - Required-target gating + Replace/Add confirmations + Unlink rule.
- **Persistence**:
  - Writes portable artifacts on completion:
    - `mapping.json`
    - `install-manifest.json`
    - (and `install-config.json` as internal record)
  - Manifest includes sha256 checksums.
- **Archive pipeline**:
  - Engine-agnostic archival pipeline skeleton with strict verification steps.
  - Deterministic `--archive-dry-run` produces stable proof logs.

---

## Remaining Deliverables (Updated / Authoritative)

### D2 - Database Setup Wizard (New vs Existing) [AUTHORITATIVE FLOW] ✅ DONE

**Status**: Implemented in GUI (App.tsx), TUI (tui/mod.rs), Backend (api/installer.rs)
**Proof mode**: `--db-setup-smoke` → `D2_db_setup_smoke_transcript.log`

**DB Setup page (first decision)** must display:

- **Title**: `Database Setup`
- **Prompt**: `Do you want CADalytix to create a NEW database, or use an EXISTING database?`
- **Buttons**:
  - `Create NEW CADalytix Database`
  - `Use EXISTING Database`

#### D2A - Create NEW CADalytix Database branch

Page prompt: `Where should the new CADalytix database be created?`

Options:
- `This machine (default location)`
- `Specific drive / path (advanced)`

Then collect (required fields):
- Max DB size / storage allocation
- Hot retention window (12/18/etc months)
- Archive schedule time (default 12:05am on 1st)
- Archive destination (path)
- Archive file type (CSV/JSON/etc)

Rules:
- Do NOT ask for provider selection on the New DB branch.
- **No disk partitioning**: The installer does NOT partition disks or create filesystems. It only validates free space and stores the cap/policy.
- Create NEW provisioning is not yet implemented; backend emits fail-fast message guiding user to choose "Use EXISTING".

Acceptance:
- ✅ UI (GUI + TUI) captures all fields and includes them in the final install payload.
- ✅ Backend refuses to proceed if any required field is missing (max_db_size_gb > 0, path if specific_path).
- ✅ Backend emits fail-fast error for Create NEW until provisioning is implemented.

#### D2B - Use EXISTING Database branch

Page prompt: `Where is the existing database hosted? (No login required)`

Options (required selection):
- `On-prem / self-hosted / unknown`
- `AWS RDS / Aurora`
- `Azure SQL / SQL MI`
- `GCP Cloud SQL`
- `Neon`
- `Supabase`
- `Other`

Then: `How do you want to connect?`
- `Connection string`
- `Enter connection details (host/server, port, db name, username, password, TLS)`

Must display this text on the page:

`CADalytix does not ask you to log in to AWS/Azure/GCP and does not scan your cloud. You only provide a database endpoint (connection string or host/port/user/password) with explicit permissions.`

Rules:
- No connection attempt until required fields for the chosen connection mode are present.
- Secrets must be masked in logs/transcripts.
- Next button is disabled until Test Connection succeeds.

Acceptance:
- "Test connection" works and logs a masked transcript.
- Backend blocks connect until required inputs exist (string OR manual fields).
- After successful connect, object/header discovery for mapping becomes available.

#### D2 Persistence (install-manifest.json)

The following D2 decisions are persisted (no secrets):
- `Database:SetupMode` (create_new | existing)
- `Database:NewLocation` (this_machine | specific_path)
- `Database:NewSpecificPath` (path if specific)
- `Database:MaxDbSizeGb` (integer)
- `Database:ExistingHostedWhere` (on_prem | aws_rds | azure_sql | gcp_cloud_sql | neon | supabase | other)
- `Database:ExistingConnectMode` (connection_string | details)
- Connection string fingerprint (sha256 of masked string, NOT the actual secret)


### D3 - Schema Mapping (GUI + TUI hardening + persistence)

Acceptance:
- Mapping persist proof transcript exists and shows duplicates + stable IDs + gating + final `mapping.json` path.

### D4 - Retention + Archive Policy (full "verified order" pipeline)

Acceptance:
- Archive dry-run transcript proves verified order (1→6), idempotent behavior, and correct destination checks.

### D5 - Real install orchestration (Phase 5 closer)

Acceptance:
- Progress events + exactly one terminal event (complete/error).
- Cancel produces terminal "cancelled" error and stops further work.

---

## Non-Negotiables (Phase 5)

- **No stepper** UI in any variant.
- **Bottom buttons** only: `[Back] [Next] [Cancel]` (Finish replaces Next at end).
- **Cancel is real**:
  - UI shows confirm modal.
  - Backend cancellation is requested.
  - Install pipeline checks cancel between steps.
  - Terminal event emitted: `install-error` with "cancelled" message is acceptable.
- **No silent failures**:
  - Errors logged with context.
  - UI shows user-friendly message.
- **No sensitive data in logs**:
  - Never log passwords or full connection strings.

---

## Single Source of Truth - Event Contract

### Event names (canonical)
- `progress`
- `install-complete`
- `install-error`

### Canonical progress payload shape (Rust → GUI/TUI)
- `phase`
- `step`
- `percent`
- `message`
- `severity`
- `correlation_id`
- (optional) `elapsed_ms`, `eta_ms`

### Contract rules
- `start_install(...)` returns immediately and begins background work.
- Emits **3+ progress** events.
- Emits **exactly one** terminal event.
- Rejects re-entry: second `start_install` while running returns "already running".

---

## Smoke / Proof Modes (Deterministic)

All proof artifacts must land under `Prod_Wizard_Log/` with stable names.

### Install contract smoke
- **Command**: `installer-unified.exe --install-contract-smoke`
- **Proof logs**:
  - `B1_install_contract_smoke_transcript.log`
  - `B1_install_contract_smoke_events_only.log`
- **Must show**:
  - re-entry guard proof
  - 3+ progress events
  - one terminal event
  - cancel path produces terminal error with "cancel"

### Archive dry-run proof
- **Command**: `installer-unified.exe --archive-dry-run`
- **Proof logs**:
  - `B2_archive_pipeline_dryrun_transcript.log`
  - `B2_archive_pipeline_dryrun_ledger.json`
  - `B2_archive_schedule_placeholders/` (placeholder schedule artifacts)
- **Must show** (in the transcript):
  - `schedule placeholder` lines showing placeholder paths written
  - `idempotent: run twice -> second skips ...`
  - `skip` line
  - `ExitCode=0`

### Mapping contract + persistence smoke (Deliverable D1)
- **Command**: `installer-unified.exe --mapping-persist-smoke`
- **Proof log**:
  - `B3_mapping_persist_smoke_transcript.log`
- **Must show** (in the transcript):
  - discovered headers list containing duplicates
  - stable source IDs for duplicates (e.g., `City__0`, `City__1`)
  - required-target gating (blocked + missing list → then unblocked)
  - Replace mapping modal text + decisions
  - Add/Override modal text + decisions
  - unlink rule line
  - `mapping.json written path=...`
  - explicit duplicate proof line: `duplicates persisted distinctly ...`

### TUI smoke (non-interactive)
- **Command**: `installer-unified.exe --tui-smoke=<target>`
- **Targets**:
  - `welcome|license|destination|db|storage|retention|archive|consent|mapping|ready|progress`
- **Exit**: must be `ExitCode=0`
- **Rule**: TUI smoke may seed sample UI state; real TUI runs must not.

---

 I'm listing exact folders/files and the important sections/symbols inside each that define the Phase 5 contracts.


*****FILE/FOLDER REFERENCE******

1) MUST-HAVE Docs (authoritative)
UNIFIED_CROSS_PLATFORM_INSTALLER_PLAN.md
Key sections: Phase 5 UI spec, event contract expectations, logging requirements, cancellation requirements, cross-platform launcher behavior.
Prod_Install_Wizard_Deployment/PHASE5_EXECUTION_PLAYBOOK.md
Key sections: D2-D5 deliverables, canonical event names/payload, smoke/proof modes + stable log names, hang recovery rules, current status.


2) The Actual Phase 5 App (minimum code surface)
Everything below is under the deployment app that Phase 5 work is happening in:
2.1 Backend (Rust / Tauri) - the real "contract source"
Root: Prod_Install_Wizard_Deployment/installer-unified/src-tauri/
src/main.rs
Why it matters: CLI/proof entrypoints (--archive-dry-run, --install-contract-smoke, --mapping-persist-smoke, --tui-smoke=*) and Linux launcher decision.
src/lib.rs
Why it matters: run_gui(), run_tui(), and the proof runners (run_*_smoke) wiring + logging init.
src/tauri.conf.json
Why it matters: desktop app configuration (window/title/icon/capabilities).
Core installer API (Phase 5 "truth")
src/api/installer.rs (the most important file in Phase 5)
Event contract: EVENT_PROGRESS, EVENT_INSTALL_COMPLETE, EVENT_INSTALL_ERROR
Install pipeline: start_install, cancel_install, run_installation
Progress payload: ProgressPayload
Terminal payload: InstallResultEvent, InstallArtifacts
DB setup contract: DbSetupConfig, validation inside start_install
Persistence outputs: build_mapping_json_bytes, build_install_manifest_json_bytes, writing mapping.json / install-manifest.json
Proof modes: install_contract_smoke, mapping_persist_smoke
Connection helpers: test_db_connection, connect_with_retry, guess_engine, validate_connection_string_for_engine
Preflight + header scan (mapping feed)
src/api/preflight.rs
Why it matters: preflight_datasource supports demo_mode and emits duplicate headers (used for mapping demo + proof).
TUI (headless Linux installer window)
src/tui/mod.rs
Smoke boundary: new_real_wizard_state() vs new_smoke_wizard_state() and smoke(...)
Wizard pages: enum Page, draw(...)
DB Setup page: Page::Database rendering + T test connection flow
Mapping UX: attempt_map, apply_mapping, modals (Replace/Add/Cancel), required gating, unlink rule, stable IDs
StartInstallRequest building: the section that builds StartInstallRequest and includes mapping_state
Archive pipeline proof
src/archiver/mod.rs
Proof: archive_dry_run(), deterministic transcript + idempotency, schedule placeholder artifacts
Utilities you can't ignore (logs/paths/security)
src/utils/path_resolver.rs
Why it matters: where Prod_Wizard_Log/ is resolved; where runtime/deployment folders are resolved.
src/utils/logging.rs
Why it matters: masking helpers (no secrets in logs).
src/utils/disk.rs
Why it matters: free space detection for folder/drive paths.
src/security/secret_protector.rs + src/security/crypto.rs
Why it matters: encryption-at-rest + sha256 helpers.
DB persistence layer (settings + mapping)
src/database/platform_db.rs
Why it matters: instance settings persistence (encrypted where applicable), artifact path persistence.
src/database/schema_mapping.rs
Why it matters: mapping persistence in DB (canonical field ↔ source).
src/database/migrations.rs
Why it matters: migration runner + checksum validation, applied migrations list.
src/database/connection.rs
Why it matters: connects to Postgres/SQL Server and is used by installer + preflight.

3) GUI (React) - the user-facing wizard
Root: Prod_Install_Wizard_Deployment/installer-unified/frontend/
src/App.tsx (main wizard UI)
Wizard flow: type WizardPage, navigation order/back/next gating
Event listeners: listenToEvent('progress'|'install-complete'|'install-error')
DB Setup (D2): the "Create NEW" vs "Use EXISTING" UI and required gating
Mapping (D3): disambiguateSourceColumns (stable IDs), attemptMap (Replace/Add/Cancel), unlink rule, required gating
Install payload: invoke('start_install', { payload: ... }) - MUST include dbSetup/storage/retention/archive/consent/mapping_state
src/App.css
Why it matters: MSI-style wizard look; button row; mapping layout; progress bar styling.
src/lib/api.ts
Why it matters: typed event/listen helpers + preflight calls.
package.json / tsconfig.json / vite.config.ts
Why it matters: build gates and TS target constraints.


4) Runtime payload + migrations (inputs required by install)
These are not "code", but they're required artifacts Phase 5 depends on:
Prod_Install_Wizard_Deployment/runtime/
runtime/windows/, runtime/linux/, runtime/shared/
Why it matters: installer checks that runtime/ exists before deployment steps.
Prod_Install_Wizard_Deployment/installer/migrations/
Why it matters: migration bundle + manifests used by the migration runner.


5) Proof Outputs (stable names the AI must produce/verify)

Root: Prod_Wizard_Log/ (outputs only; don't treat as source)


Install contract proof:
B1_install_contract_smoke_transcript.log
B1_install_contract_smoke_events_only.log


Archive proof:
B2_archive_pipeline_dryrun_transcript.log
B2_archive_pipeline_dryrun_ledger.json
B2_archive_schedule_placeholders/
Mapping persistence proof:
B3_mapping_persist_smoke_transcript.log
B3_mapping_persist_smoke_artifacts/mapping.json
6) "Minimum Handoff Bundle" (copy/paste list)
If you wanted to give Phase 5 to another AI, the minimum index is:

UNIFIED_CROSS_PLATFORM_INSTALLER_PLAN.md
Prod_Install_Wizard_Deployment/PHASE5_EXECUTION_PLAYBOOK.md
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src/main.rs
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src/lib.rs
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src/api/installer.rs
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src/api/preflight.rs
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src/tui/mod.rs
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src/archiver/mod.rs
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src/utils/{path_resolver.rs,logging.rs,disk.rs}
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src/security/{secret_protector.rs,crypto.rs}
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src/database/{connection.rs,migrations.rs,platform_db.rs,schema_mapping.rs}
Prod_Install_Wizard_Deployment/installer-unified/src-tauri/tauri.conf.json
Prod_Install_Wizard_Deployment/installer-unified/frontend/src/{App.tsx,App.css,lib/api.ts}
Prod_Install_Wizard_Deployment/installer-unified/frontend/{package.json,tsconfig.json,vite.config.ts}
Prod_Install_Wizard_Deployment/runtime/ (folders)
Prod_Install_Wizard_Deployment/installer/migrations/ (folders + manifests)

That set is enough to implement D2-D5 without needing the rest of the repo.


## "No sample data leak" boundary (TUI)

- **Smoke path** (`--tui-smoke=*`):
  - Allowed to seed sample state for deterministic rendering.
- **Real path** (interactive TUI):
  - Must start from real installer state only (no seeded sample values).
- Acceptance:
  - There is a clear boundary in code: `if smoke { seed } else { real state }`.

---

## Mapping Contract (GUI + TUI)

### Stable source IDs (duplicates disambiguation)
- Use stable IDs like: `SanitizedName__ordinal`
  - Example: `City__0`, `City__1`
  - Ordinal is 0-based within duplicates of the same raw header name.

### Required targets gating
- If required targets are unmapped:
  - Next is blocked.
  - UI shows the missing required list.

### Replace/Add confirmation rules
- **Target already mapped** → modal: Replace/Cancel
- **Source already mapped**:
  - override OFF → Replace/Cancel
  - override ON → Add/Replace/Cancel

### Unlink rule
- Selecting an already-mapped pair toggles it off (unassign).

---

## Build/Test Gates (must be logged)

### Rust
- `cargo fmt --check`
- `cargo check --locked`
- `cargo test --locked` (must include `test result: ok`)

### Frontend
- `npm run build`

All outputs and ExitCodes must be captured under `Prod_Wizard_Log/`.

---

## Hang Recovery (when terminals get "stuck")

- If any command hangs > 5 minutes:
  - kill build processes: `taskkill /F /IM cargo.exe`
  - (if needed) `cargo clean`
  - retry once
  - log timeout + recovery action to `Prod_Wizard_Log/`

---

## Current Phase 5 Status (living section)

**Last updated**: 2026-01-07

- **Done**
  - Install contract smoke proof exists (B1 logs).
  - Archive dry-run proof exists (B2 logs + schedule placeholders).
  - TUI smoke targets expanded (storage/retention/archive/consent/ready).
  - Deliverable D1: mapping persist smoke transcript + mapping.json proof.
  - Deliverable D2: DB Setup Wizard (New vs Existing) - COMPLETE
    - `--db-setup-smoke` proof mode implemented
    - D2_db_setup_smoke_transcript.log exists with all required phrases
    - GUI + TUI + Backend validation implemented
    - "Create NEW" branch: fail-fast until provisioning implemented
    - "Use EXISTING" branch: provider list + Test Connection gate
    - Secrets masked in logs
  - Deliverable D3: Schema Mapping hardening - per B3 logs
  - Deliverable D4: Retention + Archive Policy - per B2 logs
  - Deliverable D5: Install orchestration - per B1 logs (re-entry guard, cancel, terminal events)
- **Remaining**
  - Wire GUI pages to real payload end-to-end for actual deployment (not smoke).
  - Real install orchestration steps (Windows + Linux/Docker).

