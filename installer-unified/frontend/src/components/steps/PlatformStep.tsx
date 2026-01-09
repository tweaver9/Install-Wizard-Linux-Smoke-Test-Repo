export interface PlatformStepProps {
  onSelectWindows: () => void;
  onSelectDocker: () => void;
  onKeyDown: (e: React.KeyboardEvent) => void;
}

export function PlatformStep({ onSelectWindows, onSelectDocker, onKeyDown }: PlatformStepProps) {
  return (
    <div onKeyDown={onKeyDown} tabIndex={0}>
      <div className="platform-grid" role="group" aria-label="Platform selection">
        <button id="platform-windows" className="platform-card" onClick={onSelectWindows}>
          <div className="platform-card-title">Windows</div>
          <p className="platform-card-body">Install CADalytix directly on Windows.</p>
        </button>
        <button id="platform-docker" className="platform-card" onClick={onSelectDocker}>
          <div className="platform-card-title">Docker / Linux</div>
          <p className="platform-card-body">Install using Docker (Linux servers, Linux desktops, or Docker hosts).</p>
        </button>
      </div>
    </div>
  );
}

