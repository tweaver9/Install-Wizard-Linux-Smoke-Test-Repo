export interface SourceField {
  id: string;
  rawName: string;
  displayName: string;
}

export interface TargetField {
  id: string;
  name: string;
  required: boolean;
}

export interface MappingStepProps {
  sourceFields: SourceField[];
  targetFields: TargetField[];
  sourceToTargets: Record<string, string[]>;
  targetToSource: Record<string, string>;
  mappingScanning: boolean;
  mappingScanError: string | null;
  mappingOverride: boolean;
  onMappingOverrideChange: (value: boolean) => void;
  mappingDemoMode: boolean;
  onMappingDemoModeChange: (value: boolean) => void;
  sourceSearch: string;
  onSourceSearchChange: (value: string) => void;
  targetSearch: string;
  onTargetSearchChange: (value: string) => void;
  filteredSourceFields: SourceField[];
  filteredTargetFields: TargetField[];
  selectedSourceId: string | null;
  onSelectedSourceIdChange: (id: string | null) => void;
  selectedTargetId: string | null;
  onSelectedTargetIdChange: (id: string | null) => void;
  selectedSource: SourceField | undefined;
  selectedTargetsForSource: string[];
  mappedCount: number;
  requiredTargetsUnmapped: TargetField[];
  onAttemptMap: (sourceId: string, targetId: string) => void;
  onUnassignSelected: () => void;
}

export function MappingStep({
  sourceFields,
  targetFields,
  sourceToTargets,
  targetToSource,
  mappingScanning,
  mappingScanError,
  mappingOverride,
  onMappingOverrideChange,
  mappingDemoMode,
  onMappingDemoModeChange,
  sourceSearch,
  onSourceSearchChange,
  targetSearch,
  onTargetSearchChange,
  filteredSourceFields,
  filteredTargetFields,
  selectedSourceId,
  onSelectedSourceIdChange,
  selectedTargetId,
  onSelectedTargetIdChange,
  selectedSource,
  selectedTargetsForSource,
  mappedCount,
  requiredTargetsUnmapped,
  onAttemptMap,
  onUnassignSelected,
}: MappingStepProps) {
  return (
    <div>
      <div className="wizard-row">
        We found {sourceFields.length} fields in your source export.
        {mappingScanning ? ' (Scanning...)' : ''}
      </div>
      <div className="wizard-row wizard-inline">
        <label className="wizard-inline">
          <input type="checkbox" checked={mappingOverride} onChange={(e) => onMappingOverrideChange(e.target.checked)} />
          Override: Allow a source field to map to multiple targets
        </label>
      </div>
      <div className="wizard-row wizard-inline">
        <label className="wizard-inline">
          <input type="checkbox" checked={mappingDemoMode} onChange={(e) => onMappingDemoModeChange(e.target.checked)} />
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
              onChange={(e) => onSourceSearchChange(e.target.value)}
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
                  onSelectedSourceIdChange(s.id);
                  onSelectedTargetIdChange(null);
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
              onChange={(e) => onTargetSearchChange(e.target.value)}
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
                    onSelectedTargetIdChange(t.id);
                    if (selectedSourceId) {
                      onAttemptMap(selectedSourceId, t.id);
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
          <button className="wizard-button" disabled={!selectedSourceId || !selectedTargetId} onClick={onUnassignSelected}>
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
}

