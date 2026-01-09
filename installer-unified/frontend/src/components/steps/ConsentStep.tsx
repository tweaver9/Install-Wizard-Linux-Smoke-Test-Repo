export interface ConsentStepProps {
  consentToSync: boolean;
  onConsentToSyncChange: (value: boolean) => void;
  consentDetailsExpanded: boolean;
  onConsentDetailsExpandedToggle: () => void;
}

export function ConsentStep({
  consentToSync,
  onConsentToSyncChange,
  consentDetailsExpanded,
  onConsentDetailsExpandedToggle,
}: ConsentStepProps) {
  return (
    <div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="checkbox" checked={consentToSync} onChange={(e) => onConsentToSyncChange(e.target.checked)} />
          Allow CADalytix to receive install metadata + schema mapping to for support improvements
        </label>
      </div>
      <div className="wizard-row">
        <button className="wizard-button" type="button" onClick={onConsentDetailsExpandedToggle}>
          {consentDetailsExpanded ? 'Hide details…' : 'Exactly what is sent…'}
        </button>
      </div>
      {consentDetailsExpanded ? (
        <div className="wizard-help">
          <div>Install metadata (no passwords or connection strings):</div>
          <ul>
            <li>Installer version and timestamp</li>
            <li>Install mode (Windows / Docker)</li>
            <li>Selected storage/retention/archive settings</li>
            <li>Schema mapping (source field names + chosen target fields)</li>
            <li>Aggregate counts (mapped fields, detected fields)</li>
          </ul>
        </div>
      ) : null}
      <div className="wizard-help">This setting is stored locally. Network sync is not performed in this phase.</div>
    </div>
  );
}

