# Runtime Payloads

This directory contains installer-bundled runtime payloads that are copied/deployed during installation.

## Directory Structure

```
runtime/
├── linux/      # Linux runtime payloads
├── windows/    # Windows runtime payloads
└── shared/     # Cross-platform shared assets
```

## Folder Contents

### `linux/`
Linux-specific runtime payloads:
- Native Linux binaries and scripts
- Docker templates and compose files (future)
- Docker images as `.tar` files (future)

### `windows/`
Windows-specific runtime payloads (added in later phases):
- Windows service/runtime assets
- Windows-specific scripts and configuration templates
- Optional helper utilities needed during install

### `shared/`
Cross-platform shared assets (added in later phases):
- Configuration templates (appsettings.json, etc.)
- Shared schema and mapping templates
- Common assets used by both platforms

## Note

The `.gitkeep` files are intentionally placed in empty directories to ensure Git tracks them. These placeholders will be removed once real payloads are added in later phases.

