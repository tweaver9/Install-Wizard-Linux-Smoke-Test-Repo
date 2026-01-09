export type DataSourceKind = 'local' | 'remote';

export interface DataSourceStepProps {
  dataSourceKind: DataSourceKind;
  onDataSourceKindChange: (kind: DataSourceKind) => void;
  callDataHost: string;
  onCallDataHostChange: (value: string) => void;
  callDataPort: string;
  onCallDataPortChange: (value: string) => void;
  callDataDbName: string;
  onCallDataDbNameChange: (value: string) => void;
  callDataUser: string;
  onCallDataUserChange: (value: string) => void;
  callDataPassword: string;
  onCallDataPasswordChange: (value: string) => void;
  sourceObjectName: string;
  onSourceObjectNameChange: (value: string) => void;
}

export function DataSourceStep({
  dataSourceKind,
  onDataSourceKindChange,
  callDataHost,
  onCallDataHostChange,
  callDataPort,
  onCallDataPortChange,
  callDataDbName,
  onCallDataDbNameChange,
  callDataUser,
  onCallDataUserChange,
  callDataPassword,
  onCallDataPasswordChange,
  sourceObjectName,
  onSourceObjectNameChange,
}: DataSourceStepProps) {
  return (
    <div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={dataSourceKind === 'local'} onChange={() => onDataSourceKindChange('local')} />
          Use this server/host (local environment)
        </label>
      </div>
      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={dataSourceKind === 'remote'} onChange={() => onDataSourceKindChange('remote')} />
          Connect to an existing remote system/database
        </label>
      </div>
      <div style={{ marginTop: 10 }}>
        <div className="wizard-row">
          <label className="wizard-label">Host</label>
          <input
            className="wizard-input"
            value={callDataHost}
            onChange={(e) => onCallDataHostChange(e.target.value)}
            disabled={dataSourceKind === 'local'}
          />
        </div>
        <div className="wizard-row wizard-inline">
          <div style={{ flex: 1 }}>
            <label className="wizard-label">Port</label>
            <input
              className="wizard-input"
              value={callDataPort}
              onChange={(e) => onCallDataPortChange(e.target.value)}
              disabled={dataSourceKind === 'local'}
            />
          </div>
          <div style={{ flex: 2 }}>
            <label className="wizard-label">Database</label>
            <input className="wizard-input" value={callDataDbName} onChange={(e) => onCallDataDbNameChange(e.target.value)} />
          </div>
        </div>
        <div className="wizard-row">
          <label className="wizard-label">Username</label>
          <input className="wizard-input" value={callDataUser} onChange={(e) => onCallDataUserChange(e.target.value)} />
        </div>
        <div className="wizard-row">
          <label className="wizard-label">Password</label>
          <input
            className="wizard-input"
            type="password"
            value={callDataPassword}
            onChange={(e) => onCallDataPasswordChange(e.target.value)}
          />
        </div>
      </div>
      <div className="wizard-row">
        <label className="wizard-label">Source object name</label>
        <input className="wizard-input" value={sourceObjectName} onChange={(e) => onSourceObjectNameChange(e.target.value)} />
      </div>
      <div className="wizard-help">Keep simple; do not require user to understand internal architecture.</div>
    </div>
  );
}

