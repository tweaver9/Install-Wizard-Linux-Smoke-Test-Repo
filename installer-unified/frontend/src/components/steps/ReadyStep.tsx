import type { InstallMode } from '../../types';
import type { StorageMode, RetentionPolicy } from './StorageStep';
import type { ArchiveFormat } from './ArchiveStep';
import type { DbSetupMode, DbHostedWhere, NewDbLocation } from './DatabaseStep';

export interface ReadyStepProps {
  installMode: InstallMode;
  destinationFolder: string;
  dbSetupMode: DbSetupMode;
  newDbMaxSizeGb: string;
  newDbLocation: NewDbLocation;
  newDbSpecificPath: string;
  existingHostedWhere: DbHostedWhere;
  storageMode: StorageMode;
  retentionPolicy: RetentionPolicy;
  maxDiskGb: string;
  hotRetentionMonths: number;
  archiveFormat: ArchiveFormat;
  archiveDestinationPath: string;
  archiveMaxUsageGb: string;
  archiveScheduleDayOfMonth: string;
  archiveScheduleTimeLocal: string;
  archiveCatchUpOnStartup: boolean;
  consentToSync: boolean;
  mappedCount: number;
  requiredTargetsUnmappedLength: number;
}

export function ReadyStep({
  installMode,
  destinationFolder,
  dbSetupMode,
  newDbMaxSizeGb,
  newDbLocation,
  newDbSpecificPath,
  existingHostedWhere,
  storageMode,
  retentionPolicy,
  maxDiskGb,
  hotRetentionMonths,
  archiveFormat,
  archiveDestinationPath,
  archiveMaxUsageGb,
  archiveScheduleDayOfMonth,
  archiveScheduleTimeLocal,
  archiveCatchUpOnStartup,
  consentToSync,
  mappedCount,
  requiredTargetsUnmappedLength,
}: ReadyStepProps) {
  const hostedWhereLabel = () => {
    switch (existingHostedWhere) {
      case 'on_prem': return 'On-prem / self-hosted / unknown';
      case 'aws_rds': return 'AWS RDS / Aurora';
      case 'azure_sql': return 'Azure SQL / SQL MI';
      case 'gcp_cloud_sql': return 'GCP Cloud SQL';
      case 'neon': return 'Neon';
      case 'supabase': return 'Supabase';
      default: return 'Other';
    }
  };

  return (
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
              : `Use EXISTING Database — ${hostedWhereLabel()} (password hidden)`}
          </div>
          <div><strong>Storage policy:</strong> {storageMode === 'defaults' ? 'Defaults' : 'Custom'} — {retentionPolicy === '18' ? 'Rolling 18 months' : retentionPolicy === '12' ? 'Rolling 12 months' : retentionPolicy === 'max' ? `Max disk ${maxDiskGb} GB` : 'Keep everything'}</div>
          <div><strong>Hot retention:</strong> {hotRetentionMonths} months</div>
          <div>
            <strong>Archive policy:</strong> {archiveFormat === 'zip+ndjson' ? 'ZIP + NDJSON' : 'ZIP + CSV'} — {archiveDestinationPath || '(not set)'} — Cap {archiveMaxUsageGb} GB — Day {archiveScheduleDayOfMonth} at {archiveScheduleTimeLocal} — Catch-up {archiveCatchUpOnStartup ? 'Yes' : 'No'}
          </div>
          <div><strong>Consent to Sync:</strong> {consentToSync ? 'Yes' : 'No'}</div>
          <div><strong>Mapping:</strong> {mappedCount} mapped — required mapped: {requiredTargetsUnmappedLength === 0 ? 'Yes' : 'No'}</div>
        </div>
      </div>
      <div className="wizard-help">Passwords are not shown.</div>
    </div>
  );
}

