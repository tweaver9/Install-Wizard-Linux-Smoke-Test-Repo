/**
 * PlatformChooser - First screen of the installer (small chooser window)
 * 
 * Displays platform selection cards with OS-based recommendation.
 */
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

// Import assets
import logoImg from '../assets/CADalytix_No_Background_Large.png';
import windowsIcon from '../assets/Windows_Icon_No_Background.png';
import dockerLinuxIcon from '../assets/Docker-Linux_Icon.png';

import './PlatformChooser.css';

type RecommendedPlatform = 'windows' | 'docker' | 'unknown';

/**
 * Detect recommended platform based on user agent (advisory only).
 */
function detectRecommended(): RecommendedPlatform {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('windows')) return 'windows';
  if (ua.includes('linux')) return 'docker';
  if (ua.includes('mac os') || ua.includes('macintosh')) return 'docker';
  return 'unknown';
}

interface PlatformChooserProps {
  onPlatformSelect?: (platform: 'windows' | 'docker') => void;
}

export default function PlatformChooser({ onPlatformSelect }: PlatformChooserProps) {
  const recommended = detectRecommended();

  async function handlePlatformClick(platform: 'windows' | 'docker') {
    try {
      // If onPlatformSelect is provided (single-window mode), use it
      if (onPlatformSelect) {
        onPlatformSelect(platform);
        return;
      }

      // Multi-window mode: spawn installer window and close chooser
      await invoke('spawn_installer_window', { platform });
      const currentWindow = getCurrentWindow();
      await currentWindow.close();
    } catch (err) {
      console.error('Failed to spawn installer window:', err);
      // Fallback: just call onPlatformSelect if available
      if (onPlatformSelect) {
        onPlatformSelect(platform);
      }
    }
  }

  return (
    <div className="chooser-root">
      <div className="chooser-container">
        {/* Logo */}
        <div className="chooser-logo">
          <img src={logoImg} alt="CADalytix" className="chooser-logo-img" />
        </div>

        {/* Platform Cards */}
        <div className="chooser-cards">
          <button
            className={`chooser-card ${recommended === 'windows' ? 'recommended' : ''}`}
            onClick={() => handlePlatformClick('windows')}
          >
            {recommended === 'windows' && (
              <span className="chooser-recommended-badge">Recommended</span>
            )}
            <img src={windowsIcon} alt="Windows" className="chooser-card-icon" />
            <div className="chooser-card-title">Windows</div>
            <p className="chooser-card-desc">Native Windows installation</p>
          </button>

          <button
            className={`chooser-card ${recommended === 'docker' ? 'recommended' : ''}`}
            onClick={() => handlePlatformClick('docker')}
          >
            {recommended === 'docker' && (
              <span className="chooser-recommended-badge">Recommended</span>
            )}
            <img src={dockerLinuxIcon} alt="Docker/Linux" className="chooser-card-icon" />
            <div className="chooser-card-title">Docker / Linux</div>
            <p className="chooser-card-desc">Docker containers for Linux servers</p>
          </button>
        </div>

        {/* Version String */}
        <div className="chooser-version">
          CADalytix version 2.0.1 (Peony)
        </div>
      </div>
    </div>
  );
}

