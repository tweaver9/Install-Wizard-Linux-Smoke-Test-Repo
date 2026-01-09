# UI Image Assets

This directory contains image assets for the CADalytix Installer UI.

## Asset Inventory

| File | Intended Use |
|------|--------------|
| `CADalytix_No_Background_Large.png` | Splash screen, Welcome header |
| `CADalytix_No_Background_Small.png` | Wizard header logo |
| `CADalytix_White_Background_Large.jpg` | Documentation/alt contexts |
| `CADalytix_White_Background_Small.jpg` | Documentation/alt contexts |
| `Windows_Icon_No_Background.png` | Platform selection card (Windows option) |
| `Docker-Linux_Icon.png` | Platform selection card (Docker/Linux option) |
| `SQL_Icon.png` | Database step (SQL Server option) |
| `PostgreSQL_Logo_No_Background.png` | Database step (PostgreSQL option) |
| `Rust_Icon_No_Background.png` | About/Credits (optional) |

## Usage

**Preferred usage is TSX import (bundler-safe) rather than CSS `background-image`.**

Example:
```tsx
import logoLarge from '../assets/CADalytix_No_Background_Large.png';

function SplashScreen() {
  return <img src={logoLarge} alt="CADalytix" className="splash-logo" />;
}
```

This ensures the bundler (Vite) handles the asset correctly with proper hashing and path resolution.

