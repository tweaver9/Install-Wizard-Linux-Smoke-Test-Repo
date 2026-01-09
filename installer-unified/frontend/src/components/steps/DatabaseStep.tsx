export type DbSetupMode = 'createNew' | 'existing' | null;
export type NewDbLocation = 'thisMachine' | 'specificPath';
export type DbHostedWhere = 'on_prem' | 'aws_rds' | 'azure_sql' | 'gcp_cloud_sql' | 'neon' | 'supabase' | 'other';
export type DbSslMode = 'disable' | 'prefer' | 'require';
export type DbEngine = 'sqlserver' | 'postgres' | null;
export type TestStatus = 'idle' | 'testing' | 'success' | 'fail';

export interface DatabaseStepProps {
  dbSetupMode: DbSetupMode;
  onDbSetupModeChange: (mode: DbSetupMode) => void;
  // Create new fields
  newDbName: string;
  onNewDbNameChange: (value: string) => void;
  newDbAdminHost: string;
  onNewDbAdminHostChange: (value: string) => void;
  newDbAdminPort: string;
  onNewDbAdminPortChange: (value: string) => void;
  newDbAdminUser: string;
  onNewDbAdminUserChange: (value: string) => void;
  newDbAdminPassword: string;
  onNewDbAdminPasswordChange: (value: string) => void;
  newDbPrivTestStatus: TestStatus;
  newDbPrivTestMessage: string;
  onRunCreateNewPrivilegeTest: () => void;
  newDbLocation: NewDbLocation;
  onNewDbLocationChange: (location: NewDbLocation) => void;
  newDbSpecificPath: string;
  onNewDbSpecificPathChange: (value: string) => void;
  onBrowseForNewDbPath: () => void;
  newDbMaxSizeGb: string;
  onNewDbMaxSizeGbChange: (value: string) => void;
  // SQL Server sizing
  dbEngine: DbEngine;
  newDbInitialDataSizeMb: string;
  onNewDbInitialDataSizeMbChange: (value: string) => void;
  newDbInitialLogSizeMb: string;
  onNewDbInitialLogSizeMbChange: (value: string) => void;
  newDbMaxDataSizeMb: string;
  onNewDbMaxDataSizeMbChange: (value: string) => void;
  newDbMaxLogSizeMb: string;
  onNewDbMaxLogSizeMbChange: (value: string) => void;
  newDbDataFilegrowth: string;
  onNewDbDataFilegrowthChange: (value: string) => void;
  newDbLogFilegrowth: string;
  onNewDbLogFilegrowthChange: (value: string) => void;
  // Postgres options
  newDbPgOwner: string;
  onNewDbPgOwnerChange: (value: string) => void;
  dbCreateValidationError: string | null;
  // Existing database fields
  existingHostedWhere: DbHostedWhere;
  onExistingHostedWhereChange: (where: DbHostedWhere) => void;
  dbUseConnString: boolean;
  onDbUseConnStringChange: (value: boolean) => void;
  dbConnString: string;
  onDbConnStringChange: (value: string) => void;
  dbHost: string;
  onDbHostChange: (value: string) => void;
  dbPort: string;
  onDbPortChange: (value: string) => void;
  dbName: string;
  onDbNameChange: (value: string) => void;
  dbUser: string;
  onDbUserChange: (value: string) => void;
  dbPassword: string;
  onDbPasswordChange: (value: string) => void;
  dbSslMode: DbSslMode;
  onDbSslModeChange: (mode: DbSslMode) => void;
  dbExistingMissingInputs: string[];
  canRunDbTest: boolean;
  dbTestStatus: TestStatus;
  dbTestMessage: string;
  onRunDbTest: () => void;
  // Mark port as touched
  onDbPortTouched: () => void;
}

export function DatabaseStep(props: DatabaseStepProps) {
  const { dbSetupMode, onDbSetupModeChange } = props;

  return (
    <div>
      <div className="wizard-row">
        Do you want CADalytix to create a NEW database, or use an EXISTING database?
      </div>

      <div className="platform-grid">
        <button
          type="button"
          className="platform-card"
          onClick={() => onDbSetupModeChange('createNew')}
        >
          <div className="platform-card-title">Create NEW CADalytix Database</div>
          <div className="wizard-help">Create a new database for this installation.</div>
        </button>
        <button
          type="button"
          className="platform-card"
          onClick={() => onDbSetupModeChange('existing')}
        >
          <div className="platform-card-title">Use EXISTING Database</div>
          <div className="wizard-help">Connect to an existing database you provide.</div>
        </button>
      </div>

      {dbSetupMode === 'createNew' ? (
        <DatabaseStepCreateNew {...props} />
      ) : null}

      {dbSetupMode === 'existing' ? (
        <DatabaseStepExisting {...props} />
      ) : null}
    </div>
  );
}

function DatabaseStepCreateNew(props: DatabaseStepProps) {
  const {
    newDbName, onNewDbNameChange,
    newDbAdminHost, onNewDbAdminHostChange,
    newDbAdminPort, onNewDbAdminPortChange,
    newDbAdminUser, onNewDbAdminUserChange,
    newDbAdminPassword, onNewDbAdminPasswordChange,
    newDbPrivTestStatus, newDbPrivTestMessage, onRunCreateNewPrivilegeTest,
    newDbLocation, onNewDbLocationChange,
    newDbSpecificPath, onNewDbSpecificPathChange, onBrowseForNewDbPath,
    newDbMaxSizeGb, onNewDbMaxSizeGbChange,
    dbEngine,
    newDbInitialDataSizeMb, onNewDbInitialDataSizeMbChange,
    newDbInitialLogSizeMb, onNewDbInitialLogSizeMbChange,
    newDbMaxDataSizeMb, onNewDbMaxDataSizeMbChange,
    newDbMaxLogSizeMb, onNewDbMaxLogSizeMbChange,
    newDbDataFilegrowth, onNewDbDataFilegrowthChange,
    newDbLogFilegrowth, onNewDbLogFilegrowthChange,
    newDbPgOwner, onNewDbPgOwnerChange,
    dbCreateValidationError,
  } = props;

  return (
    <div style={{ marginTop: 12 }}>
      <div className="wizard-row">
        <label className="wizard-label">New Database Name</label>
        <input
          className="wizard-input"
          style={{ width: 280 }}
          value={newDbName}
          onChange={(e) => onNewDbNameChange(e.target.value)}
          placeholder="CADalytix_Production"
        />
      </div>

      <div style={{ marginTop: 12, padding: 10, border: '1px solid #ccc', borderRadius: 4 }}>
        <div className="wizard-row"><strong>Database Server Admin Connection</strong></div>
        <div className="wizard-help">Provide credentials for an account with CREATE DATABASE privileges.</div>
        <div className="wizard-row">
          <label className="wizard-label">Host</label>
          <input className="wizard-input" style={{ width: 200 }} value={newDbAdminHost} onChange={(e) => onNewDbAdminHostChange(e.target.value)} placeholder="localhost" />
        </div>
        <div className="wizard-row">
          <label className="wizard-label">Port</label>
          <input className="wizard-input" style={{ width: 100 }} value={newDbAdminPort} onChange={(e) => onNewDbAdminPortChange(e.target.value)} placeholder="1433 or 5432" />
          <span className="wizard-help" style={{ marginLeft: 8 }}>1433 = SQL Server, 5432 = PostgreSQL</span>
        </div>
        <div className="wizard-row">
          <label className="wizard-label">Admin Username</label>
          <input className="wizard-input" style={{ width: 200 }} value={newDbAdminUser} onChange={(e) => onNewDbAdminUserChange(e.target.value)} placeholder="sa" />
        </div>
        <div className="wizard-row">
          <label className="wizard-label">Admin Password</label>
          <input className="wizard-input" style={{ width: 200 }} type="password" value={newDbAdminPassword} onChange={(e) => onNewDbAdminPasswordChange(e.target.value)} />
        </div>
        <div className="wizard-row">
          <button
            className="wizard-button"
            type="button"
            disabled={newDbPrivTestStatus === 'testing' || !newDbAdminHost.trim() || !newDbAdminUser.trim() || !newDbAdminPassword.trim()}
            onClick={onRunCreateNewPrivilegeTest}
          >
            {newDbPrivTestStatus === 'testing' ? 'Testing...' : 'Test Connection & Privileges'}
          </button>
          {newDbPrivTestStatus === 'success' ? <span className="wizard-success" style={{ marginLeft: 10 }}>✓ {newDbPrivTestMessage}</span> : null}
          {newDbPrivTestStatus === 'fail' ? <span className="wizard-error" style={{ marginLeft: 10 }}>✗ {newDbPrivTestMessage}</span> : null}
        </div>
      </div>

      <div className="wizard-row" style={{ marginTop: 12 }}>
        <strong>Where should the new CADalytix database be created?</strong>
      </div>

      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={newDbLocation === 'thisMachine'} onChange={() => onNewDbLocationChange('thisMachine')} />
          This machine (default location)
        </label>
      </div>

      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={newDbLocation === 'specificPath'} onChange={() => onNewDbLocationChange('specificPath')} />
          Specific drive / path (advanced)
        </label>
      </div>

      {newDbLocation === 'specificPath' ? (
        <div className="wizard-row">
          <label className="wizard-label">Database path</label>
          <div className="wizard-inline">
            <input className="wizard-input" value={newDbSpecificPath} onChange={(e) => onNewDbSpecificPathChange(e.target.value)} />
            <button className="wizard-button" type="button" onClick={onBrowseForNewDbPath}>Browse…</button>
          </div>
        </div>
      ) : null}

      <div className="wizard-row">
        <label className="wizard-label">Max DB size / storage allocation (GB)</label>
        <input className="wizard-input" style={{ width: 180 }} value={newDbMaxSizeGb} onChange={(e) => onNewDbMaxSizeGbChange(e.target.value)} />
      </div>

      {dbEngine === 'sqlserver' ? (
        <div style={{ marginTop: 12, padding: 10, border: '1px solid #ccc', borderRadius: 4 }}>
          <div className="wizard-row"><strong>SQL Server Sizing (optional)</strong></div>
          <div className="wizard-row">
            <label className="wizard-label">Initial data file size (MB)</label>
            <input className="wizard-input" style={{ width: 120 }} value={newDbInitialDataSizeMb} onChange={(e) => onNewDbInitialDataSizeMbChange(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Initial log file size (MB)</label>
            <input className="wizard-input" style={{ width: 120 }} value={newDbInitialLogSizeMb} onChange={(e) => onNewDbInitialLogSizeMbChange(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Max data file size (MB, 0 = unlimited)</label>
            <input className="wizard-input" style={{ width: 120 }} value={newDbMaxDataSizeMb} onChange={(e) => onNewDbMaxDataSizeMbChange(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Max log file size (MB, 0 = unlimited)</label>
            <input className="wizard-input" style={{ width: 120 }} value={newDbMaxLogSizeMb} onChange={(e) => onNewDbMaxLogSizeMbChange(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Data filegrowth (MB, or negative for %)</label>
            <input className="wizard-input" style={{ width: 120 }} value={newDbDataFilegrowth} onChange={(e) => onNewDbDataFilegrowthChange(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Log filegrowth (MB, or negative for %)</label>
            <input className="wizard-input" style={{ width: 120 }} value={newDbLogFilegrowth} onChange={(e) => onNewDbLogFilegrowthChange(e.target.value)} />
          </div>
          <div className="wizard-help">Leave at defaults for typical installations. Negative values indicate percentage growth (e.g., -10 = 10%).</div>
        </div>
      ) : null}

      {dbEngine === 'postgres' ? (
        <div style={{ marginTop: 12, padding: 10, border: '1px solid #ccc', borderRadius: 4 }}>
          <div className="wizard-row"><strong>PostgreSQL Options (optional)</strong></div>
          <div className="wizard-row">
            <label className="wizard-label">Owner role (leave blank for current user)</label>
            <input className="wizard-input" style={{ width: 200 }} value={newDbPgOwner} onChange={(e) => onNewDbPgOwnerChange(e.target.value)} placeholder="e.g., cadalytix_admin" />
          </div>
          <div className="wizard-help">PostgreSQL does not support SQL Server-style sizing. The database grows with available disk space.</div>
        </div>
      ) : null}

      {dbCreateValidationError ? <div className="wizard-error">{dbCreateValidationError}</div> : null}

      <div className="wizard-help">Hot retention and archive policy are configured on the next pages.</div>
    </div>
  );
}


function DatabaseStepExisting(props: DatabaseStepProps) {
  const {
    existingHostedWhere, onExistingHostedWhereChange,
    dbUseConnString, onDbUseConnStringChange,
    dbConnString, onDbConnStringChange,
    dbHost, onDbHostChange,
    dbPort, onDbPortChange, onDbPortTouched,
    dbName, onDbNameChange,
    dbUser, onDbUserChange,
    dbPassword, onDbPasswordChange,
    dbSslMode, onDbSslModeChange,
    dbExistingMissingInputs,
    canRunDbTest, dbTestStatus, dbTestMessage, onRunDbTest,
  } = props;

  return (
    <div style={{ marginTop: 12 }}>
      <div className="wizard-row">
        <strong>Where is the existing database hosted? (No login required)</strong>
      </div>

      <div className="wizard-row">
        <select
          className="wizard-select"
          value={existingHostedWhere}
          onChange={(e) => onExistingHostedWhereChange(e.target.value as DbHostedWhere)}
        >
          <option value="on_prem">On-prem / self-hosted / unknown</option>
          <option value="aws_rds">AWS RDS / Aurora</option>
          <option value="azure_sql">Azure SQL / SQL MI</option>
          <option value="gcp_cloud_sql">GCP Cloud SQL</option>
          <option value="neon">Neon</option>
          <option value="supabase">Supabase</option>
          <option value="other">Other</option>
        </select>
      </div>

      <div className="wizard-row">
        <strong>How do you want to connect?</strong>
      </div>

      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={dbUseConnString} onChange={() => onDbUseConnStringChange(true)} />
          Connection string
        </label>
      </div>

      <div className="wizard-row">
        <label className="wizard-inline">
          <input type="radio" checked={!dbUseConnString} onChange={() => onDbUseConnStringChange(false)} />
          Enter connection details (host/server, port, db name, username, password, TLS)
        </label>
      </div>

      <div className="wizard-help" style={{ marginTop: 6 }}>
        CADalytix does not ask you to log in to AWS/Azure/GCP and does not scan your cloud. You only provide a database endpoint (connection string or host/port/user/password) with explicit permissions.
      </div>

      {dbUseConnString ? (
        <div className="wizard-row" style={{ marginTop: 10 }}>
          <label className="wizard-label">Connection string</label>
          <input className="wizard-input" value={dbConnString} onChange={(e) => onDbConnStringChange(e.target.value)} />
        </div>
      ) : (
        <div style={{ marginTop: 10 }}>
          <div className="wizard-row">
            <label className="wizard-label">Host</label>
            <input className="wizard-input" value={dbHost} onChange={(e) => onDbHostChange(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Port</label>
            <input
              className="wizard-input"
              value={dbPort}
              onChange={(e) => {
                onDbPortTouched();
                onDbPortChange(e.target.value);
              }}
            />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Database</label>
            <input className="wizard-input" value={dbName} onChange={(e) => onDbNameChange(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Username</label>
            <input className="wizard-input" value={dbUser} onChange={(e) => onDbUserChange(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Password</label>
            <input className="wizard-input" type="password" value={dbPassword} onChange={(e) => onDbPasswordChange(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">TLS</label>
            <select className="wizard-select" value={dbSslMode} onChange={(e) => onDbSslModeChange(e.target.value as DbSslMode)}>
              <option value="disable">Disable</option>
              <option value="prefer">Prefer</option>
              <option value="require">Require</option>
            </select>
          </div>
        </div>
      )}

      {dbExistingMissingInputs.length > 0 ? (
        <div className="wizard-error" style={{ marginTop: 10 }}>
          Missing required inputs: {dbExistingMissingInputs.join(', ')}
        </div>
      ) : null}

      <div className="wizard-row">
        <button className="wizard-button" disabled={dbTestStatus === 'testing' || !canRunDbTest} onClick={onRunDbTest}>
          Test Connection
        </button>
      </div>

      {dbTestStatus === 'success' ? <div className="wizard-help">{dbTestMessage || 'Connection successful.'}</div> : null}
      {dbTestStatus === 'fail' ? <div className="wizard-error">{dbTestMessage || 'Connection failed.'}</div> : null}
    </div>
  );
}

