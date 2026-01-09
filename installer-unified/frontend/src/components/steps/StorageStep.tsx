export type StorageMode = 'defaults' | 'custom';
export type StorageLocation = 'system' | 'attached' | 'custom';
export type RetentionPolicy = '18' | '12' | 'max' | 'keep';

export interface StorageStepProps {
  storageMode: StorageMode;
  onStorageModeChange: (mode: StorageMode) => void;
  storageLocation: StorageLocation;
  onStorageLocationChange: (location: StorageLocation) => void;
  storageCustomPath: string;
  onStorageCustomPathChange: (path: string) => void;
  retentionPolicy: RetentionPolicy;
  onRetentionPolicyChange: (policy: RetentionPolicy) => void;
  maxDiskGb: string;
  onMaxDiskGbChange: (value: string) => void;
}

export function StorageStep({
  storageMode,
  onStorageModeChange,
  storageLocation,
  onStorageLocationChange,
  storageCustomPath,
  onStorageCustomPathChange,
  retentionPolicy,
  onRetentionPolicyChange,
  maxDiskGb,
  onMaxDiskGbChange,
}: StorageStepProps) {
  return (
    <div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={storageMode === 'defaults'} onChange={() => onStorageModeChange('defaults')} />
          Use defaults (Recommended)
        </label>
      </div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={storageMode === 'custom'} onChange={() => onStorageModeChange('custom')} />
          Customize storage
        </label>
      </div>

      {storageMode === 'custom' ? (
        <div style={{ marginTop: 10 }}>
          <div className="wizard-row">
            <label className="wizard-label">Storage location</label>
            <div className="wizard-row">
              <label className="wizard-inline">
                <input type="radio" checked={storageLocation === 'system'} onChange={() => onStorageLocationChange('system')} />
                Use system disk
              </label>
            </div>
            <div className="wizard-row">
              <label className="wizard-inline">
                <input type="radio" checked={storageLocation === 'attached'} onChange={() => onStorageLocationChange('attached')} />
                Use attached drive
              </label>
            </div>
            <div className="wizard-row">
              <label className="wizard-inline">
                <input type="radio" checked={storageLocation === 'custom'} onChange={() => onStorageLocationChange('custom')} />
                Use custom path
              </label>
            </div>
          </div>

          {storageLocation === 'custom' ? (
            <div className="wizard-row">
              <label className="wizard-label">Custom path</label>
              <input className="wizard-input" value={storageCustomPath} onChange={(e) => onStorageCustomPathChange(e.target.value)} />
            </div>
          ) : null}

          <div className="wizard-row">
            <label className="wizard-label">Storage policy</label>
            <div className="wizard-row">
              <label className="wizard-inline">
                <input type="radio" checked={retentionPolicy === '18'} onChange={() => onRetentionPolicyChange('18')} />
                Rolling 18 months (Recommended)
              </label>
            </div>
            <div className="wizard-row">
              <label className="wizard-inline">
                <input type="radio" checked={retentionPolicy === '12'} onChange={() => onRetentionPolicyChange('12')} />
                Rolling 12 months
              </label>
            </div>
            <div className="wizard-row wizard-inline">
              <label className="wizard-inline">
                <input type="radio" checked={retentionPolicy === 'max'} onChange={() => onRetentionPolicyChange('max')} />
                Max disk usage:
              </label>
              <input className="wizard-input" style={{ width: 120 }} value={maxDiskGb} onChange={(e) => onMaxDiskGbChange(e.target.value)} />
              <span className="wizard-help">GB</span>
            </div>
            <div className="wizard-row">
              <label className="wizard-inline">
                <input type="radio" checked={retentionPolicy === 'keep'} onChange={() => onRetentionPolicyChange('keep')} />
                Keep everything (Not recommended)
              </label>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

