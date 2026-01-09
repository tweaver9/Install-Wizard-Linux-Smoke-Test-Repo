export interface DestinationStepProps {
  destinationFolder: string;
  onDestinationChange: (path: string) => void;
  destinationError: string | null;
  onBrowseForFolder: () => void;
}

export function DestinationStep({
  destinationFolder,
  onDestinationChange,
  destinationError,
  onBrowseForFolder,
}: DestinationStepProps) {
  return (
    <div>
      <div className="wizard-row">
        <label className="wizard-label">Install path</label>
        <div className="wizard-inline">
          <input
            className="wizard-input"
            value={destinationFolder}
            onChange={(e) => onDestinationChange(e.target.value)}
          />
          <button className="wizard-button" onClick={onBrowseForFolder}>
            Browse…
          </button>
        </div>
        <div className="wizard-help">Required space: ~2–5 GB</div>
        {destinationError ? <div className="wizard-error">{destinationError}</div> : null}
      </div>
    </div>
  );
}

