export type ArchiveFormat = 'zip+ndjson' | 'zip+csv';

export interface ArchiveStepProps {
  archiveFormat: ArchiveFormat;
  onArchiveFormatChange: (format: ArchiveFormat) => void;
  archiveDestinationPath: string;
  onArchiveDestinationPathChange: (path: string) => void;
  archiveMaxUsageGb: string;
  onArchiveMaxUsageGbChange: (value: string) => void;
  archiveScheduleDayOfMonth: string;
  onArchiveScheduleDayOfMonthChange: (value: string) => void;
  archiveScheduleTimeLocal: string;
  onArchiveScheduleTimeLocalChange: (value: string) => void;
  archiveCatchUpOnStartup: boolean;
  onArchiveCatchUpOnStartupChange: (value: boolean) => void;
  archiveValidationError: string | null;
  onBrowseForArchiveFolder: () => void;
}

export function ArchiveStep({
  archiveFormat,
  onArchiveFormatChange,
  archiveDestinationPath,
  onArchiveDestinationPathChange,
  archiveMaxUsageGb,
  onArchiveMaxUsageGbChange,
  archiveScheduleDayOfMonth,
  onArchiveScheduleDayOfMonthChange,
  archiveScheduleTimeLocal,
  onArchiveScheduleTimeLocalChange,
  archiveCatchUpOnStartup,
  onArchiveCatchUpOnStartupChange,
  archiveValidationError,
  onBrowseForArchiveFolder,
}: ArchiveStepProps) {
  return (
    <div>
      <div className="wizard-row">Configure cold storage (archive) settings.</div>

      <div className="wizard-row">
        <label className="wizard-label">Archive format</label>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={archiveFormat === 'zip+ndjson'} onChange={() => onArchiveFormatChange('zip+ndjson')} />
            ZIP + NDJSON (Preferred)
          </label>
        </div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={archiveFormat === 'zip+csv'} onChange={() => onArchiveFormatChange('zip+csv')} />
            ZIP + CSV
          </label>
        </div>
      </div>

      <div className="wizard-row">
        <label className="wizard-label">Archive destination folder</label>
        <div className="wizard-row wizard-inline">
          <input className="wizard-input" value={archiveDestinationPath} onChange={(e) => onArchiveDestinationPathChange(e.target.value)} />
          <button className="wizard-button" type="button" onClick={onBrowseForArchiveFolder}>
            Browse…
          </button>
        </div>
      </div>

      <div className="wizard-row">
        <label className="wizard-label">Max archive usage cap (GB)</label>
        <input className="wizard-input" style={{ width: 160 }} value={archiveMaxUsageGb} onChange={(e) => onArchiveMaxUsageGbChange(e.target.value)} />
      </div>

      <div className="wizard-row">
        <label className="wizard-label">Schedule (local server time)</label>
        <div className="wizard-row wizard-inline">
          <span className="wizard-help">Day</span>
          <input
            className="wizard-input"
            style={{ width: 90 }}
            value={archiveScheduleDayOfMonth}
            onChange={(e) => onArchiveScheduleDayOfMonthChange(e.target.value)}
          />
          <span className="wizard-help">at</span>
          <input
            className="wizard-input"
            style={{ width: 120 }}
            value={archiveScheduleTimeLocal}
            onChange={(e) => onArchiveScheduleTimeLocalChange(e.target.value)}
          />
          <span className="wizard-help">(HH:MM)</span>
        </div>
      </div>

      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="checkbox" checked={archiveCatchUpOnStartup} onChange={(e) => onArchiveCatchUpOnStartupChange(e.target.checked)} />
          If missed, run on next startup for eligible months
        </label>
      </div>

      {archiveValidationError ? <div className="wizard-error">{archiveValidationError}</div> : null}
    </div>
  );
}

