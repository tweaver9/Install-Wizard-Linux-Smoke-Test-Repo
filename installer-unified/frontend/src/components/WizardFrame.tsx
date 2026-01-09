/**
 * WizardFrame - Main wizard container with header, content area, and footer buttons
 * 
 * Features:
 * - Platform theming via data-platform attribute
 * - Optional step indicator
 * - Back/Next/Cancel buttons with configurable labels and states
 */
import StepIndicator from './StepIndicator';
import './WizardFrame.css';

export interface WizardFrameProps {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
  
  // Navigation
  backDisabled: boolean;
  nextDisabled: boolean;
  nextLabel: string;
  cancelDisabled?: boolean;
  onBack: () => void;
  onNext: () => void;
  onCancel: () => void;
  
  // Step indicator (optional - only show when provided)
  currentStep?: number;
  totalSteps?: number;
  stepNames?: string[];
  
  // Platform theming
  platform?: 'windows' | 'docker';
}

export default function WizardFrame(props: WizardFrameProps) {
  const showStepIndicator = props.currentStep !== undefined && props.totalSteps !== undefined && props.totalSteps > 0;

  return (
    <div className="wizard-root" data-platform={props.platform || 'windows'}>
      <div className="wizard-window">
        <div className="wizard-header">
          <h2 className="wizard-title">{props.title}</h2>
          {props.subtitle && <p className="wizard-subtitle">{props.subtitle}</p>}
          {showStepIndicator && (
            <StepIndicator
              currentStep={props.currentStep!}
              totalSteps={props.totalSteps!}
              stepNames={props.stepNames}
            />
          )}
        </div>
        <div className="wizard-content">{props.children}</div>
        <div className="wizard-footer">
          <button
            className="wizard-button"
            disabled={props.backDisabled}
            onClick={props.onBack}
          >
            Back
          </button>
          <button
            className="wizard-button primary"
            disabled={props.nextDisabled}
            onClick={props.onNext}
          >
            {props.nextLabel}
          </button>
          <button
            className="wizard-button"
            disabled={props.cancelDisabled}
            onClick={props.onCancel}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

