import type { ProgressEvent } from '../../lib/api';

export interface InstallingStepProps {
  progress: ProgressEvent | null;
  installDetailLines: string[];
  installError: string | null;
}

export function InstallingStep({ progress, installDetailLines, installError }: InstallingStepProps) {
  const elapsedMs = progress?.elapsedMs;
  const etaMs = progress?.etaMs;
  const fmt = (ms?: number) => {
    if (!ms || ms <= 0) return '';
    const total = Math.floor(ms / 1000);
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${m}:${String(s).padStart(2, '0')}`;
  };

  return (
    <div>
      <div className="wizard-row">Progress</div>
      <progress className="progress-bar" value={progress?.percent ?? 0} max={100} />
      <div className="wizard-row">Current action: {progress?.message ?? ''}</div>
      <div className="wizard-row wizard-inline">
        {elapsedMs ? <div className="wizard-help">Elapsed: {fmt(elapsedMs)}</div> : null}
        {etaMs ? <div className="wizard-help">Estimated remaining: {fmt(etaMs)}</div> : null}
      </div>
      {installDetailLines.length > 0 ? (
        <div className="install-detail-log" aria-label="Installation details">
          {installDetailLines.map((l, idx) => (
            <div key={`${idx}-${l}`}>{l}</div>
          ))}
        </div>
      ) : null}
      {installError ? <div className="wizard-error">{installError}</div> : null}
    </div>
  );
}

