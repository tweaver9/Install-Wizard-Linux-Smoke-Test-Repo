# Smoke Tests

Quick validation tests for the CADalytix Installer.

## TUI Smoke Test Modes

The TUI (Text User Interface) mode supports smoke tests that validate specific
wizard steps without requiring Docker or a full installation.

### Available Smoke Test Commands

```bash
# Test platform selection
./INSTALL --tui-smoke=platform

# Test database configuration step
./INSTALL --tui-smoke=database

# Test field mapping interface
./INSTALL --tui-smoke=mapping

# Test complete/success screen
./INSTALL --tui-smoke=complete
```

### What Each Test Validates

| Mode | Validates |
|------|-----------|
| `platform` | Platform chooser renders, Windows/Docker options visible |
| `database` | Create New / Existing toggle works, form fields render |
| `mapping` | Source/target field lists render, mapping interaction works |
| `complete` | Success screen shows, paths are displayed |

## No Docker Required

These smoke tests **do not require Docker** to be installed. They test the
installer's UI/UX flow without actually performing installations.

## GUI Quick Click-Through

For GUI mode validation:

1. **Launch** → Chooser screen appears
2. **Click Windows** → Chooser closes, installer window opens
3. **StepIndicator** → Shows "Step 1 of 14: Welcome"
4. **Navigate forward** → Each step advances correctly
5. **Database step** → Create New / Existing toggles work
6. **Complete step** → Shows installation paths

## Integration Test Scenarios

For full integration testing (requires actual environment):

| Scenario | Expected Result |
|----------|-----------------|
| Linux native install | `systemctl status cadalytix` shows active |
| Docker/Linux install | `docker compose ps` shows services Up |
| Bad DB connection | Installer fails gracefully with readable error |
| Disk space failure | Preflight blocks cleanly with error message |

## Running Tests Programmatically

```bash
# Run all Rust unit tests
cd src-tauri
cargo test

# Run specific test module
cargo test database::

# Run with output
cargo test -- --nocapture
```

## CI/CD Smoke Test

For automated pipelines:

```bash
#!/bin/bash
set -e

cd frontend && npm run build
cd ../src-tauri && cargo test --lib

echo "Smoke tests passed"
```

