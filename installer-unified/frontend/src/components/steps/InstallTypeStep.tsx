export type InstallationType = 'typical' | 'custom' | 'import';

export interface InstallTypeStepProps {
  installationType: InstallationType;
  onTypeChange: (type: InstallationType) => void;
  importConfigPath: string;
  onImportConfigPathChange: (path: string) => void;
  importConfigError: string | null;
  onImportConfigErrorClear: () => void;
  onBrowseForFile: () => void;
}

export function InstallTypeStep({
  installationType,
  onTypeChange,
  importConfigPath,
  onImportConfigPathChange,
  importConfigError,
  onImportConfigErrorClear,
  onBrowseForFile,
}: InstallTypeStepProps) {
  return (
    <div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input
            type="radio"
            checked={installationType === 'typical'}
            onChange={() => onTypeChange('typical')}
          />
          Typical (Recommended)
        </label>
      </div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={installationType === 'custom'} onChange={() => onTypeChange('custom')} />
          Custom
        </label>
      </div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={installationType === 'import'} onChange={() => onTypeChange('import')} />
          Import configuration file…
        </label>
      </div>

      {installationType === 'import' ? (
        <div style={{ marginTop: 12 }}>
          <div className="wizard-row">
            <label className="wizard-label">Configuration file</label>
            <div className="wizard-inline">
              <input
                className="wizard-input"
                value={importConfigPath}
                onChange={(e) => {
                  onImportConfigPathChange(e.target.value);
                  onImportConfigErrorClear();
                }}
              />
              <button className="wizard-button" onClick={onBrowseForFile}>
                Browse…
              </button>
            </div>
            {importConfigError ? <div className="wizard-error">{importConfigError}</div> : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}

