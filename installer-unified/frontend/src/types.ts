/**
 * Shared types for the CADalytix Installer Wizard.
 */

/** Installation mode: Windows native or Docker/Linux */
export type InstallMode = 'windows' | 'docker';

/** Wizard page identifiers */
export type WizardPage =
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

/** Installation type selection */
export type InstallationType = 'typical' | 'custom' | 'import';

/** Database setup mode */
export type DbSetupMode = 'createNew' | 'existing' | null;

/** Database hosting location */
export type DbHostedWhere = 'on_prem' | 'aws_rds' | 'azure_sql' | 'gcp_cloud_sql' | 'neon' | 'supabase' | 'other';

/** Database engine type */
export type DbEngine = 'sqlserver' | 'postgres' | null;

/** SSL/TLS mode for database connections */
export type DbSslMode = 'disable' | 'prefer' | 'require';

/** Test connection status */
export type TestStatus = 'idle' | 'testing' | 'success' | 'fail';

/** Storage mode */
export type StorageMode = 'defaults' | 'custom';

/** Storage location */
export type StorageLocation = 'system' | 'attached' | 'custom';

/** Retention policy */
export type RetentionPolicy = '18' | '12' | 'max' | 'keep';

/** Hot retention choice */
export type HotRetentionChoice = '12' | '18' | 'custom';

/** Archive format */
export type ArchiveFormat = 'zip+ndjson' | 'zip+csv';

/** Data source kind */
export type DataSourceKind = 'local' | 'remote';

/** New database location */
export type NewDbLocation = 'thisMachine' | 'specificPath';

/** Source field from schema scan */
export interface SourceField {
  id: string;
  rawName: string;
  displayName: string;
}

/** Target field for mapping */
export interface TargetField {
  id: string;
  name: string;
  required: boolean;
}

