export type HotRetentionChoice = '12' | '18' | 'custom';

export interface RetentionStepProps {
  hotRetentionChoice: HotRetentionChoice;
  onHotRetentionChoiceChange: (choice: HotRetentionChoice) => void;
  hotRetentionCustomMonths: string;
  onHotRetentionCustomMonthsChange: (value: string) => void;
  retentionValidationError: string | null;
}

export function RetentionStep({
  hotRetentionChoice,
  onHotRetentionChoiceChange,
  hotRetentionCustomMonths,
  onHotRetentionCustomMonthsChange,
  retentionValidationError,
}: RetentionStepProps) {
  return (
    <div>
      <div className="wizard-row">Choose how long to keep hot data in the database.</div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={hotRetentionChoice === '12'} onChange={() => onHotRetentionChoiceChange('12')} />
          12 months (Recommended)
        </label>
      </div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={hotRetentionChoice === '18'} onChange={() => onHotRetentionChoiceChange('18')} />
          18 months (Recommended)
        </label>
      </div>
      <div className="wizard-row wizard-inline">
        <label className="wizard-inline">
          <input type="radio" checked={hotRetentionChoice === 'custom'} onChange={() => onHotRetentionChoiceChange('custom')} />
          Custom:
        </label>
        <input
          className="wizard-input"
          style={{ width: 120 }}
          value={hotRetentionCustomMonths}
          onChange={(e) => onHotRetentionCustomMonthsChange(e.target.value)}
          disabled={hotRetentionChoice !== 'custom'}
        />
        <span className="wizard-help">months</span>
      </div>
      {retentionValidationError ? <div className="wizard-error">{retentionValidationError}</div> : null}
    </div>
  );
}

