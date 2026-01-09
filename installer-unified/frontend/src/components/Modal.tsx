/**
 * Modal - Generic modal dialog component
 * 
 * Features:
 * - Overlay background
 * - Primary/Secondary/Tertiary action buttons
 * - Accessible role="dialog" aria-modal
 */
import './Modal.css';

export interface ModalState {
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

export function emptyModal(): ModalState {
  return { kind: 'none' };
}

interface ModalProps {
  state: ModalState;
}

export default function Modal({ state }: ModalProps) {
  if (state.kind === 'none') return null;

  const primary = state.primaryLabel ?? 'OK';
  const secondary = state.secondaryLabel ?? 'Cancel';

  return (
    <div className="modal-overlay" role="dialog" aria-modal="true">
      <div className="modal">
        <div className="modal-header">{state.title ?? ''}</div>
        <div className="modal-body">{state.body ?? ''}</div>
        <div className="modal-footer">
          {state.onTertiary && state.tertiaryLabel && (
            <button className="wizard-button" onClick={state.onTertiary}>
              {state.tertiaryLabel}
            </button>
          )}
          {state.onSecondary && (
            <button className="wizard-button" onClick={state.onSecondary}>
              {secondary}
            </button>
          )}
          {state.onPrimary && (
            <button className="wizard-button primary" onClick={state.onPrimary}>
              {primary}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

