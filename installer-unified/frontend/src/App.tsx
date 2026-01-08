import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { listenToEvent, preflightDataSource, type DiscoveredColumnDto, type ProgressEvent } from './lib/api';
import './App.css';

type InstallMode = 'windows' | 'docker';
type WizardPage =
  | 'platform'
  | 'welcome'
  | 'license'
  | 'installType'
  | 'destination'
  | 'dataSource'
  | 'database'
  | 'storage'
  | 'retention'
  | 'archive'
  | 'consent'
  | 'mapping'
  | 'ready'
  | 'installing'
  | 'complete';

type InstallationType = 'typical' | 'custom' | 'import';

interface InstallerReadyEvent {
  timestamp: string;
  version: string;
}

interface InstallResultEvent {
  correlationId: string;
  ok: boolean;
  message: string;
  details?: {
    logFolder?: string;
    artifactsDir?: string;
    manifestPath?: string;
    mappingPath?: string;
    configPath?: string;
  } | null;
}

interface TestDbConnectionResponse {
  success: boolean;
  message: string;
}

interface TargetField {
  id: string;
  name: string;
  required: boolean;
}

interface SourceField {
  id: string;
  rawName: string;
  displayName: string;
}

interface ModalState {
  kind: 'none' | 'confirmCancel' | 'error' | 'replaceMapping' | 'sourceAlreadyMapped';
  title?: string;
  body?: string;
  primaryLabel?: string;
  secondaryLabel?: string;
  tertiaryLabel?: string;
  onPrimary?: (() => void) | null;
  onSecondary?: (() => void) | null;
  onTertiary?: (() => void) | null;
}

function Modal({ state }: { state: ModalState }) {
  if (state.kind === 'none') return null;

  const primary = state.primaryLabel ?? 'OK';
  const secondary = state.secondaryLabel ?? 'Cancel';

  return (
    <div className="modal-overlay" role="dialog" aria-modal="true">
      <div className="modal">
        <div className="modal-header">{state.title ?? ''}</div>
        <div className="modal-body">{state.body ?? ''}</div>
        <div className="modal-footer">
          {state.onTertiary && state.tertiaryLabel ? (
            <button className="wizard-button" onClick={state.onTertiary}>
              {state.tertiaryLabel}
            </button>
          ) : null}
          {state.onSecondary ? (
            <button className="wizard-button" onClick={state.onSecondary}>
              {secondary}
            </button>
          ) : null}
          {state.onPrimary ? (
            <button className="wizard-button primary" onClick={state.onPrimary}>
              {primary}
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function WizardFrame(props: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
  backDisabled: boolean;
  nextDisabled: boolean;
  nextLabel: string;
  cancelDisabled?: boolean;
  onBack: () => void;
  onNext: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="wizard-root">
      <div className="wizard-window">
        <div className="wizard-header">
          <h2 className="wizard-title">{props.title}</h2>
          {props.subtitle ? <p className="wizard-subtitle">{props.subtitle}</p> : null}
        </div>
        <div className="wizard-content">{props.children}</div>
        <div className="wizard-footer">
          <button className="wizard-button" disabled={props.backDisabled} onClick={props.onBack}>
            Back
          </button>
          <button className="wizard-button primary" disabled={props.nextDisabled} onClick={props.onNext}>
            {props.nextLabel}
          </button>
          <button className="wizard-button" disabled={props.cancelDisabled} onClick={props.onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

function normalizeWindowsPath(path: string): string {
  return path.trim().split('/').join('\\');
}

function defaultInstallPath(mode: InstallMode): string {
  if (mode === 'windows') return 'C:\\\\Program Files\\\\CADalytix';
  return '/opt/cadalytix';
}

function disambiguateSourceColumns(cols: DiscoveredColumnDto[]): SourceField[] {
  const counts = new Map<string, number>();
  const seen = new Map<string, number>();

  for (const c of cols) counts.set(c.name, (counts.get(c.name) ?? 0) + 1);

  const sanitizeBase = (raw: string) =>
    raw
      .split('')
      .map((ch) => (/[A-Za-z0-9_]/.test(ch) ? ch : '_'))
      .join('')
      .replace(/_+/g, '_')
      .replace(/^_+|_+$/g, '');

  return cols.map((c) => {
    const total = counts.get(c.name) ?? 1;
    const n = (seen.get(c.name) ?? 0) + 1;
    seen.set(c.name, n);
    const displayName = total > 1 ? `${c.name} (${n})` : c.name;
    const base = sanitizeBase(c.name) || 'col';
    const ordinal = n - 1; // 0-based ordinal per duplicate group
    return { id: `${base}__${ordinal}`, rawName: c.name, displayName };
  });
}

const FALLBACK_TARGET_FIELDS: TargetField[] = [
  { id: 'CallReceivedAt', name: 'Call Received At', required: true },
  { id: 'IncidentNumber', name: 'Incident Number', required: true },
  { id: 'City', name: 'City', required: false },
  { id: 'State', name: 'State', required: false },
  { id: 'Zip', name: 'Zip', required: false },
  { id: 'Address', name: 'Address', required: false },
  { id: 'Latitude', name: 'Latitude', required: false },
  { id: 'Longitude', name: 'Longitude', required: false },
  { id: 'UnitId', name: 'Unit ID', required: false },
  { id: 'Disposition', name: 'Disposition', required: false },
];

export default function App() {
  const [page, setPage] = useState<WizardPage>('platform');
  const [installMode, setInstallMode] = useState<InstallMode>('windows');

  const [modal, setModal] = useState<ModalState>({ kind: 'none' });

  // License acceptance state (Page 2)
  const [licenseAccepted, setLicenseAccepted] = useState(false);
  const licenseScrollRef = useRef<HTMLDivElement | null>(null);
  const licenseText =
    'LICENSE TEXT NOT PROVIDED.\n\nPlace your license text (EULA) under Prod_Install_Wizard_Deployment/licenses/ and wire the loader to display it here.';

  // Global wizard settings
  const [installationType, setInstallationType] = useState<InstallationType>('typical');
  const [importConfigPath, setImportConfigPath] = useState('');
  const [importConfigError, setImportConfigError] = useState<string | null>(null);

  const [destinationFolder, setDestinationFolder] = useState(defaultInstallPath('windows'));
  const [destinationError, setDestinationError] = useState<string | null>(null);

  // Data source/environment
  const [dataSourceKind, setDataSourceKind] = useState<'local' | 'remote'>('local');
  const [sourceObjectName, setSourceObjectName] = useState('dbo.CallData');

  // Call data connection (SQL Server; used for schema scan)
  const [callDataHost, setCallDataHost] = useState('localhost');
  const [callDataPort, setCallDataPort] = useState('1433');
  const [callDataDbName, setCallDataDbName] = useState('');
  const [callDataUser, setCallDataUser] = useState('');
  const [callDataPassword, setCallDataPassword] = useState('');

  // Database setup
  // D2 Database Setup Wizard (New vs Existing)
  const [dbSetupMode, setDbSetupMode] = useState<'createNew' | 'existing' | null>(null);
  // Phase 9: Create NEW - database name and admin connection fields
  const [newDbName, setNewDbName] = useState('CADalytix_Production');
  const [newDbAdminHost, setNewDbAdminHost] = useState('localhost');
  const [newDbAdminPort, setNewDbAdminPort] = useState('1433');
  const [newDbAdminUser, setNewDbAdminUser] = useState('sa');
  const [newDbAdminPassword, setNewDbAdminPassword] = useState('');
  const [newDbLocation, setNewDbLocation] = useState<'thisMachine' | 'specificPath'>('thisMachine');
  const [newDbSpecificPath, setNewDbSpecificPath] = useState('');
  const [newDbMaxSizeGb, setNewDbMaxSizeGb] = useState('50');
  // Phase 9: SQL Server sizing fields
  const [newDbInitialDataSizeMb, setNewDbInitialDataSizeMb] = useState('100');
  const [newDbInitialLogSizeMb, setNewDbInitialLogSizeMb] = useState('50');
  const [newDbMaxDataSizeMb, setNewDbMaxDataSizeMb] = useState('0'); // 0 = UNLIMITED
  const [newDbMaxLogSizeMb, setNewDbMaxLogSizeMb] = useState('0');
  const [newDbDataFilegrowth, setNewDbDataFilegrowth] = useState('64'); // MB
  const [newDbLogFilegrowth, setNewDbLogFilegrowth] = useState('-10'); // -10 = 10%
  // Phase 9: PostgreSQL owner field
  const [newDbPgOwner, setNewDbPgOwner] = useState('');
  // Phase 9: Create NEW - privilege test state
  const [newDbPrivTestStatus, setNewDbPrivTestStatus] = useState<'idle' | 'testing' | 'success' | 'fail'>('idle');
  const [newDbPrivTestMessage, setNewDbPrivTestMessage] = useState('');
  const [existingHostedWhere, setExistingHostedWhere] = useState<
    'on_prem' | 'aws_rds' | 'azure_sql' | 'gcp_cloud_sql' | 'neon' | 'supabase' | 'other'
  >('on_prem');

  const [dbName, setDbName] = useState('cadalytix');
  const [dbUser, setDbUser] = useState('cadalytix_admin');
  const [dbPassword, setDbPassword] = useState('');
  const [dbHost, setDbHost] = useState('localhost');
  const [dbPort, setDbPort] = useState('1433');
  const [dbSslMode, setDbSslMode] = useState<'disable' | 'prefer' | 'require'>('prefer');
  const [dbUseConnString, setDbUseConnString] = useState(false);
  const [dbConnString, setDbConnString] = useState('');

  const [dbTestStatus, setDbTestStatus] = useState<'idle' | 'testing' | 'success' | 'fail'>('idle');
  const [dbTestMessage, setDbTestMessage] = useState<string>('');

  // Storage policy
  const [storageMode, setStorageMode] = useState<'defaults' | 'custom'>('defaults');
  const [storageLocation, setStorageLocation] = useState<'system' | 'attached' | 'custom'>('system');
  const [storageCustomPath, setStorageCustomPath] = useState('');
  const [retentionPolicy, setRetentionPolicy] = useState<'18' | '12' | 'max' | 'keep'>('18');
  const [maxDiskGb, setMaxDiskGb] = useState('100');

  // Retention + archive policy (Phase 5 extension)
  const [hotRetentionChoice, setHotRetentionChoice] = useState<'12' | '18' | 'custom'>('18');
  const [hotRetentionCustomMonths, setHotRetentionCustomMonths] = useState('24');

  const [archiveFormat, setArchiveFormat] = useState<'zip+ndjson' | 'zip+csv'>('zip+ndjson');
  const [archiveDestinationPath, setArchiveDestinationPath] = useState('');
  const [archiveMaxUsageGb, setArchiveMaxUsageGb] = useState('10');
  const [archiveScheduleDayOfMonth, setArchiveScheduleDayOfMonth] = useState('1');
  const [archiveScheduleTimeLocal, setArchiveScheduleTimeLocal] = useState('00:05');
  const [archiveCatchUpOnStartup, setArchiveCatchUpOnStartup] = useState(true);

  // Consent to sync (OFF by default; stored only)
  const [consentToSync, setConsentToSync] = useState(false);
  const [consentDetailsExpanded, setConsentDetailsExpanded] = useState(false);

  // Schema mapping
  const [mappingOverride, setMappingOverride] = useState(false);
  const [mappingDemoMode, setMappingDemoMode] = useState(false);
  const [sourceFields, setSourceFields] = useState<SourceField[]>([]);
  const [targetFields] = useState<TargetField[]>(FALLBACK_TARGET_FIELDS);
  const [sourceSearch, setSourceSearch] = useState('');
  const [targetSearch, setTargetSearch] = useState('');
  const [selectedSourceId, setSelectedSourceId] = useState<string | null>(null);
  const [selectedTargetId, setSelectedTargetId] = useState<string | null>(null);
  const [sourceToTargets, setSourceToTargets] = useState<Record<string, string[]>>({});
  const [targetToSource, setTargetToSource] = useState<Record<string, string>>({});

  const [mappingScanError, setMappingScanError] = useState<string | null>(null);
  const [mappingScanning, setMappingScanning] = useState(false);
  const mappingAutoScanKeyRef = useRef<string>('');

  // Installing progress
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const [installLogFolder, setInstallLogFolder] = useState<string | null>(null);
  const [installManifestPath, setInstallManifestPath] = useState<string | null>(null);
  const [installMappingPath, setInstallMappingPath] = useState<string | null>(null);
  const [installConfigPath, setInstallConfigPath] = useState<string | null>(null);
  const [installDetailLines, setInstallDetailLines] = useState<string[]>([]);
  const isInstalling = page === 'installing';

  const platformFocus = useRef<'windows' | 'docker'>('windows');
  const dbPortTouchedRef = useRef(false);

  // Derived engine inference for DB test. We do NOT ask the user to pick a provider/engine;
  // we infer it from hosted-where, port defaults, or the connection string itself.
  const dbEngine: 'sqlserver' | 'postgres' = useMemo(() => {
    const guessFromConnString = (conn: string): 'sqlserver' | 'postgres' => {
      const s = conn.trim().toLowerCase();
      if (s.startsWith('postgres://') || s.startsWith('postgresql://')) return 'postgres';
      if (s.includes('host=')) return 'postgres';
      // Default to SQL Server when ambiguous.
      return 'sqlserver';
    };

    // Create NEW path: infer from admin port
    if (dbSetupMode === 'createNew') {
      const port = newDbAdminPort.trim();
      if (port === '5432') return 'postgres';
      if (port === '1433') return 'sqlserver';
      // Fallback based on install mode
      return installMode === 'windows' ? 'sqlserver' : 'postgres';
    }

    // Existing DB path: infer from explicit hosting selection when available.
    if (dbSetupMode === 'existing') {
      if (dbUseConnString && dbConnString.trim()) {
        return guessFromConnString(dbConnString);
      }
      if (existingHostedWhere === 'azure_sql') return 'sqlserver';
      if (existingHostedWhere === 'neon' || existingHostedWhere === 'supabase') return 'postgres';

      const port = dbPort.trim();
      if (port === '1433') return 'sqlserver';
      if (port === '5432') return 'postgres';
    }

    // Fallback: prior behavior based on install mode.
    return installMode === 'windows' ? 'sqlserver' : 'postgres';
  }, [dbConnString, dbPort, dbSetupMode, dbUseConnString, existingHostedWhere, installMode, newDbAdminPort]);

  const computedConfigDbConnectionString = useMemo(() => {
    if (dbUseConnString && dbConnString.trim()) return dbConnString.trim();

    if (dbEngine === 'postgres') {
      const port = dbPort.trim() || '5432';
      const ssl = dbSslMode;
      const user = encodeURIComponent(dbUser.trim());
      const pass = encodeURIComponent(dbPassword);
      const host = dbHost.trim() || 'localhost';
      const db = dbName.trim() || 'cadalytix';
      return `postgresql://${user}:${pass}@${host}:${port}/${db}?sslmode=${ssl}`;
    }

    // SQL Server: use a basic ADO-style connection string.
    const host = dbHost.trim() || 'localhost';
    const port = dbPort.trim();
    const server = port ? `${host},${port}` : host;
    const db = dbName.trim() || 'cadalytix';
    const user = dbUser.trim();
    const pass = dbPassword;
    const encrypt = dbSslMode === 'disable' ? 'false' : 'true';
    return `Server=${server};Database=${db};User Id=${user};Password=${pass};TrustServerCertificate=true;Encrypt=${encrypt};`;
  }, [dbConnString, dbEngine, dbHost, dbName, dbPassword, dbPort, dbSslMode, dbUseConnString, dbUser]);

  const computedCallDataConnectionString = useMemo(() => {
    const host = callDataHost.trim() || 'localhost';
    const port = callDataPort.trim() || '1433';
    const server = port ? `${host},${port}` : host;
    const db = callDataDbName.trim();
    const user = callDataUser.trim();
    const pass = callDataPassword;
    return `Server=${server};Database=${db};User Id=${user};Password=${pass};TrustServerCertificate=true;Encrypt=false;`;
  }, [callDataDbName, callDataHost, callDataPassword, callDataPort, callDataUser]);

  // Phase 9: Compute maintenance/admin connection string for Create NEW mode
  // Points to master (SQL Server) or postgres (PostgreSQL) database
  const computedCreateNewMaintenanceConnString = useMemo(() => {
    if (dbEngine === 'postgres') {
      const port = newDbAdminPort.trim() || '5432';
      const user = encodeURIComponent(newDbAdminUser.trim());
      const pass = encodeURIComponent(newDbAdminPassword);
      const host = newDbAdminHost.trim() || 'localhost';
      // Connect to postgres maintenance database
      return `postgresql://${user}:${pass}@${host}:${port}/postgres?sslmode=prefer`;
    }
    // SQL Server: connect to master database
    const host = newDbAdminHost.trim() || 'localhost';
    const port = newDbAdminPort.trim() || '1433';
    const server = port ? `${host},${port}` : host;
    const user = newDbAdminUser.trim();
    const pass = newDbAdminPassword;
    return `Server=${server};Database=master;User Id=${user};Password=${pass};TrustServerCertificate=true;Encrypt=false;`;
  }, [dbEngine, newDbAdminHost, newDbAdminPassword, newDbAdminPort, newDbAdminUser]);

  const dbCreateValidationError = useMemo(() => {
    if (dbSetupMode !== 'createNew') return null;
    // Validate database name
    if (!newDbName.trim()) return 'New database name is required.';
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(newDbName.trim())) return 'Database name must start with letter/underscore and contain only letters, digits, underscores.';
    if (newDbName.trim().length > 128) return 'Database name too long (max 128 characters).';
    // Validate admin connection fields
    if (!newDbAdminHost.trim()) return 'Admin host is required.';
    if (!newDbAdminPort.trim()) return 'Admin port is required.';
    if (!newDbAdminUser.trim()) return 'Admin username is required.';
    if (!newDbAdminPassword.trim()) return 'Admin password is required.';
    // Validate sizing
    const gb = parseInt(newDbMaxSizeGb.trim(), 10);
    if (!Number.isFinite(gb) || gb <= 0) return 'Max DB size must be a positive number.';
    if (newDbLocation === 'specificPath' && !newDbSpecificPath.trim()) return 'Database path is required.';
    return null;
  }, [dbSetupMode, newDbLocation, newDbMaxSizeGb, newDbSpecificPath, newDbName, newDbAdminHost, newDbAdminPort, newDbAdminUser, newDbAdminPassword]);

  const dbExistingMissingInputs = useMemo(() => {
    if (dbSetupMode !== 'existing') return [] as string[];
    const missing: string[] = [];
    // Hosted where is required (we default to on-prem for safety, but still validate it exists).
    if (!existingHostedWhere) missing.push('Hosted where');

    if (dbUseConnString) {
      if (!dbConnString.trim()) missing.push('Connection string');
    } else {
      if (!dbHost.trim()) missing.push('Host');
      if (!dbPort.trim()) missing.push('Port');
      if (!dbName.trim()) missing.push('Database');
      if (!dbUser.trim()) missing.push('Username');
      if (!dbPassword.trim()) missing.push('Password');
    }
    return missing;
  }, [
    dbSetupMode,
    existingHostedWhere,
    dbUseConnString,
    dbConnString,
    dbHost,
    dbPort,
    dbName,
    dbUser,
    dbPassword,
    dbEngine,
  ]);

  // Best-effort: nudge the port default based on install mode / hosting selection.
  // This runs only while the user has not manually edited the port value.
  useEffect(() => {
    if (page !== 'database') return;
    if (dbSetupMode !== 'existing') return;
    if (dbUseConnString) return;
    if (dbPortTouchedRef.current) return;

    const recommended =
      existingHostedWhere === 'azure_sql'
        ? '1433'
        : existingHostedWhere === 'neon' || existingHostedWhere === 'supabase'
          ? '5432'
          : installMode === 'docker'
            ? '5432'
            : '1433';

    if (dbPort.trim() !== recommended) setDbPort(recommended);
  }, [dbPort, dbSetupMode, dbUseConnString, existingHostedWhere, installMode, page]);

  const canRunDbTest = useMemo(
    () => dbSetupMode === 'existing' && dbExistingMissingInputs.length === 0,
    [dbExistingMissingInputs.length, dbSetupMode]
  );

  const hotRetentionMonths = useMemo(() => {
    if (hotRetentionChoice === '12') return 12;
    if (hotRetentionChoice === '18') return 18;
    const n = parseInt(hotRetentionCustomMonths.trim(), 10);
    return Number.isFinite(n) && n > 0 ? n : 0;
  }, [hotRetentionChoice, hotRetentionCustomMonths]);

  const retentionValidationError = useMemo(() => {
    if (hotRetentionChoice !== 'custom') return null;
    if (hotRetentionMonths <= 0) return 'Enter a valid number of months.';
    if (hotRetentionMonths > 240) return 'Custom months is too large.';
    return null;
  }, [hotRetentionChoice, hotRetentionMonths]);

  const archiveValidationError = useMemo(() => {
    if (!archiveDestinationPath.trim()) return 'Archive destination folder is required.';
    const gb = parseInt(archiveMaxUsageGb.trim(), 10);
    if (!Number.isFinite(gb) || gb <= 0) return 'Max archive usage must be a positive number.';
    const day = parseInt(archiveScheduleDayOfMonth.trim(), 10);
    if (!Number.isFinite(day) || day < 1 || day > 28) return 'Schedule day must be between 1 and 28.';
    const t = archiveScheduleTimeLocal.trim();
    const m = /^(\d{2}):(\d{2})$/.exec(t);
    if (!m) return 'Schedule time must be HH:MM.';
    const hh = parseInt(m[1], 10);
    const mm = parseInt(m[2], 10);
    if (hh < 0 || hh > 23 || mm < 0 || mm > 59) return 'Schedule time must be HH:MM.';
    return null;
  }, [archiveDestinationPath, archiveMaxUsageGb, archiveScheduleDayOfMonth, archiveScheduleTimeLocal]);

  const requiredTargetsUnmapped = useMemo(() => {
    const required = targetFields.filter((t) => t.required);
    const unmapped = required.filter((t) => !targetToSource[t.id]);
    return unmapped;
  }, [targetFields, targetToSource]);

  const mappedCount = useMemo(() => Object.keys(targetToSource).length, [targetToSource]);

  useEffect(() => {
    let unlistenReady: (() => void) | null = null;
    let unlistenProgress: (() => void) | null = null;
    let unlistenInstallComplete: (() => void) | null = null;
    let unlistenInstallError: (() => void) | null = null;

    (async () => {
      unlistenReady = await listenToEvent<InstallerReadyEvent>('installer-ready', (event) => {
        void event;
      });
      unlistenProgress = await listenToEvent<ProgressEvent>('progress', (evt) => {
        setProgress(evt);
        if (evt.message && evt.message.trim()) {
          setInstallDetailLines((prev) => [...prev, evt.message as string].slice(-20));
        }
      });
      unlistenInstallComplete = await listenToEvent<InstallResultEvent>('install-complete', (evt) => {
        setInstallLogFolder(evt.details?.logFolder ?? null);
        setInstallManifestPath(evt.details?.manifestPath ?? null);
        setInstallMappingPath(evt.details?.mappingPath ?? null);
        setInstallConfigPath(evt.details?.configPath ?? null);
        goTo('complete');
      });
      unlistenInstallError = await listenToEvent<InstallResultEvent>('install-error', (evt) => {
        setInstallError(evt.message || 'Installation failed.');
        const logFolder = evt.details?.logFolder ?? null;
        setInstallLogFolder(logFolder);
        setInstallManifestPath(evt.details?.manifestPath ?? null);
        setInstallMappingPath(evt.details?.mappingPath ?? null);
        setInstallConfigPath(evt.details?.configPath ?? null);

        const bodyLines: string[] = [evt.message || 'An error occurred during installation.'];
        if (logFolder) bodyLines.push('', `Log folder: ${logFolder}`);

        setModal({
          kind: 'error',
          title: 'Installation failed',
          body: bodyLines.join('\n'),
          primaryLabel: 'OK',
          onPrimary: () => setModal({ kind: 'none' }),
          secondaryLabel: 'Create support bundle…',
          onSecondary: async () => {
            setModal({ kind: 'none' });
            try {
              const resp = await invoke<{ bundleDir: string }>('create_support_bundle', {
                payload: {
                  destinationFolder: destinationFolder,
                },
              });
              setModal({
                kind: 'error',
                title: 'Support bundle created',
                body: `Support bundle folder: ${resp.bundleDir}`,
                primaryLabel: 'OK',
                onPrimary: () => setModal({ kind: 'none' }),
                onSecondary: null,
                secondaryLabel: undefined,
                onTertiary: null,
                tertiaryLabel: undefined,
              });
            } catch (e: any) {
              setModal({
                kind: 'error',
                title: 'Support bundle failed',
                body: `Unable to create support bundle.${logFolder ? `\n\nLog folder: ${logFolder}` : ''}`,
                primaryLabel: 'OK',
                onPrimary: () => setModal({ kind: 'none' }),
                onSecondary: null,
                secondaryLabel: undefined,
                onTertiary: null,
                tertiaryLabel: undefined,
              });
            }
          },
          tertiaryLabel: logFolder ? 'Copy log path' : undefined,
          onTertiary: logFolder
            ? () => {
                try {
                  void navigator.clipboard.writeText(logFolder);
                } catch {}
              }
            : null,
        });
      });
    })();

    return () => {
      try {
        unlistenReady?.();
      } catch {}
      try {
        unlistenProgress?.();
      } catch {}
      try {
        unlistenInstallComplete?.();
      } catch {}
      try {
        unlistenInstallError?.();
      } catch {}
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      // Modal keyboard behavior (spec): Esc cancels/closes; Enter accepts primary action.
      if (modal.kind !== 'none') {
        if (e.key === 'Escape') {
          e.preventDefault();
          if (modal.onSecondary) modal.onSecondary();
          else if (modal.onPrimary) modal.onPrimary();
          return;
        }
        if (e.key === 'Enter') {
          e.preventDefault();
          if (modal.onPrimary) modal.onPrimary();
          return;
        }
        return;
      }

      if (e.key === 'Escape') {
        e.preventDefault();
        openCancelConfirm();
        return;
      }

      // Enter triggers default action (Next/Install/Finish) unless user is typing.
      if (e.key === 'Enter') {
        const el = document.activeElement;
        const tag = el ? (el as HTMLElement).tagName.toLowerCase() : '';
        if (tag === 'input' || tag === 'textarea' || tag === 'select' || (el as HTMLElement)?.isContentEditable) {
          return;
        }
        e.preventDefault();
        onNext();
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [
    modal.kind,
    page,
    installMode,
    installationType,
    importConfigPath,
    destinationFolder,
    dbTestStatus,
    sourceToTargets,
    targetToSource,
    mappingOverride,
    computedConfigDbConnectionString,
    computedCallDataConnectionString,
    sourceObjectName,
    retentionValidationError,
    archiveValidationError,
    hotRetentionMonths,
    archiveFormat,
    archiveDestinationPath,
    archiveMaxUsageGb,
    archiveScheduleDayOfMonth,
    archiveScheduleTimeLocal,
    archiveCatchUpOnStartup,
    consentToSync,
  ]);

  function openCancelConfirm() {
    setModal({
      kind: 'confirmCancel',
      title: 'Cancel Setup?',
      body: 'If you cancel now, the installation may be incomplete.',
      primaryLabel: 'Yes, cancel',
      secondaryLabel: 'No',
      onPrimary: async () => {
        setModal({ kind: 'none' });
        if (page === 'installing') {
          try {
            await invoke('cancel_install');
            setProgress((prev) =>
              prev
                ? { ...prev, message: 'Cancelling installation...' }
                : {
                    correlationId: 'pending',
                    step: 'cancel',
                    severity: 'info',
                    phase: 'install',
                    percent: 0,
                    message: 'Cancelling installation...',
                  }
            );
          } catch {
            // Best-effort cancel (errors are handled by backend logs)
          }
          return;
        }

        // Non-install pages: Cancel closes the wizard.
        try {
          await getCurrentWindow().close();
        } catch {
          // If closing fails, fall back to returning to the first screen.
          setInstallError(null);
          setProgress(null);
          setInstallLogFolder(null);
          setInstallManifestPath(null);
          setInstallMappingPath(null);
          setInstallConfigPath(null);
          setInstallDetailLines([]);
          setPage('platform');
        }
      },
      onSecondary: () => setModal({ kind: 'none' }),
    });
  }

  function openError(title: string, body: string) {
    setModal({
      kind: 'error',
      title,
      body,
      primaryLabel: 'OK',
      onPrimary: () => setModal({ kind: 'none' }),
      onSecondary: null,
      secondaryLabel: undefined,
      onTertiary: null,
      tertiaryLabel: undefined,
    });
  }

  function goTo(nextPage: WizardPage) {
    setPage(nextPage);
  }

  function buildCanonicalToSourceColumnMappings(): Record<string, string> {
    const out: Record<string, string> = {};
    for (const [canonicalFieldId, sourceId] of Object.entries(targetToSource)) {
      const src = sourceFields.find((s) => s.id === sourceId);
      if (src) out[canonicalFieldId] = src.rawName;
    }
    return out;
  }

  function buildMappingStateForPayload() {
    return {
      mappingOverride,
      sourceFields,
      targetFields,
      sourceToTargets,
      targetToSource,
    };
  }

  function onBack() {
    if (isInstalling) return;
    const order: WizardPage[] = [
      'platform',
      'welcome',
      'license',
      'installType',
      'destination',
      'dataSource',
      'database',
      'storage',
      'retention',
      'archive',
      'consent',
      'mapping',
      'ready',
      'installing',
      'complete',
    ];
    const idx = order.indexOf(page);
    if (idx <= 0) return;
    goTo(order[idx - 1]);
  }

  async function onNext() {
    if (isInstalling) return;

    if (page === 'platform') {
      // Screen 0: selection occurs via buttons; Next does nothing.
      return;
    }

    if (page === 'welcome') {
      goTo('license');
      return;
    }

    if (page === 'license') {
      goTo('installType');
      return;
    }

    if (page === 'installType') {
      if (installationType === 'import') {
        if (!importConfigPath.trim()) return;
        // Best-effort validate by asking backend (fail closed on error).
        try {
          setImportConfigError(null);
          const ok = await invoke<boolean>('file_exists', { payload: { path: importConfigPath.trim() } });
          if (!ok) {
            setImportConfigError('Selected file could not be read.');
            return;
          }
        } catch {
          setImportConfigError('Selected file could not be read.');
          return;
        }
      }
      goTo('destination');
      return;
    }

    if (page === 'destination') {
      if (destinationError) return;
      goTo('dataSource');
      return;
    }

    if (page === 'dataSource') {
      goTo('database');
      return;
    }

    if (page === 'database') {
      if (!dbSetupMode) return;
      if (dbSetupMode === 'createNew') {
        if (dbCreateValidationError) return;
      } else {
        if (dbTestStatus !== 'success') return;
      }
      goTo('storage');
      return;
    }

    if (page === 'storage') {
      goTo('retention');
      return;
    }

    if (page === 'retention') {
      if (retentionValidationError) return;
      goTo('archive');
      return;
    }

    if (page === 'archive') {
      if (archiveValidationError) return;
      goTo('consent');
      return;
    }

    if (page === 'consent') {
      goTo('mapping');
      return;
    }

    if (page === 'mapping') {
      if (requiredTargetsUnmapped.length > 0) return;
      goTo('ready');
      return;
    }

    if (page === 'ready') {
      // Begin install
      setInstallError(null);
      setProgress({
        correlationId: 'pending',
        step: 'start',
        severity: 'info',
        phase: 'install',
        percent: 0,
        message: 'Starting installation...',
      });
      setInstallLogFolder(null);
      setInstallManifestPath(null);
      setInstallMappingPath(null);
      setInstallConfigPath(null);
      setInstallDetailLines([]);
      goTo('installing');

      try {
        await invoke('start_install', {
          payload: {
            installMode,
            installationType,
            destinationFolder,
            // Phase 9: For Create NEW, send maintenance connection string (master/postgres)
            configDbConnectionString: dbSetupMode === 'createNew' ? computedCreateNewMaintenanceConnString : computedConfigDbConnectionString,
            callDataConnectionString: computedCallDataConnectionString,
            sourceObjectName,
            dbSetup: {
              mode: dbSetupMode === 'createNew' ? 'create_new' : 'existing',
              // Phase 9: Include new database name for Create NEW mode
              newDbName: dbSetupMode === 'createNew' ? newDbName.trim() : undefined,
              newLocation: newDbLocation === 'thisMachine' ? 'this_machine' : 'specific_path',
              newSpecificPath: newDbLocation === 'specificPath' ? newDbSpecificPath.trim() : '',
              maxDbSizeGb: parseInt(newDbMaxSizeGb.trim(), 10) || 0,
              existingHostedWhere,
              existingConnectMode: dbUseConnString ? 'connection_string' : 'details',
              // Phase 9: SQL Server sizing
              sqlServerSizing: dbEngine === 'sqlserver' && dbSetupMode === 'createNew' ? {
                initialDataSizeMb: parseInt(newDbInitialDataSizeMb.trim(), 10) || 0,
                initialLogSizeMb: parseInt(newDbInitialLogSizeMb.trim(), 10) || 0,
                maxDataSizeMb: parseInt(newDbMaxDataSizeMb.trim(), 10) || 0,
                maxLogSizeMb: parseInt(newDbMaxLogSizeMb.trim(), 10) || 0,
                dataFilegrowth: parseInt(newDbDataFilegrowth.trim(), 10) || 0,
                logFilegrowth: parseInt(newDbLogFilegrowth.trim(), 10) || 0,
              } : undefined,
              // Phase 9: PostgreSQL options
              postgresOptions: dbEngine === 'postgres' && dbSetupMode === 'createNew' ? {
                owner: newDbPgOwner.trim() || undefined,
              } : undefined,
            },
            storage: {
              mode: storageMode,
              location: storageLocation,
              customPath: storageCustomPath,
              retentionPolicy,
              maxDiskGb,
            },
            hotRetention: {
              months: hotRetentionMonths,
            },
            archivePolicy: {
              format: archiveFormat,
              destinationPath: archiveDestinationPath.trim(),
              maxUsageGb: parseInt(archiveMaxUsageGb.trim(), 10),
              schedule: {
                dayOfMonth: parseInt(archiveScheduleDayOfMonth.trim(), 10),
                timeLocal: archiveScheduleTimeLocal.trim(),
              },
              catchUpOnStartup: archiveCatchUpOnStartup,
            },
            consentToSync,
            mappings: buildCanonicalToSourceColumnMappings(),
            mappingOverride,
            mappingState: buildMappingStateForPayload(),
          },
        });
      } catch (e: any) {
        const msg = e?.message || String(e);
        setInstallError(msg);
        openError('Installation failed', 'An error occurred during installation. Please check logs.');
      }

      return;
    }

    if (page === 'complete') {
      // Finish closes wizard (return to start screen for now).
      setPage('platform');
      return;
    }
  }

  async function browseForFolder() {
    const selected = await open({ directory: true, multiple: false, title: 'Select Destination Folder' });
    if (typeof selected === 'string' && selected.trim()) {
      setDestinationFolder(installMode === 'windows' ? normalizeWindowsPath(selected) : selected);
    }
  }

  async function browseForNewDbPath() {
    const selected = await open({ directory: true, multiple: false, title: 'Select Database Folder' });
    if (typeof selected === 'string' && selected.trim()) {
      setNewDbSpecificPath(installMode === 'windows' ? normalizeWindowsPath(selected) : selected);
    }
  }

  async function browseForArchiveFolder() {
    const selected = await open({ directory: true, multiple: false, title: 'Select Archive Destination Folder' });
    if (typeof selected === 'string' && selected.trim()) {
      setArchiveDestinationPath(installMode === 'windows' ? normalizeWindowsPath(selected) : selected);
    }
  }

  async function browseForFile() {
    const selected = await open({ directory: false, multiple: false, title: 'Select Configuration File' });
    if (typeof selected === 'string' && selected.trim()) {
      setImportConfigPath(installMode === 'windows' ? normalizeWindowsPath(selected) : selected);
      setImportConfigError(null);
    }
  }

  async function runDbTest() {
    if (dbSetupMode !== 'existing') return;
    if (!canRunDbTest) {
      setDbTestStatus('fail');
      setDbTestMessage(
        `Connection failed: missing required inputs (${dbExistingMissingInputs.join(', ')}).`
      );
      return;
    }
    setDbTestStatus('testing');
    setDbTestMessage('');
    try {
      const res = await invoke<TestDbConnectionResponse>('test_db_connection', {
        payload: { engine: dbEngine, connectionString: computedConfigDbConnectionString },
      });
      if (res.success) {
        setDbTestStatus('success');
        setDbTestMessage('Connection successful.');
      } else {
        setDbTestStatus('fail');
        setDbTestMessage(`Connection failed: ${res.message}`);
      }
    } catch (e: any) {
      setDbTestStatus('fail');
      setDbTestMessage(`Connection failed: ${e?.message || String(e)}`);
    }
  }

  // Phase 9: Test connection and privileges for Create NEW mode
  async function runCreateNewPrivilegeTest() {
    if (!newDbAdminHost.trim() || !newDbAdminUser.trim() || !newDbAdminPassword.trim()) {
      setNewDbPrivTestStatus('fail');
      setNewDbPrivTestMessage('Host, username, and password are required.');
      return;
    }
    setNewDbPrivTestStatus('testing');
    setNewDbPrivTestMessage('');
    try {
      const res = await invoke<{ canCreate: boolean; reason: string; detectedRole?: string }>('db_can_create_database', {
        payload: { engine: dbEngine, connectionString: computedCreateNewMaintenanceConnString },
      });
      if (res.canCreate) {
        // Also check if database already exists
        const existsRes = await invoke<{ exists: boolean; error?: string }>('db_exists', {
          payload: { engine: dbEngine, connectionString: computedCreateNewMaintenanceConnString, dbName: newDbName.trim() },
        });
        if (existsRes.exists) {
          setNewDbPrivTestStatus('fail');
          setNewDbPrivTestMessage(`Database "${newDbName.trim()}" already exists.`);
        } else {
          setNewDbPrivTestStatus('success');
          setNewDbPrivTestMessage(res.reason || 'Privileges OK.');
        }
      } else {
        setNewDbPrivTestStatus('fail');
        setNewDbPrivTestMessage(res.reason || 'Insufficient privileges.');
      }
    } catch (e: any) {
      setNewDbPrivTestStatus('fail');
      setNewDbPrivTestMessage(e?.message || String(e));
    }
  }

  async function scanSourceFields() {
    setMappingScanError(null);
    setMappingScanning(true);
    try {
      const res = await preflightDataSource({
        callDataConnectionString: computedCallDataConnectionString,
        sourceObjectName,
        sampleLimit: 10,
        demoMode: mappingDemoMode,
      });
      if (!res.success || !res.data) {
        setMappingScanError(res.error || 'Unable to scan source fields.');
        return;
      }
      setSourceFields(disambiguateSourceColumns(res.data.discoveredColumns));
    } catch (e: any) {
      setMappingScanError(e?.message || String(e));
    } finally {
      setMappingScanning(false);
    }
  }

  // Mapping page: auto-scan source headers on entry and when demo mode toggles.
  useEffect(() => {
    if (page === 'mapping') {
      const key = mappingDemoMode ? 'demo' : 'real';
      if (mappingAutoScanKeyRef.current === key) return;
      mappingAutoScanKeyRef.current = key;
      void scanSourceFields();
    } else {
      mappingAutoScanKeyRef.current = '';
    }
    // Intentionally depend only on page to avoid re-scanning on every keystroke.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [page, mappingDemoMode]);

  function unassignSelected() {
    if (!selectedSourceId || !selectedTargetId) return;
    const currentSource = targetToSource[selectedTargetId];
    if (currentSource !== selectedSourceId) return;

    setTargetToSource((prev) => {
      const next = { ...prev };
      delete next[selectedTargetId];
      return next;
    });
    setSourceToTargets((prev) => {
      const next = { ...prev };
      const arr = next[selectedSourceId] ?? [];
      next[selectedSourceId] = arr.filter((t) => t !== selectedTargetId);
      return next;
    });
  }

  function applyMapping(sourceId: string, targetId: string, mode: 'replace' | 'add') {
    setTargetToSource((prev) => ({ ...prev, [targetId]: sourceId }));
    setSourceToTargets((prev) => {
      const next = { ...prev };
      const existing = next[sourceId] ?? [];
      if (mode === 'replace') {
        next[sourceId] = [targetId];
      } else {
        next[sourceId] = existing.includes(targetId) ? existing : [...existing, targetId];
      }
      return next;
    });
  }

  function removeTargetFromOldSource(targetId: string) {
    const oldSource = targetToSource[targetId];
    if (!oldSource) return;
    setSourceToTargets((prev) => {
      const next = { ...prev };
      const arr = next[oldSource] ?? [];
      next[oldSource] = arr.filter((t) => t !== targetId);
      return next;
    });
    setTargetToSource((prev) => {
      const next = { ...prev };
      delete next[targetId];
      return next;
    });
  }

  function attemptMap(sourceId: string, targetId: string) {
    const targetAlreadyMappedTo = targetToSource[targetId];
    const sourceAlreadyMappedTo = sourceToTargets[sourceId] ?? [];

    const targetName = targetFields.find((t) => t.id === targetId)?.name ?? targetId;
    const sourceName = sourceFields.find((s) => s.id === sourceId)?.displayName ?? sourceId;

    // Unlink rule: clicking an already-mapped pair toggles it off.
    if (targetAlreadyMappedTo && targetAlreadyMappedTo === sourceId) {
      removeTargetFromOldSource(targetId);
      return;
    }

    // CASE C (both mapped): target is mapped to another source AND source already has a mapping (and override off OR user intent ambiguous)
    if (targetAlreadyMappedTo && targetAlreadyMappedTo !== sourceId) {
      const oldSourceName = sourceFields.find((s) => s.id === targetAlreadyMappedTo)?.displayName ?? targetAlreadyMappedTo;

      const hasSourceMapping = sourceAlreadyMappedTo.length > 0 && !sourceAlreadyMappedTo.includes(targetId);
      if (hasSourceMapping) {
        if (mappingOverride) {
          setModal({
            kind: 'sourceAlreadyMapped',
            title: 'Source already mapped',
            body: `Target "${targetName}" is currently mapped to Source "${oldSourceName}".\n\nSource "${sourceName}" is currently mapped to: ${sourceAlreadyMappedTo
              .map((t) => targetFields.find((x) => x.id === t)?.name ?? t)
              .join(', ')}.\n\nWhat would you like to do?`,
            primaryLabel: 'Add',
            secondaryLabel: 'Replace',
            tertiaryLabel: 'Cancel',
            onPrimary: () => {
              setModal({ kind: 'none' });
              removeTargetFromOldSource(targetId);
              applyMapping(sourceId, targetId, 'add');
            },
            onSecondary: () => {
              setModal({ kind: 'none' });
              // Replace source mapping(s) and replace target mapping
              removeTargetFromOldSource(targetId);
              setSourceToTargets((prev) => ({ ...prev, [sourceId]: [] }));
              applyMapping(sourceId, targetId, 'replace');
            },
            onTertiary: () => setModal({ kind: 'none' }),
          });
          return;
        }

        setModal({
          kind: 'replaceMapping',
          title: 'Replace mapping?',
          body: `Target "${targetName}" is currently mapped to Source "${oldSourceName}".\nSource "${sourceName}" is currently mapped to Target "${targetFields.find((x) => x.id === sourceAlreadyMappedTo[0])?.name ?? sourceAlreadyMappedTo[0]}".\n\nDo you want to replace these mappings with Source "${sourceName}" → Target "${targetName}"?`,
          primaryLabel: 'Replace',
          secondaryLabel: 'Cancel',
          onPrimary: () => {
            setModal({ kind: 'none' });
            removeTargetFromOldSource(targetId);
            setSourceToTargets((prev) => ({ ...prev, [sourceId]: [] }));
            applyMapping(sourceId, targetId, 'replace');
          },
          onSecondary: () => setModal({ kind: 'none' }),
        });
        return;
      }

      // CASE A (target already mapped)
      setModal({
        kind: 'replaceMapping',
        title: 'Replace mapping?',
        body: `Target "${targetName}" is currently mapped to Source "${oldSourceName}".\nDo you want to replace it with Source "${sourceName}"?`,
        primaryLabel: 'Replace',
        secondaryLabel: 'Cancel',
        onPrimary: () => {
          setModal({ kind: 'none' });
          removeTargetFromOldSource(targetId);
          if (!mappingOverride && sourceAlreadyMappedTo.length > 0 && !sourceAlreadyMappedTo.includes(targetId)) {
            // Source already mapped (override off): replace existing source mapping
            setSourceToTargets((prev) => ({ ...prev, [sourceId]: [] }));
            applyMapping(sourceId, targetId, 'replace');
          } else {
            applyMapping(sourceId, targetId, mappingOverride ? 'add' : 'replace');
          }
        },
        onSecondary: () => setModal({ kind: 'none' }),
      });
      return;
    }

    // CASE B (source already mapped)
    if (!mappingOverride && sourceAlreadyMappedTo.length > 0 && !sourceAlreadyMappedTo.includes(targetId)) {
      const oldTargetName = targetFields.find((t) => t.id === sourceAlreadyMappedTo[0])?.name ?? sourceAlreadyMappedTo[0];
      setModal({
        kind: 'replaceMapping',
        title: 'Replace mapping?',
        body: `Source "${sourceName}" is currently mapped to Target "${oldTargetName}".\nDo you want to replace it with Target "${targetName}"?`,
        primaryLabel: 'Replace',
        secondaryLabel: 'Cancel',
        onPrimary: () => {
          setModal({ kind: 'none' });
          // Remove old target->source mapping(s) for this source
          for (const t of sourceAlreadyMappedTo) {
            removeTargetFromOldSource(t);
          }
          applyMapping(sourceId, targetId, 'replace');
        },
        onSecondary: () => setModal({ kind: 'none' }),
      });
      return;
    }

    if (mappingOverride && sourceAlreadyMappedTo.length > 0 && !sourceAlreadyMappedTo.includes(targetId)) {
      setModal({
        kind: 'sourceAlreadyMapped',
        title: 'Source already mapped',
        body: `Source "${sourceName}" is currently mapped to: ${sourceAlreadyMappedTo
          .map((t) => targetFields.find((x) => x.id === t)?.name ?? t)
          .join(', ')}.\nWhat would you like to do?`,
        primaryLabel: 'Add',
        secondaryLabel: 'Replace',
        tertiaryLabel: 'Cancel',
        onPrimary: () => {
          setModal({ kind: 'none' });
          applyMapping(sourceId, targetId, 'add');
        },
        onSecondary: () => {
          setModal({ kind: 'none' });
          for (const t of sourceAlreadyMappedTo) {
            removeTargetFromOldSource(t);
          }
          applyMapping(sourceId, targetId, 'replace');
        },
        onTertiary: () => setModal({ kind: 'none' }),
      });
      return;
    }

    // No conflicts: map directly.
    applyMapping(sourceId, targetId, mappingOverride ? 'add' : 'replace');
  }

  // Destination validation (best-effort synchronous validation).
  useEffect(() => {
    const p = destinationFolder.trim();
    if (!p) {
      setDestinationError('Destination folder is required.');
      return;
    }
    setDestinationError(null);
  }, [destinationFolder]);

  // When mode changes, update default install path if user hasn’t customized it much.
  useEffect(() => {
    setDestinationFolder(defaultInstallPath(installMode));
  }, [installMode]);

  const wizardTitle = useMemo(() => {
    switch (page) {
      case 'platform':
        return 'CADalytix Setup';
      case 'welcome':
        return 'Welcome to the CADalytix Setup Wizard';
      case 'license':
        return 'License Agreement';
      case 'installType':
        return 'Installation Type';
      case 'destination':
        return 'Destination Folder';
      case 'dataSource':
        return 'Data Source';
      case 'database':
        return 'Database Setup';
      case 'storage':
        return 'Database Storage';
      case 'retention':
        return 'Hot Retention';
      case 'archive':
        return 'Archive Policy';
      case 'consent':
        return 'Support Improvements';
      case 'mapping':
        return 'Schema Mapping';
      case 'ready':
        return 'Ready to Install';
      case 'installing':
        return 'Installing CADalytix';
      case 'complete':
        return 'Completed';
      default:
        return 'CADalytix Setup';
    }
  }, [page]);

  const nextLabel = page === 'ready' ? 'Install' : page === 'complete' ? 'Finish' : 'Next';
  const backDisabled = page === 'platform' || page === 'welcome' || page === 'installing' || page === 'complete';
  const cancelDisabled = page === 'complete';

  const nextDisabled = useMemo(() => {
    if (page === 'platform') return true;
    if (page === 'welcome') return false;
    if (page === 'license') return !licenseAccepted;
    if (page === 'installType') {
      if (installationType === 'import') return !importConfigPath.trim() || !!importConfigError;
      return true ? false : false;
    }
    if (page === 'destination') return !!destinationError;
    if (page === 'database') {
      if (!dbSetupMode) return true;
      if (dbSetupMode === 'createNew') return !!dbCreateValidationError;
      return dbTestStatus !== 'success';
    }
    if (page === 'retention') return !!retentionValidationError;
    if (page === 'archive') return !!archiveValidationError;
    if (page === 'consent') return false;
    if (page === 'mapping') return requiredTargetsUnmapped.length > 0;
    if (page === 'installing') return true;
    return false;
  }, [
    page,
    destinationError,
    dbTestStatus,
    dbSetupMode,
    dbCreateValidationError,
    requiredTargetsUnmapped.length,
    installationType,
    importConfigError,
    importConfigPath,
    licenseAccepted,
    retentionValidationError,
    archiveValidationError,
  ]);

  function platformKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      platformFocus.current = platformFocus.current === 'windows' ? 'docker' : 'windows';
      const id = platformFocus.current === 'windows' ? 'platform-windows' : 'platform-docker';
      const el = document.getElementById(id) as HTMLButtonElement | null;
      el?.focus();
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      if (platformFocus.current === 'windows') {
        setInstallMode('windows');
      } else {
        setInstallMode('docker');
      }
      goTo('welcome');
    }
  }

  const filteredSourceFields = useMemo(() => {
    const q = sourceSearch.trim().toLowerCase();
    if (!q) return sourceFields;
    return sourceFields.filter((s) => s.displayName.toLowerCase().includes(q));
  }, [sourceFields, sourceSearch]);

  const filteredTargetFields = useMemo(() => {
    const q = targetSearch.trim().toLowerCase();
    if (!q) return targetFields;
    return targetFields.filter((t) => t.name.toLowerCase().includes(q));
  }, [targetFields, targetSearch]);

  const selectedSource = selectedSourceId ? sourceFields.find((s) => s.id === selectedSourceId) ?? null : null;
  const selectedTargetsForSource = selectedSourceId ? sourceToTargets[selectedSourceId] ?? [] : [];
  // selectedTargetId is used to drive Unassign button enablement and selection highlighting.

  // Render current page body
  let body: React.ReactNode = null;
  if (page === 'platform') {
    body = (
      <div onKeyDown={platformKeyDown} tabIndex={0}>
        <div className="platform-grid" role="group" aria-label="Platform selection">
          <button
            id="platform-windows"
            className="platform-card"
            onClick={() => {
              setInstallMode('windows');
              goTo('welcome');
            }}
          >
            <div className="platform-card-title">Windows</div>
            <p className="platform-card-body">Install CADalytix directly on Windows.</p>
          </button>
          <button
            id="platform-docker"
            className="platform-card"
            onClick={() => {
              setInstallMode('docker');
              goTo('welcome');
            }}
          >
            <div className="platform-card-title">Docker / Linux</div>
            <p className="platform-card-body">Install using Docker (Linux servers, Linux desktops, or Docker hosts).</p>
          </button>
        </div>
      </div>
    );
  } else if (page === 'welcome') {
    body = (
      <div>
        <p>This wizard will guide you through installing CADalytix.</p>
        <p>
          Mode: {installMode === 'windows' ? 'Windows' : 'Docker / Linux'}
        </p>
      </div>
    );
  } else if (page === 'license') {
    body = (
      <div
        onKeyDown={(e) => {
          if (e.key === ' ') {
            e.preventDefault();
            setLicenseAccepted((v) => !v);
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
            onChange={(e) => setLicenseAccepted(e.target.checked)}
          />
          <label htmlFor="licenseAccept" className="wizard-label" style={{ margin: 0 }}>
            I accept the terms of the license agreement
          </label>
        </div>
      </div>
    );
  } else if (page === 'installType') {
    body = (
      <div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input
              type="radio"
              checked={installationType === 'typical'}
              onChange={() => setInstallationType('typical')}
            />
            Typical (Recommended)
          </label>
        </div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={installationType === 'custom'} onChange={() => setInstallationType('custom')} />
            Custom
          </label>
        </div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={installationType === 'import'} onChange={() => setInstallationType('import')} />
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
                    setImportConfigPath(e.target.value);
                    setImportConfigError(null);
                  }}
                />
                <button className="wizard-button" onClick={browseForFile}>
                  Browse…
                </button>
              </div>
              {importConfigError ? <div className="wizard-error">{importConfigError}</div> : null}
            </div>
          </div>
        ) : null}
      </div>
    );
  } else if (page === 'destination') {
    body = (
      <div>
        <div className="wizard-row">
          <label className="wizard-label">Install path</label>
          <div className="wizard-inline">
            <input className="wizard-input" value={destinationFolder} onChange={(e) => setDestinationFolder(e.target.value)} />
            <button className="wizard-button" onClick={browseForFolder}>
              Browse…
            </button>
          </div>
          <div className="wizard-help">Required space: ~2–5 GB</div>
          {destinationError ? <div className="wizard-error">{destinationError}</div> : null}
        </div>
      </div>
    );
  } else if (page === 'dataSource') {
    body = (
      <div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={dataSourceKind === 'local'} onChange={() => setDataSourceKind('local')} />
            Use this server/host (local environment)
          </label>
        </div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={dataSourceKind === 'remote'} onChange={() => setDataSourceKind('remote')} />
            Connect to an existing remote system/database
          </label>
        </div>
        <div style={{ marginTop: 10 }}>
          <div className="wizard-row">
            <label className="wizard-label">Host</label>
            <input
              className="wizard-input"
              value={callDataHost}
              onChange={(e) => setCallDataHost(e.target.value)}
              disabled={dataSourceKind === 'local'}
            />
          </div>
          <div className="wizard-row wizard-inline">
            <div style={{ flex: 1 }}>
              <label className="wizard-label">Port</label>
              <input
                className="wizard-input"
                value={callDataPort}
                onChange={(e) => setCallDataPort(e.target.value)}
                disabled={dataSourceKind === 'local'}
              />
            </div>
            <div style={{ flex: 2 }}>
              <label className="wizard-label">Database</label>
              <input className="wizard-input" value={callDataDbName} onChange={(e) => setCallDataDbName(e.target.value)} />
            </div>
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Username</label>
            <input className="wizard-input" value={callDataUser} onChange={(e) => setCallDataUser(e.target.value)} />
          </div>
          <div className="wizard-row">
            <label className="wizard-label">Password</label>
            <input
              className="wizard-input"
              type="password"
              value={callDataPassword}
              onChange={(e) => setCallDataPassword(e.target.value)}
            />
          </div>
        </div>
        <div className="wizard-row">
          <label className="wizard-label">Source object name</label>
          <input className="wizard-input" value={sourceObjectName} onChange={(e) => setSourceObjectName(e.target.value)} />
        </div>
        <div className="wizard-help">Keep simple; do not require user to understand internal architecture.</div>
      </div>
    );
  } else if (page === 'database') {
    body = (
      <div>
        <div className="wizard-row">
          Do you want CADalytix to create a NEW database, or use an EXISTING database?
        </div>

        <div className="platform-grid">
          <button
            type="button"
            className="platform-card"
            onClick={() => {
              setDbSetupMode('createNew');
              setDbTestStatus('idle');
              setDbTestMessage('');
            }}
          >
            <div className="platform-card-title">Create NEW CADalytix Database</div>
            <div className="wizard-help">Create a new database for this installation.</div>
          </button>
          <button
            type="button"
            className="platform-card"
            onClick={() => {
              setDbSetupMode('existing');
              setDbTestStatus('idle');
              setDbTestMessage('');
            }}
          >
            <div className="platform-card-title">Use EXISTING Database</div>
            <div className="wizard-help">Connect to an existing database you provide.</div>
          </button>
        </div>

        {dbSetupMode === 'createNew' ? (
          <div style={{ marginTop: 12 }}>
            {/* Phase 9: New database name field */}
            <div className="wizard-row">
              <label className="wizard-label">New Database Name</label>
              <input
                className="wizard-input"
                style={{ width: 280 }}
                value={newDbName}
                onChange={(e) => setNewDbName(e.target.value)}
                placeholder="CADalytix_Production"
              />
            </div>

            {/* Phase 9: Admin connection fields */}
            <div style={{ marginTop: 12, padding: 10, border: '1px solid #ccc', borderRadius: 4 }}>
              <div className="wizard-row"><strong>Database Server Admin Connection</strong></div>
              <div className="wizard-help">Provide credentials for an account with CREATE DATABASE privileges.</div>
              <div className="wizard-row">
                <label className="wizard-label">Host</label>
                <input className="wizard-input" style={{ width: 200 }} value={newDbAdminHost} onChange={(e) => setNewDbAdminHost(e.target.value)} placeholder="localhost" />
              </div>
              <div className="wizard-row">
                <label className="wizard-label">Port</label>
                <input className="wizard-input" style={{ width: 100 }} value={newDbAdminPort} onChange={(e) => setNewDbAdminPort(e.target.value)} placeholder="1433 or 5432" />
                <span className="wizard-help" style={{ marginLeft: 8 }}>1433 = SQL Server, 5432 = PostgreSQL</span>
              </div>
              <div className="wizard-row">
                <label className="wizard-label">Admin Username</label>
                <input className="wizard-input" style={{ width: 200 }} value={newDbAdminUser} onChange={(e) => setNewDbAdminUser(e.target.value)} placeholder="sa" />
              </div>
              <div className="wizard-row">
                <label className="wizard-label">Admin Password</label>
                <input className="wizard-input" style={{ width: 200 }} type="password" value={newDbAdminPassword} onChange={(e) => setNewDbAdminPassword(e.target.value)} />
              </div>
              <div className="wizard-row">
                <button
                  className="wizard-button"
                  type="button"
                  disabled={newDbPrivTestStatus === 'testing' || !newDbAdminHost.trim() || !newDbAdminUser.trim() || !newDbAdminPassword.trim()}
                  onClick={runCreateNewPrivilegeTest}
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
                <input type="radio" checked={newDbLocation === 'thisMachine'} onChange={() => setNewDbLocation('thisMachine')} />
                This machine (default location)
              </label>
            </div>

            <div className="wizard-row">
              <label className="wizard-inline">
                <input type="radio" checked={newDbLocation === 'specificPath'} onChange={() => setNewDbLocation('specificPath')} />
                Specific drive / path (advanced)
              </label>
            </div>

            {newDbLocation === 'specificPath' ? (
              <div className="wizard-row">
                <label className="wizard-label">Database path</label>
                <div className="wizard-inline">
                  <input
                    className="wizard-input"
                    value={newDbSpecificPath}
                    onChange={(e) => setNewDbSpecificPath(e.target.value)}
                  />
                  <button className="wizard-button" type="button" onClick={browseForNewDbPath}>
                    Browse…
                  </button>
                </div>
              </div>
            ) : null}

            <div className="wizard-row">
              <label className="wizard-label">Max DB size / storage allocation (GB)</label>
              <input
                className="wizard-input"
                style={{ width: 180 }}
                value={newDbMaxSizeGb}
                onChange={(e) => setNewDbMaxSizeGb(e.target.value)}
              />
            </div>

            {/* Phase 9: Engine-specific sizing fields */}
            {dbEngine === 'sqlserver' ? (
              <div style={{ marginTop: 12, padding: 10, border: '1px solid #ccc', borderRadius: 4 }}>
                <div className="wizard-row"><strong>SQL Server Sizing (optional)</strong></div>
                <div className="wizard-row">
                  <label className="wizard-label">Initial data file size (MB)</label>
                  <input className="wizard-input" style={{ width: 120 }} value={newDbInitialDataSizeMb} onChange={(e) => setNewDbInitialDataSizeMb(e.target.value)} />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">Initial log file size (MB)</label>
                  <input className="wizard-input" style={{ width: 120 }} value={newDbInitialLogSizeMb} onChange={(e) => setNewDbInitialLogSizeMb(e.target.value)} />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">Max data file size (MB, 0 = unlimited)</label>
                  <input className="wizard-input" style={{ width: 120 }} value={newDbMaxDataSizeMb} onChange={(e) => setNewDbMaxDataSizeMb(e.target.value)} />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">Max log file size (MB, 0 = unlimited)</label>
                  <input className="wizard-input" style={{ width: 120 }} value={newDbMaxLogSizeMb} onChange={(e) => setNewDbMaxLogSizeMb(e.target.value)} />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">Data filegrowth (MB, or negative for %)</label>
                  <input className="wizard-input" style={{ width: 120 }} value={newDbDataFilegrowth} onChange={(e) => setNewDbDataFilegrowth(e.target.value)} />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">Log filegrowth (MB, or negative for %)</label>
                  <input className="wizard-input" style={{ width: 120 }} value={newDbLogFilegrowth} onChange={(e) => setNewDbLogFilegrowth(e.target.value)} />
                </div>
                <div className="wizard-help">Leave at defaults for typical installations. Negative values indicate percentage growth (e.g., -10 = 10%).</div>
              </div>
            ) : null}

            {dbEngine === 'postgres' ? (
              <div style={{ marginTop: 12, padding: 10, border: '1px solid #ccc', borderRadius: 4 }}>
                <div className="wizard-row"><strong>PostgreSQL Options (optional)</strong></div>
                <div className="wizard-row">
                  <label className="wizard-label">Owner role (leave blank for current user)</label>
                  <input className="wizard-input" style={{ width: 200 }} value={newDbPgOwner} onChange={(e) => setNewDbPgOwner(e.target.value)} placeholder="e.g., cadalytix_admin" />
                </div>
                <div className="wizard-help">PostgreSQL does not support SQL Server-style sizing. The database grows with available disk space.</div>
              </div>
            ) : null}

            {dbCreateValidationError ? <div className="wizard-error">{dbCreateValidationError}</div> : null}

            <div className="wizard-help">
              Hot retention and archive policy are configured on the next pages.
            </div>
          </div>
        ) : null}

        {dbSetupMode === 'existing' ? (
          <div style={{ marginTop: 12 }}>
            <div className="wizard-row">
              <strong>Where is the existing database hosted? (No login required)</strong>
            </div>

            <div className="wizard-row">
              <select
                className="wizard-select"
                value={existingHostedWhere}
                onChange={(e) => setExistingHostedWhere(e.target.value as any)}
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
                <input type="radio" checked={dbUseConnString} onChange={() => setDbUseConnString(true)} />
                Connection string
              </label>
            </div>

            <div className="wizard-row">
              <label className="wizard-inline">
                <input type="radio" checked={!dbUseConnString} onChange={() => setDbUseConnString(false)} />
                Enter connection details (host/server, port, db name, username, password, TLS)
              </label>
            </div>

            <div className="wizard-help" style={{ marginTop: 6 }}>
              CADalytix does not ask you to log in to AWS/Azure/GCP and does not scan your cloud. You only provide a database endpoint (connection string or host/port/user/password) with explicit permissions.
            </div>

            {dbUseConnString ? (
              <div className="wizard-row" style={{ marginTop: 10 }}>
                <label className="wizard-label">Connection string</label>
                <input className="wizard-input" value={dbConnString} onChange={(e) => setDbConnString(e.target.value)} />
              </div>
            ) : (
              <div style={{ marginTop: 10 }}>
                <div className="wizard-row">
                  <label className="wizard-label">Host</label>
                  <input className="wizard-input" value={dbHost} onChange={(e) => setDbHost(e.target.value)} />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">Port</label>
                  <input
                    className="wizard-input"
                    value={dbPort}
                    onChange={(e) => {
                      dbPortTouchedRef.current = true;
                      setDbPort(e.target.value);
                    }}
                  />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">Database</label>
                  <input className="wizard-input" value={dbName} onChange={(e) => setDbName(e.target.value)} />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">Username</label>
                  <input className="wizard-input" value={dbUser} onChange={(e) => setDbUser(e.target.value)} />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">Password</label>
                  <input className="wizard-input" type="password" value={dbPassword} onChange={(e) => setDbPassword(e.target.value)} />
                </div>
                <div className="wizard-row">
                  <label className="wizard-label">TLS</label>
                  <select className="wizard-select" value={dbSslMode} onChange={(e) => setDbSslMode(e.target.value as any)}>
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
              <button className="wizard-button" disabled={dbTestStatus === 'testing' || !canRunDbTest} onClick={runDbTest}>
                Test Connection
              </button>
            </div>

            {dbTestStatus === 'success' ? <div className="wizard-help">{dbTestMessage || 'Connection successful.'}</div> : null}
            {dbTestStatus === 'fail' ? <div className="wizard-error">{dbTestMessage || 'Connection failed.'}</div> : null}
          </div>
        ) : null}
    </div>
  );
  } else if (page === 'storage') {
    body = (
      <div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={storageMode === 'defaults'} onChange={() => setStorageMode('defaults')} />
            Use defaults (Recommended)
          </label>
        </div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={storageMode === 'custom'} onChange={() => setStorageMode('custom')} />
            Customize storage
          </label>
        </div>

        {storageMode === 'custom' ? (
          <div style={{ marginTop: 10 }}>
            <div className="wizard-row">
              <label className="wizard-label">Storage location</label>
              <div className="wizard-row">
                <label className="wizard-inline">
                  <input type="radio" checked={storageLocation === 'system'} onChange={() => setStorageLocation('system')} />
                  Use system disk
                </label>
              </div>
              <div className="wizard-row">
                <label className="wizard-inline">
                  <input
                    type="radio"
                    checked={storageLocation === 'attached'}
                    onChange={() => setStorageLocation('attached')}
                  />
                  Use attached drive
                </label>
              </div>
              <div className="wizard-row">
                <label className="wizard-inline">
                  <input type="radio" checked={storageLocation === 'custom'} onChange={() => setStorageLocation('custom')} />
                  Use custom path
                </label>
              </div>
            </div>

            {storageLocation === 'custom' ? (
              <div className="wizard-row">
                <label className="wizard-label">Custom path</label>
                <input className="wizard-input" value={storageCustomPath} onChange={(e) => setStorageCustomPath(e.target.value)} />
              </div>
            ) : null}

            <div className="wizard-row">
              <label className="wizard-label">Storage policy</label>
              <div className="wizard-row">
                <label className="wizard-inline">
                  <input type="radio" checked={retentionPolicy === '18'} onChange={() => setRetentionPolicy('18')} />
                  Rolling 18 months (Recommended)
                </label>
              </div>
              <div className="wizard-row">
                <label className="wizard-inline">
                  <input type="radio" checked={retentionPolicy === '12'} onChange={() => setRetentionPolicy('12')} />
                  Rolling 12 months
                </label>
              </div>
              <div className="wizard-row wizard-inline">
                <label className="wizard-inline">
                  <input type="radio" checked={retentionPolicy === 'max'} onChange={() => setRetentionPolicy('max')} />
                  Max disk usage:
                </label>
                <input className="wizard-input" style={{ width: 120 }} value={maxDiskGb} onChange={(e) => setMaxDiskGb(e.target.value)} />
                <span className="wizard-help">GB</span>
              </div>
              <div className="wizard-row">
                <label className="wizard-inline">
                  <input type="radio" checked={retentionPolicy === 'keep'} onChange={() => setRetentionPolicy('keep')} />
                  Keep everything (Not recommended)
                </label>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    );
  } else if (page === 'retention') {
    body = (
      <div>
        <div className="wizard-row">Choose how long to keep hot data in the database.</div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={hotRetentionChoice === '12'} onChange={() => setHotRetentionChoice('12')} />
            12 months (Recommended)
          </label>
        </div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="radio" checked={hotRetentionChoice === '18'} onChange={() => setHotRetentionChoice('18')} />
            18 months (Recommended)
          </label>
        </div>
        <div className="wizard-row wizard-inline">
          <label className="wizard-inline">
            <input type="radio" checked={hotRetentionChoice === 'custom'} onChange={() => setHotRetentionChoice('custom')} />
            Custom:
          </label>
          <input
            className="wizard-input"
            style={{ width: 120 }}
            value={hotRetentionCustomMonths}
            onChange={(e) => setHotRetentionCustomMonths(e.target.value)}
            disabled={hotRetentionChoice !== 'custom'}
          />
          <span className="wizard-help">months</span>
        </div>
        {retentionValidationError ? <div className="wizard-error">{retentionValidationError}</div> : null}
      </div>
    );
  } else if (page === 'archive') {
    body = (
      <div>
        <div className="wizard-row">Configure cold storage (archive) settings.</div>

        <div className="wizard-row">
          <label className="wizard-label">Archive format</label>
          <div className="wizard-row">
            <label className="wizard-inline">
              <input type="radio" checked={archiveFormat === 'zip+ndjson'} onChange={() => setArchiveFormat('zip+ndjson')} />
              ZIP + NDJSON (Preferred)
            </label>
          </div>
          <div className="wizard-row">
            <label className="wizard-inline">
              <input type="radio" checked={archiveFormat === 'zip+csv'} onChange={() => setArchiveFormat('zip+csv')} />
              ZIP + CSV
            </label>
          </div>
        </div>

        <div className="wizard-row">
          <label className="wizard-label">Archive destination folder</label>
          <div className="wizard-row wizard-inline">
            <input className="wizard-input" value={archiveDestinationPath} onChange={(e) => setArchiveDestinationPath(e.target.value)} />
            <button className="wizard-button" type="button" onClick={browseForArchiveFolder}>
              Browse…
            </button>
          </div>
        </div>

        <div className="wizard-row">
          <label className="wizard-label">Max archive usage cap (GB)</label>
          <input className="wizard-input" style={{ width: 160 }} value={archiveMaxUsageGb} onChange={(e) => setArchiveMaxUsageGb(e.target.value)} />
        </div>

        <div className="wizard-row">
          <label className="wizard-label">Schedule (local server time)</label>
          <div className="wizard-row wizard-inline">
            <span className="wizard-help">Day</span>
            <input
              className="wizard-input"
              style={{ width: 90 }}
              value={archiveScheduleDayOfMonth}
              onChange={(e) => setArchiveScheduleDayOfMonth(e.target.value)}
            />
            <span className="wizard-help">at</span>
            <input
              className="wizard-input"
              style={{ width: 120 }}
              value={archiveScheduleTimeLocal}
              onChange={(e) => setArchiveScheduleTimeLocal(e.target.value)}
            />
            <span className="wizard-help">(HH:MM)</span>
          </div>
        </div>

        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="checkbox" checked={archiveCatchUpOnStartup} onChange={(e) => setArchiveCatchUpOnStartup(e.target.checked)} />
            If missed, run on next startup for eligible months
          </label>
        </div>

        {archiveValidationError ? <div className="wizard-error">{archiveValidationError}</div> : null}
      </div>
    );
  } else if (page === 'consent') {
    body = (
      <div>
        <div className="wizard-row">
          <label className="wizard-inline">
            <input type="checkbox" checked={consentToSync} onChange={(e) => setConsentToSync(e.target.checked)} />
            Allow CADalytix to receive install metadata + schema mapping to for support improvements
          </label>
        </div>
        <div className="wizard-row">
          <button className="wizard-button" type="button" onClick={() => setConsentDetailsExpanded((v) => !v)}>
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
  } else if (page === 'mapping') {
    body = (
      <div>
        <div className="wizard-row">
          We found {sourceFields.length} fields in your source export.
          {mappingScanning ? ' (Scanning...)' : ''}
        </div>
        <div className="wizard-row wizard-inline">
          <label className="wizard-inline">
            <input type="checkbox" checked={mappingOverride} onChange={(e) => setMappingOverride(e.target.checked)} />
            Override: Allow a source field to map to multiple targets
          </label>
        </div>
        <div className="wizard-row wizard-inline">
          <label className="wizard-inline">
            <input type="checkbox" checked={mappingDemoMode} onChange={(e) => setMappingDemoMode(e.target.checked)} />
            Demo mode: Use sample source headers (no database connection)
          </label>
        </div>
        {mappingScanError ? <div className="wizard-error">{mappingScanError}</div> : null}

        <div className="mapping-layout" style={{ marginTop: 10 }}>
          <div className="mapping-pane">
            <div className="mapping-pane-header">Source Fields</div>
            <div className="mapping-pane-search">
              <input
                className="wizard-input"
                placeholder="Search source fields…"
                value={sourceSearch}
                onChange={(e) => setSourceSearch(e.target.value)}
              />
            </div>
            <div className="mapping-list" role="listbox" aria-label="Source fields">
              {filteredSourceFields.map((s) => (
                <div
                  key={s.id}
                  className={[
                    'mapping-row',
                    selectedSourceId === s.id ? 'selected' : '',
                    (sourceToTargets[s.id] ?? []).length > 0 ? 'mapped' : '',
                  ].join(' ')}
                  onClick={() => {
                    setSelectedSourceId(s.id);
                    setSelectedTargetId(null);
                  }}
                >
                  {s.displayName}
                </div>
              ))}
            </div>
          </div>

          <div className="mapping-pane">
            <div className="mapping-pane-header">Target Fields</div>
            <div className="mapping-pane-search">
              <input
                className="wizard-input"
                placeholder="Search target fields…"
                value={targetSearch}
                onChange={(e) => setTargetSearch(e.target.value)}
              />
            </div>
            <div className="mapping-list" role="listbox" aria-label="Target fields">
              {filteredTargetFields.map((t) => {
                const mappedSource = targetToSource[t.id];
                const isSelected = selectedTargetId === t.id;
                const isMapped = !!mappedSource;
                const highlight =
                  selectedSourceId && (sourceToTargets[selectedSourceId] ?? []).includes(t.id);
                return (
                  <div
                    key={t.id}
                    className={[
                      'mapping-row',
                      isSelected || highlight ? 'selected' : '',
                      isMapped ? 'mapped' : '',
                    ].join(' ')}
                    onClick={() => {
                      setSelectedTargetId(t.id);
                      if (selectedSourceId) {
                        attemptMap(selectedSourceId, t.id);
                      }
                    }}
                  >
                    {t.name}
                    {t.required ? ' *' : ''}
                    {mappedSource ? ` — mapped to ${sourceFields.find((s) => s.id === mappedSource)?.displayName ?? mappedSource}` : ''}
                  </div>
                );
              })}
            </div>
          </div>
        </div>

        <div className="mapping-preview">
          <div>
            Select a source field, then select a target field.
          </div>
          <div style={{ marginTop: 8 }}>
            <div>Source: [{selectedSource?.displayName ?? ''}]</div>
            <div>↓</div>
            <div>
              Target(s): [{selectedTargetsForSource.map((id) => targetFields.find((t) => t.id === id)?.name ?? id).join(', ')}]
            </div>
          </div>
          <div style={{ marginTop: 10 }} className="wizard-inline">
            <button className="wizard-button" disabled={!selectedSourceId || !selectedTargetId} onClick={unassignSelected}>
              Unassign
            </button>
            <div className="wizard-help">
              Mapped: {mappedCount} / Target fields: {targetFields.length} — Unassigned source fields: {sourceFields.filter((s) => (sourceToTargets[s.id] ?? []).length === 0).length}
            </div>
          </div>
          {requiredTargetsUnmapped.length > 0 ? (
            <div className="wizard-error" style={{ marginTop: 10 }}>
              Required fields not mapped: {requiredTargetsUnmapped.map((t) => t.name).join(', ')}
            </div>
          ) : null}
        </div>
      </div>
    );
  } else if (page === 'ready') {
    body = (
      <div>
        <div className="wizard-row">
          <div style={{ border: '1px solid #bcbcbc', background: '#f8f8f8', padding: 12 }}>
            <div><strong>Mode:</strong> {installMode === 'windows' ? 'Windows' : 'Docker / Linux'}</div>
            <div><strong>Install path:</strong> {destinationFolder}</div>
            <div>
              <strong>Database setup:</strong>{' '}
              {dbSetupMode === 'createNew'
                ? `Create NEW CADalytix Database — Max ${newDbMaxSizeGb} GB${
                    newDbLocation === 'specificPath'
                      ? ` — Path ${newDbSpecificPath || '(not set)'}`
                      : ' — This machine (default location)'
                  }`
                : `Use EXISTING Database — ${
                    existingHostedWhere === 'on_prem'
                      ? 'On-prem / self-hosted / unknown'
                      : existingHostedWhere === 'aws_rds'
                        ? 'AWS RDS / Aurora'
                        : existingHostedWhere === 'azure_sql'
                          ? 'Azure SQL / SQL MI'
                          : existingHostedWhere === 'gcp_cloud_sql'
                            ? 'GCP Cloud SQL'
                            : existingHostedWhere === 'neon'
                              ? 'Neon'
                              : existingHostedWhere === 'supabase'
                                ? 'Supabase'
                                : 'Other'
                  } (password hidden)`}
            </div>
            <div><strong>Storage policy:</strong> {storageMode === 'defaults' ? 'Defaults' : 'Custom'} — {retentionPolicy === '18' ? 'Rolling 18 months' : retentionPolicy === '12' ? 'Rolling 12 months' : retentionPolicy === 'max' ? `Max disk ${maxDiskGb} GB` : 'Keep everything'}</div>
            <div><strong>Hot retention:</strong> {hotRetentionMonths} months</div>
            <div>
              <strong>Archive policy:</strong> {archiveFormat === 'zip+ndjson' ? 'ZIP + NDJSON' : 'ZIP + CSV'} — {archiveDestinationPath || '(not set)'} — Cap {archiveMaxUsageGb} GB — Day {archiveScheduleDayOfMonth} at {archiveScheduleTimeLocal} — Catch-up {archiveCatchUpOnStartup ? 'Yes' : 'No'}
            </div>
            <div><strong>Consent to Sync:</strong> {consentToSync ? 'Yes' : 'No'}</div>
            <div><strong>Mapping:</strong> {mappedCount} mapped — required mapped: {requiredTargetsUnmapped.length === 0 ? 'Yes' : 'No'}</div>
          </div>
        </div>
        <div className="wizard-help">Passwords are not shown.</div>
      </div>
    );
  } else if (page === 'installing') {
    const elapsedMs = progress?.elapsedMs;
    const etaMs = progress?.etaMs;
    const fmt = (ms?: number) => {
      if (!ms || ms <= 0) return '';
      const total = Math.floor(ms / 1000);
      const m = Math.floor(total / 60);
      const s = total % 60;
      return `${m}:${String(s).padStart(2, '0')}`;
    };

    body = (
      <div>
        <div className="wizard-row">Progress</div>
        <progress className="progress-bar" value={progress?.percent ?? 0} max={100} />
        <div className="wizard-row">Current action: {progress?.message ?? ''}</div>
        <div className="wizard-row wizard-inline">
          {elapsedMs ? <div className="wizard-help">Elapsed: {fmt(elapsedMs)}</div> : null}
          {etaMs ? <div className="wizard-help">Estimated remaining: {fmt(etaMs)}</div> : null}
        </div>
        {installDetailLines.length > 0 ? (
          <div className="install-detail-log" aria-label="Installation details">
            {installDetailLines.map((l, idx) => (
              <div key={`${idx}-${l}`}>{l}</div>
            ))}
          </div>
        ) : null}
        {installError ? <div className="wizard-error">{installError}</div> : null}
      </div>
    );
  } else if (page === 'complete') {
    body = (
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

  return (
    <>
      <WizardFrame
        title={wizardTitle}
        subtitle={page === 'welcome' ? `Mode: ${installMode === 'windows' ? 'Windows' : 'Docker / Linux'}` : undefined}
        backDisabled={backDisabled}
        nextDisabled={nextDisabled}
        nextLabel={nextLabel}
        cancelDisabled={cancelDisabled}
        onBack={onBack}
        onNext={onNext}
        onCancel={openCancelConfirm}
      >
        {body}
      </WizardFrame>
      <Modal state={modal} />
    </>
  );
}

