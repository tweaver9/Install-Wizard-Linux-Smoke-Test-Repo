export interface CompleteStepProps {
  installLogFolder: string | null;
  installManifestPath: string | null;
  installMappingPath: string | null;
  installConfigPath: string | null;
}

export function CompleteStep({
  installLogFolder,
  installManifestPath,
  installMappingPath,
  installConfigPath,
}: CompleteStepProps) {
  return (
    <div>
      <p>CADalytix Setup has completed.</p>
      {installLogFolder ? <div className="wizard-help">Log folder: {installLogFolder}</div> : null}
      {installManifestPath ? <div className="wizard-help">Install manifest: {installManifestPath}</div> : null}
      {installMappingPath ? <div className="wizard-help">Mapping: {installMappingPath}</div> : null}
      {installConfigPath ? <div className="wizard-help">Install config: {installConfigPath}</div> : null}
      <div className="wizard-row wizard-inline">
        <input id="launchAfter" type="checkbox" disabled />
        <label htmlFor="launchAfter" className="wizard-label" style={{ margin: 0 }}>
          Launch CADalytix
        </label>
      </div>
    </div>
  );
}

