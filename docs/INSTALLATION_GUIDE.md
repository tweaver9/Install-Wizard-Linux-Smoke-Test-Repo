# CADalytix Installation Guide

This guide covers installation of the CADalytix platform using the Unified Cross-Platform Installer.

## Prerequisites

### Windows
- **Windows 10/11** or **Windows Server 2016+**
- **PowerShell 5.1+** (built-in)
- **SQL Server 2016+** or **PostgreSQL 13+** for database
- **Administrator privileges** for service installation

### Linux
- **Ubuntu 22.04 LTS** (recommended) or RHEL 8+
- **PostgreSQL 13+** for database
- **systemd** for service management
- **Root or sudo access** for service installation

## Quick Start

### 1. Extract the Bundle

```powershell
# Windows
Expand-Archive CADALYTIX_INSTALLER.zip -DestinationPath C:\CADalytix
```

```bash
# Linux
unzip CADALYTIX_INSTALLER.zip -d /opt/cadalytix
```

### 2. Verify Bundle Integrity

```powershell
# Windows - verify manifest
cd CADALYTIX_INSTALLER\VERIFY
.\verify-manifest.ps1
```

```bash
# Linux - verify manifest
cd CADALYTIX_INSTALLER/VERIFY
./verify-manifest.sh
```

### 3. Run the Installer

**Windows (GUI):**
```powershell
.\installer-unified.exe
```

**Windows (Terminal/Headless):**
```powershell
.\installer-unified.exe --headless
```

**Linux (Terminal):**
```bash
./installer-unified --headless
```

## Installation Steps

### Step 1: Welcome & License
Review and accept the license agreement.

### Step 2: Destination Folder
Choose where CADalytix components will be installed.
- Windows default: `C:\Program Files\CADalytix`
- Linux default: `/opt/cadalytix`

### Step 3: Database Setup
Configure database connections:
- **Configuration Database**: Stores CADalytix settings and state
- **Call Data Database**: Source for call records

Supported database types:
- PostgreSQL 13+ (recommended for Linux)
- SQL Server 2016+ (Windows only)

### Step 4: Storage Configuration
Choose storage type for archived data:
- **Local**: Store on local filesystem
- **Azure Blob**: Store in Azure Blob Storage
- **AWS S3**: Store in Amazon S3

### Step 5: Retention Policy
Configure how long call data is retained:
- **Hot Days**: Data kept in primary database
- **Archive Days**: Data kept in archive storage
- **Total Retention**: Maximum data age

### Step 6: Archive Schedule
Set when archive jobs run:
- Day of month (1-28)
- Time of day (local timezone)
- Catch-up on startup option

### Step 7: Consent
Choose whether to enable:
- Support sync (anonymous telemetry)
- Field mapping override

### Step 8: Field Mapping
Map source database fields to CADalytix schema.
Default mappings work for most installations.

### Step 9: Ready
Review configuration and begin installation.

### Step 10: Progress
Monitor installation progress. Services are installed and started automatically.

## Post-Installation

### Verify Installation
```powershell
# Windows - check service status
Get-Service CADalytix*

# Linux - check service status
systemctl status cadalytix
```

### View Logs
```powershell
# Windows
Get-Content "$env:ProgramData\CADalytix\logs\*.log" -Tail 100

# Linux
tail -100 /var/log/cadalytix/*.log
```

## Troubleshooting

### Database Connection Issues
1. Verify database server is running
2. Check firewall allows connection
3. Verify credentials have appropriate permissions

### Service Won't Start
1. Check logs in `Prod_Wizard_Log/` folder
2. Verify configuration in `appsettings.json`
3. Ensure database is accessible

### Manifest Verification Fails
1. Re-download the bundle
2. Verify no files were modified
3. Contact support if issue persists

## Support

For installation assistance:
- Email: support@cadalytix.com
- Documentation: https://docs.cadalytix.com

