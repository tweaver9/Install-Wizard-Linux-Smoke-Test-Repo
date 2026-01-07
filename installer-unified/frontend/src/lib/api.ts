/**
 * API Client for CADalytix Installer
 * Modified for Tauri - uses window.__TAURI__.invoke() instead of webviewBridge
 * 
 * This file should be replaced when copying the actual React UI from ui/cadalytix-ui/
 * Only the communication layer (invoke/emit) needs to change, all API function signatures remain the same
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/**
 * Base API response type
 */
export interface ApiResponse<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  message?: string;
}

/**
 * Send a request to the Rust backend via Tauri invoke
 * Note: command names are the Rust function names (snake_case) from `#[tauri::command]`.
 * The payload is passed under a `payload` key to match the Rust command signature:
 *   #[tauri::command] async fn my_command(payload: Option<MyRequest>) { ... }
 */
async function sendRequest<T = any>(
  command: string,
  payload?: any
): Promise<ApiResponse<T>> {
  try {
    // Tauri invoke args must be an object keyed by the Rust parameter name.
    // We standardize on a single parameter named `payload`.
    const response = payload === undefined
      ? await invoke<ApiResponse<T>>(command)
      : await invoke<ApiResponse<T>>(command, { payload: payload ?? null });
    
    // If the response is already an ApiResponse, return it
    if (response && typeof response === 'object' && 'success' in response) {
      return response as ApiResponse<T>;
    }
    
    // Otherwise wrap it
    return {
      success: true,
      data: response as T,
    };
  } catch (error: any) {
    return {
      success: false,
      error: error?.message || String(error),
    };
  }
}

/**
 * Listen to events from the Rust backend
 */
export function listenToEvent<T = any>(
  eventName: string,
  callback: (payload: T) => void
): Promise<() => void> {
  return listen<T>(eventName, (event) => {
    callback(event.payload);
  });
}

// ============================================================================
// Setup API Endpoints
// ============================================================================

// Matches Rust: `src-tauri/src/models/requests.rs` and `src-tauri/src/models/responses.rs`.

export type AuthMode = 'External' | 'LocalInClient' | 'HostedByCadalytix';

export interface CallDataConfig {
  connectionString: string;
  sourceObjectName: string;
  sourceName?: string;
}

export interface ExternalAuthHeadersConfig {
  trustHeaders?: boolean;
  userHeader?: string;
  rolesHeader?: string;
}

export interface ConfigDbConfig {
  connectionString: string;
}

export interface InitRequest {
  configDbConnectionString: string;
  callDataConnectionString: string;
  sourceObjectName: string;
}

export interface InitResponse {
  success: boolean;
  message: string;
  installationId?: string | null;
  alreadyInitialized: boolean;
  coreMigrationsApplied: string[];
  errors: string[];
  correlationId?: string | null;
}

export async function initSetup(request: InitRequest): Promise<ApiResponse<InitResponse>> {
  return sendRequest<InitResponse>('init_setup', request);
}

export interface SetupPlanRequest {
  authMode: AuthMode;
  callData: CallDataConfig;
  externalAuthHeaders?: ExternalAuthHeadersConfig | null;
  configDb: ConfigDbConfig;
}

export interface SetupPlanResponse {
  authMode: AuthMode;
  actions: string[];
  instanceSettings: Record<string, string>;
  migrationsToApply: string[];
  warnings: string[];
}

export async function planSetup(request: SetupPlanRequest): Promise<ApiResponse<SetupPlanResponse>> {
  return sendRequest<SetupPlanResponse>('plan_setup', request);
}

export interface SetupApplyResponse {
  success: boolean;
  actionsPerformed: string[];
  migrationsApplied: string[];
  errors: string[];
  warnings: string[];
}

export async function applySetup(request: SetupPlanRequest): Promise<ApiResponse<SetupApplyResponse>> {
  return sendRequest<SetupApplyResponse>('apply_setup', request);
}

export interface CommitRequest {
  configDbConnectionString: string;
  callDataConnectionString: string;
  authMode: string;
  sourceName: string;
  sourceObjectName: string;
  mappings: Record<string, string>;
  authSettings: Record<string, string>;
  dashboardUrl?: string | null;
  initialIngestStartDate?: string | null;
  initialIngestEndDate?: string | null;
}

export interface CommitResponse {
  success: boolean;
  message: string;
  migrationsApplied: string[];
  actionsPerformed: string[];
  errors: string[];
  correlationId?: string | null;
}

export async function commitSetup(request: CommitRequest): Promise<ApiResponse<CommitResponse>> {
  return sendRequest<CommitResponse>('commit_setup', request);
}

export interface SetupVerifyRequest {
  configDbConnectionString?: string | null;
  expectedCommitted?: boolean | null;
  callDataConnectionString?: string | null;
  sourceObjectName?: string | null;
}

export interface SetupVerifyCheckResult {
  id: string;
  label: string;
  status: string;
  message: string;
  durationMs: number;
}

export interface SetupVerifyResponse {
  success: boolean;
  checks: SetupVerifyCheckResult[];
  errors: string[];
}

export async function verifySetup(request: SetupVerifyRequest): Promise<ApiResponse<SetupVerifyResponse>> {
  return sendRequest<SetupVerifyResponse>('verify_setup', request);
}

export interface AppliedMigrationDto {
  name: string;
  appliedAt: string;
}

export interface SetupStatusResponse {
  authMode?: AuthMode | null;
  appliedMigrations: AppliedMigrationDto[];
  sourceObjectName?: string | null;
  sourceName?: string | null;
  schemaMappingExists: boolean;
  mappingCompleteness: number;
  isConfigured: boolean;
  message?: string | null;
}

export async function getSetupStatus(): Promise<ApiResponse<SetupStatusResponse>> {
  return sendRequest<SetupStatusResponse>('get_setup_status');
}

export interface SetupCompletionStatusResponse {
  isComplete: boolean;
  dashboardUrl?: string | null;
  initialIngestStartDate?: string | null;
  initialIngestEndDate?: string | null;
  committedUtc?: string | null;
}

export async function getSetupCompletionStatus(): Promise<ApiResponse<SetupCompletionStatusResponse>> {
  return sendRequest<SetupCompletionStatusResponse>('get_setup_completion_status');
}

export interface CheckpointResponse {
  stepName: string;
  stateJson: string;
  updatedAt: string;
}

export async function getLatestCheckpoint(): Promise<ApiResponse<CheckpointResponse>> {
  return sendRequest<CheckpointResponse>('get_latest_checkpoint');
}

export interface CheckpointSaveRequest {
  stepName: string;
  stateJson: string;
}

export async function saveCheckpoint(request: CheckpointSaveRequest): Promise<ApiResponse<{ saved: boolean }>> {
  return sendRequest<{ saved: boolean }>('save_checkpoint', request);
}

export interface LicenseSummaryDto {
  mode: string;
  status: string;
  expiresAtUtc: string;
  lastVerifiedAtUtc: string;
  features: string[];
}

export interface SetupEventDto {
  eventType: string;
  description: string;
  actor?: string | null;
  occurredAt: string;
}

export interface SupportBundleResponse {
  appVersion: string;
  buildHash: string;
  generatedAtUtc: string;
  configFingerprints: Record<string, string>;
  appliedMigrations: AppliedMigrationDto[];
  environmentInfo: Record<string, any>;
  schemaColumnNames: string[];
  licenseSummary?: LicenseSummaryDto | null;
  recentEvents: SetupEventDto[];
  phiStatement: string;
}

export async function getSupportBundle(): Promise<ApiResponse<SupportBundleResponse>> {
  return sendRequest<SupportBundleResponse>('get_support_bundle');
}

// ============================================================================
// License API Endpoints
// ============================================================================

export interface LicenseVerifyRequest {
  mode?: string;
  licenseKey: string;
  offlineBundle?: string | null;
  opsApiBaseUrl?: string | null;
}

export interface LicenseEntitlementDto {
  licenseMode: string;
  expiresAtUtc?: string | null;
  graceUntilUtc?: string | null;
  features: string[];
  clientId?: string | null;
  lastVerifiedAtUtc: string;
}

export interface LicenseVerifyResponse {
  success: boolean;
  message: string;
  entitlement?: LicenseEntitlementDto | null;
  correlationId: string;
}

export async function verifyLicense(request: LicenseVerifyRequest): Promise<ApiResponse<LicenseVerifyResponse>> {
  return sendRequest<LicenseVerifyResponse>('verify_license', request);
}

export interface LicenseStatusResponse {
  isActive: boolean;
  entitlement?: LicenseEntitlementDto | null;
  message: string;
}

export async function getLicenseStatus(): Promise<ApiResponse<LicenseStatusResponse>> {
  return sendRequest<LicenseStatusResponse>('get_license_status');
}

// ============================================================================
// Preflight API Endpoints
// ============================================================================

export interface PreflightCheckDto {
  name: string;
  status: string;
  detail: string;
}

export interface PreflightHostRequestDto {
  strictMode: boolean;
}

export interface PreflightHostResponseDto {
  machineName: string;
  osDescription: string;
  isWindows: boolean;
  isWindowsServer: boolean;
  isDomainJoined: boolean;
  isIisHosting: boolean;
  isContainer: boolean;
  checks: PreflightCheckDto[];
  overallStatus: string;
}

export async function preflightHost(request: PreflightHostRequestDto): Promise<ApiResponse<PreflightHostResponseDto>> {
  return sendRequest<PreflightHostResponseDto>('preflight_host', request);
}

export interface PreflightPermissionsRequestDto {
  configDbConnectionString: string;
  callDataConnectionString: string;
  requireConfigDbDdl?: boolean;
  requireConfigDbDml?: boolean;
  requireCallDataRead?: boolean;
  sourceObjectName: string;
}

export interface PreflightPermissionsResponseDto {
  checks: PreflightCheckDto[];
  overallStatus: string;
  recommendedRemediation: string;
}

export async function preflightPermissions(
  request: PreflightPermissionsRequestDto
): Promise<ApiResponse<PreflightPermissionsResponseDto>> {
  return sendRequest<PreflightPermissionsResponseDto>('preflight_permissions', request);
}

export interface DiscoveredColumnDto {
  name: string;
  dataType: string;
  isNullable: boolean;
}

export interface SampleStatsDto {
  sampleCount: number;
  minCallReceivedAt?: string | null;
  maxCallReceivedAt?: string | null;
}

export interface PreflightDataSourceRequestDto {
  callDataConnectionString: string;
  sourceObjectName: string;
  dateFromIso?: string | null;
  dateToIso?: string | null;
  sampleLimit?: number;
  demoMode?: boolean;
}

export interface PreflightDataSourceResponseDto {
  checks: PreflightCheckDto[];
  overallStatus: string;
  discoveredColumns: DiscoveredColumnDto[];
  sampleStats: SampleStatsDto;
}

export async function preflightDataSource(
  request: PreflightDataSourceRequestDto
): Promise<ApiResponse<PreflightDataSourceResponseDto>> {
  return sendRequest<PreflightDataSourceResponseDto>('preflight_datasource', request);
}

// ============================================================================
// Schema API Endpoints
// ============================================================================

export interface VerifySchemaRequest {
  engine?: string;
  connectionString?: string | null;
}

export interface VerifySchemaResponse {
  isValid: boolean;
  summary: string;
  totalIssues: number;
  missingSchemas: string[];
  missingTables: string[];
  missingColumns: string[];
  missingIndexes: string[];
  typeMismatches: string[];
  nullabilityMismatches: string[];
}

export async function verifySchema(request: VerifySchemaRequest): Promise<ApiResponse<VerifySchemaResponse>> {
  return sendRequest<VerifySchemaResponse>('verify_schema', request);
}

export interface VerifyAllRequest {
  configDbConnectionString?: string | null;
  callDataConnectionString?: string | null;
  sourceObjectName?: string | null;
  engine?: string;
}

export interface VerifyAllResponse {
  success: boolean;
  summary: string;
  checks: SetupVerifyCheckResult[];
  schemaVerification?: VerifySchemaResponse | null;
  errors: string[];
}

export async function verifyAllSchemas(request: VerifyAllRequest): Promise<ApiResponse<VerifyAllResponse>> {
  return sendRequest<VerifyAllResponse>('verify_all_schemas', request);
}

// ============================================================================
// Progress events
// ============================================================================

export interface ProgressEvent {
  correlationId: string;
  step: string;
  severity: string;
  phase: string;
  percent: number;
  message?: string;
  elapsedMs?: number;
  etaMs?: number;
}

