# CADalytix Unified Installer — Customer-Ready Checklist

**Generated**: 2026-01-07  
**Purpose**: Track all deliverables needed for 100% customer/sale-ready state.

---

## Audit Summary

### Existing Proof Logs (under Prod_Wizard_Log/)

| Proof Artifact | Status | Path |
|----------------|--------|------|
| B1_install_contract_smoke_transcript.log | ✅ EXISTS | Prod_Wizard_Log/B1_install_contract_smoke_transcript.log |
| B1_install_contract_smoke_events_only.log | ✅ EXISTS | Prod_Wizard_Log/B1_install_contract_smoke_events_only.log |
| B2_archive_pipeline_dryrun_transcript.log | ✅ EXISTS | Prod_Wizard_Log/B2_archive_pipeline_dryrun_transcript.log |
| B2_archive_pipeline_dryrun_ledger.json | ✅ EXISTS | Prod_Wizard_Log/B2_archive_pipeline_dryrun_ledger.json |
| B2_archive_schedule_placeholders/ | ✅ EXISTS | Prod_Wizard_Log/B2_archive_schedule_placeholders/ |
| B3_mapping_persist_smoke_transcript.log | ✅ EXISTS | Prod_Wizard_Log/B3_mapping_persist_smoke_transcript.log |
| B3_mapping_persist_smoke_artifacts/mapping.json | ✅ EXISTS | Prod_Wizard_Log/B3_mapping_persist_smoke_artifacts/mapping.json |
| D2_db_setup_smoke_transcript.log | ✅ EXISTS | Prod_Wizard_Log/D2_db_setup_smoke_transcript.log |

### Build Gate Status (2026-01-07 Audit)

| Gate | Status | Log File |
|------|--------|----------|
| cargo fmt --check | ✅ PASS (after fmt apply) | AUDIT_cargo_fmt_apply.log |
| cargo check --locked | ✅ PASS | AUDIT_cargo_check_locked.log |
| cargo test --locked | ✅ PASS (8 tests) | AUDIT_cargo_test_locked.log |
| npm run build | ✅ PASS | AUDIT_npm_run_build.log |

---

## Phase 5 — Installation Logic (Authoritative: PHASE5_EXECUTION_PLAYBOOK.md)

### D1 — Mapping Contract + Persistence ✅ DONE

| Item | Owner | Files | Command | Proof | Done Condition |
|------|-------|-------|---------|-------|----------------|
| Mapping persist smoke | AI | api/installer.rs, tui/mod.rs | `--mapping-persist-smoke` | B3_*.log | Transcript shows duplicates, stable IDs, gating, modals, mapping.json path |

### D2 — Database Setup Wizard (New vs Existing) ✅ DONE

| Item | Owner | Files | Command | Proof | Done Condition |
|------|-------|-------|---------|-------|----------------|
| D2A: Create NEW flow (GUI) | AI | frontend/src/App.tsx | Manual test | D2_db_setup_smoke_transcript.log | UI shows location + max size fields, fail-fast message for provisioning |
| D2A: Create NEW flow (TUI) | AI | tui/mod.rs | `--tui-smoke=db` | Log | TUI renders NEW DB branch with location + size fields |
| D2A: Backend validation | AI | api/installer.rs | `--db-setup-smoke` | D2_*.log | Backend validates required fields, shows fail-fast message |
| D2B: Use EXISTING flow (GUI) | AI | frontend/src/App.tsx | Manual test | D2_*.log | Provider dropdown, conn mode, disclaimer shown |
| D2B: Use EXISTING flow (TUI) | AI | tui/mod.rs | `--tui-smoke=db` | Log | TUI renders EXISTING branch + T test |
| D2B: Test connection (masked log) | AI | api/installer.rs | `--db-setup-smoke` | D2_*.log | Connection attempt logged with masked secrets |
| D2 Proof mode | AI | api/installer.rs, lib.rs, main.rs | `--db-setup-smoke` | D2_db_setup_smoke_transcript.log | Transcript shows both branches, validation, masking |

### D3 — Schema Mapping Hardening ✅ DONE (per B3 log)

| Item | Owner | Files | Command | Proof | Done Condition |
|------|-------|-------|---------|-------|----------------|
| Duplicates disambiguation | AI | api/installer.rs, App.tsx | `--mapping-persist-smoke` | B3_*.log | Stable source IDs (City__0, City__1) |
| Required-target gating | AI | App.tsx, tui/mod.rs | N/A | B3 log line | blocked=true/false + missing list |
| Replace/Add/Cancel modal | AI | App.tsx, tui/mod.rs | N/A | B3 log line | Modal text + decision logged |
| Unlink rule | AI | App.tsx, tui/mod.rs | N/A | B3 log line | "unlink rule: ..." |
| Persist mapping.json | AI | api/installer.rs | N/A | B3_mapping_persist_smoke_artifacts/mapping.json | File exists + content correct |

### D4 — Retention + Archive Policy ✅ DONE (per B2 log)

| Item | Owner | Files | Command | Proof | Done Condition |
|------|-------|-------|---------|-------|----------------|
| Archive dry-run proof | AI | archiver/mod.rs | `--archive-dry-run` | B2_*.log | Shows verified order 1..6, idempotency, skip line |
| Schedule placeholders | AI | archiver/mod.rs | N/A | B2_archive_schedule_placeholders/ | .ps1, .service, .timer files |
| Ledger JSON | AI | archiver/mod.rs | N/A | B2_archive_pipeline_dryrun_ledger.json | Exists |

### D5 — Real Install Orchestration ✅ DONE

| Item | Owner | Files | Command | Proof | Done Condition |
|------|-------|-------|---------|-------|----------------|
| 3+ progress events | AI | api/installer.rs | `--install-contract-smoke` | B1_*.log | progress_events=3+ ✅ |
| Exactly one terminal event | AI | api/installer.rs | N/A | B1 log | terminal_events=1 ✅ |
| Re-entry guard | AI | api/installer.rs | N/A | B1 log line | guard_try_begin shows rejection ✅ |
| Cancel path | AI | api/installer.rs | N/A | B1 log | "Installation cancelled." terminal ✅ |

**Note**: Real Windows/Linux service installation is Phase 7/8 scope. Phase 5 proves the contract; Phase 7/8 implements the actual deployment steps.

---

## Phase 6 — Testing + Validation ⬜ IN PROGRESS

**Purpose**: Make correctness repeatable, failures diagnosable, regressions protected.

### 6.0 — Stable Log Naming + Proof Discipline

| Item | Stable Log Name | Description |
|------|-----------------|-------------|
| Windows smoke | P6_smoke_windows.log | Master summary of all proof modes |
| Linux smoke | P6_smoke_linux.log | Master summary of all proof modes |
| Unit tests | P6_unit_tests.log | cargo test --lib output |
| Connection failure test | P6_connection_failure_deterministic.log | Behavioral timeout/failure proof |

### 6.1 — Smoke Test Scripts

| Item | Status | Files | Command | Proof | Done Condition |
|------|--------|-------|---------|-------|----------------|
| Windows smoke script | ✅ DONE | tools/smoke-test-unified-installer.ps1 | Run script | P6_smoke_windows.log | All proof modes + tui-smokes pass, ExitCode=0 |
| Linux smoke script | ⚠️ CREATED / NOT EXECUTED | tools/smoke-test-unified-installer.sh | Run script | P6_smoke_linux.log | **DONE when:** P6_smoke_linux.log exists with ExitCode=0 (generated by CI or WSL) |

### 6.2 — Unit Tests (High-Risk Regression Locks) ✅ DONE

46 unit tests passing. See P6_unit_tests.log.

| Item | Status | Files | Test Name | Done Condition |
|------|--------|-------|-----------|----------------|
| D2 New DB validation | ✅ | api/installer.rs | db_setup_create_new_* (4 tests) | Required fields enforced |
| D2 Existing DB validation | ✅ | api/installer.rs | db_setup_existing_* (2 tests) | Connection details required |
| Secret masking | ✅ | utils/logging.rs | mask_* (14 tests) | No raw password in logs |
| Connection string validation | ✅ | api/installer.rs | validate_*_connection_string_* (7 tests) | Format validation |
| Terminal contract | ✅ | api/installer.rs | progress_payload_*, install_result_* (3 tests) | Correct serialization |
| Error message safety | ✅ | api/installer.rs | connection_*_error_* (4 tests) | User-friendly, no leaks |

### 6.3 — Behavioral Connection Failure Tests (Deterministic, No Real DB)

| Item | Status | Files | Done Condition |
|------|--------|-------|----------------|
| Connection timeout deterministic | ✅ DONE | database/connection.rs | Test completes in <3s, times out stub, user-friendly error |
| Immediate failure path | ✅ DONE | database/connection.rs | Stub returns controlled failure, error is friendly |
| Masking under failure | ✅ DONE | database/connection.rs | Logs contain only masked secrets |
| Retry bounded | ✅ DONE | database/connection.rs | Retry count capped, no infinite loop |

**Proof**: P6_connection_failure_deterministic.log with ExitCode=0

### 6.4 — Feature-Gated Real DB Tests (Optional)

| Item | Owner | Files | Feature Flag | Done Condition |
|------|-------|-------|--------------|----------------|
| PostgreSQL integration | AI | database/*.rs | `--features test-postgres` | Migrations + CRUD tested |
| SQL Server integration | AI | database/*.rs | `--features test-sqlserver` | Migrations + CRUD tested |

### 6.5 — E2E Scenario Checklists ✅ DONE

Document: `docs/E2E_SCENARIO_CHECKLISTS.md`

| Scenario | Prerequisite | Evidence Required |
|----------|--------------|-------------------|
| Fresh Windows GUI | Use EXISTING DB | E2E_WIN_GUI_{date}_{result}.log |
| Fresh Linux Docker | Use EXISTING DB | E2E_LINUX_DOCKER_{date}_{result}.log |
| Fresh Linux Native | Use EXISTING DB | E2E_LINUX_NATIVE_{date}_{result}.log |
| Upgrade | Existing installation | E2E_UPGRADE_{date}_{result}.log |

### 6.6 — Fast Lane vs Full Lane ✅ DONE

Document: `docs/CI_WORKFLOW_LANES.md`

| Lane | When to Use | Runtime | Commands |
|------|-------------|---------|----------|
| Fast Lane | Every change | ~30s | smoke script only |
| Full Lane | Before merge | ~5min | fmt + check + test + build + smoke |
| Release Lane | Before release | ~10min | Full + cross-platform + E2E |

### 6.7 — Linux CI Workflow

| Item | Status | Files | Done Condition |
|------|--------|-------|----------------|
| GitHub Actions workflow | ✅ DONE | .github/workflows/linux-smoke.yml | ubuntu-latest runs smoke, uploads P6_smoke_linux.log |
| WSL fallback docs | ✅ DONE | docs/CI_WORKFLOW_LANES.md | Instructions for manual WSL run |

---

## Phase 7 — Packaging + Distribution ⬜ NOT STARTED

| Item | Owner | Files | Command | Proof | Done Condition |
|------|-------|-------|---------|-------|----------------|
| Build script (Windows) | AI | tools/build-unified-installer.ps1 | Run script | Log | Produces CADALYTIX_INSTALLER/ |
| Build script (Linux) | AI | tools/build-unified-installer.sh | Run script | Log | Produces CADALYTIX_INSTALLER/ |
| VERSIONS.txt | AI | build output | N/A | File | Contains Rust/Tauri/frontend versions |
| MANIFEST.sha256 | AI | build output | N/A | File | SHA256 for all shipped files |
| External drive structure | AI | CADALYTIX_INSTALLER/ | N/A | Directory listing | Matches plan spec |
| No hardcoded paths | AI | src-tauri/ | Code review | N/A | All runtime paths relative |
| Linux exec permissions | AI | build script | ls -la | Terminal | INSTALL has +x |

---

## Phase 8 — Final Validation + Release ⬜ NOT STARTED

| Item | Owner | Files | Command | Proof | Done Condition |
|------|-------|-------|---------|-------|----------------|
| E2E runs logged | AI | N/A | Manual | Prod_Wizard_Log/E2E_*.log | All scenarios pass |
| Security: no secrets in logs | AI | utils/logging.rs | Code review + grep | N/A | Passwords/keys masked |
| Security: encryption-at-rest | AI | security/*.rs | Test | N/A | Secrets encrypted before DB store |
| Security: parameterized queries | AI | database/*.rs | Code review | N/A | No string interpolation in SQL |
| Performance: startup time | AI | N/A | Measure | Log | <5s to ready |
| Performance: progress smoothness | AI | N/A | Manual | N/A | No UI freeze, events every 1%/100ms |
| Docs: README.md | AI | CADALYTIX_INSTALLER/README.md | N/A | File | Exists |
| Docs: QUICK_START.md | AI | CADALYTIX_INSTALLER/QUICK_START.md | N/A | File | Exists |
| Docs: INSTALLATION_GUIDE.md | AI | CADALYTIX_INSTALLER/docs/INSTALLATION_GUIDE.md | N/A | File | Exists |
| Docs: TROUBLESHOOTING.md | AI | CADALYTIX_INSTALLER/docs/TROUBLESHOOTING.md | N/A | File | Exists |
| Docs: SYSTEM_REQUIREMENTS.md | AI | CADALYTIX_INSTALLER/docs/SYSTEM_REQUIREMENTS.md | N/A | File | Exists |
| MANIFEST.sha256 verifies | AI | N/A | shasum -c | Terminal | All files pass |
| Final package ready | AI | CADALYTIX_INSTALLER/ | N/A | N/A | Standalone, works from external drive |

---

## Definition of Done

### Phase 5 DoD ✅ COMPLETE (2026-01-07)

- [x] Phase 5 proof logs (B1/B2/B3/D2) pass and are current
- [x] cargo fmt/check/test pass; frontend build passes
- [x] All D1-D5 deliverables have proof transcripts

### Phase 6 DoD — DEV Lane ✅ COMPLETE (Windows-only acceptable for iteration)

| Criterion | Status | Proof |
|-----------|--------|-------|
| Windows smoke script passes | ✅ | P6_smoke_windows.log (ExitCode=0) |
| Unit tests pass (46 tests) | ✅ | P6_unit_tests.log (ExitCode=0) |
| Behavioral connection failure test passes | ✅ | P6_connection_failure_deterministic.log (ExitCode=0) |
| E2E scenario checklists documented | ✅ | docs/E2E_SCENARIO_CHECKLISTS.md |

### Phase 6 DoD — RELEASE Lane ⬜ PENDING (Linux proof required)

| Criterion | Status | Proof | Done Condition |
|-----------|--------|-------|----------------|
| All DEV Lane criteria | ✅ | See above | — |
| Linux smoke script passes | ⚠️ PENDING | P6_smoke_linux.log | ExitCode=0 from CI or WSL |
| CI artifacts captured | ⚠️ PENDING | GitHub Actions | linux-smoke.yml uploads P6_smoke_linux.log |

**RELEASE Lane is DONE when:** P6_smoke_linux.log exists with ExitCode=0 and all DEV Lane criteria pass.

### Final DoD (Phase 8 - Customer Ready)

- [ ] All Phase 5/6 items complete (both DEV and RELEASE lanes)
- [ ] Smoke scripts pass on Windows and Linux with proof logs
- [ ] E2E scenario evidence exists with timestamped logs
- [ ] Package structure matches plan and is standalone
- [ ] Docs complete, consistent, and included in package
- [ ] This checklist is fully checked off with proof artifacts
- [ ] New DB provisioning implemented (or explicitly documented as "Use EXISTING only")

