import type React from 'react';

export interface LicenseStepProps {
  licenseText: string;
  licenseAccepted: boolean;
  onAcceptChange: (accepted: boolean) => void;
  licenseScrollRef: React.RefObject<HTMLDivElement | null>;
}

export function LicenseStep({
  licenseText,
  licenseAccepted,
  onAcceptChange,
  licenseScrollRef,
}: LicenseStepProps) {
  return (
    <div
      onKeyDown={(e) => {
        if (e.key === ' ') {
          e.preventDefault();
          onAcceptChange(!licenseAccepted);
        }
        if (e.key === 'PageDown' || e.key === 'PageUp') {
          const el = licenseScrollRef.current;
          if (!el) return;
          e.preventDefault();
          const delta = e.key === 'PageDown' ? el.clientHeight - 24 : -(el.clientHeight - 24);
          el.scrollTop = el.scrollTop + delta;
        }
      }}
    >
      <div className="license-box" ref={licenseScrollRef} aria-label="License text">
        {licenseText}
      </div>
      <div className="wizard-row wizard-inline" style={{ marginTop: 10 }}>
        <input
          id="licenseAccept"
          type="checkbox"
          checked={licenseAccepted}
          onChange={(e) => onAcceptChange(e.target.checked)}
        />
        <label htmlFor="licenseAccept" className="wizard-label" style={{ margin: 0 }}>
          I accept the terms of the license agreement
        </label>
      </div>
    </div>
  );
}

