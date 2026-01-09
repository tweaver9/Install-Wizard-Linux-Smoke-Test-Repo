/**
 * StepIndicator - Progress indicator showing current step and total steps
 * 
 * Displays:
 * - Progress bar with fill based on current step
 * - "Step X of Y" text label
 */
import './StepIndicator.css';

interface StepIndicatorProps {
  currentStep: number;
  totalSteps: number;
  /** Optional: array of step names for accessibility */
  stepNames?: string[];
}

export default function StepIndicator({ currentStep, totalSteps, stepNames }: StepIndicatorProps) {
  const percent = totalSteps > 0 ? Math.round((currentStep / totalSteps) * 100) : 0;
  const currentStepName = stepNames?.[currentStep - 1];

  return (
    <div className="step-indicator" role="progressbar" aria-valuenow={currentStep} aria-valuemin={1} aria-valuemax={totalSteps}>
      <div className="step-indicator-bar">
        <div 
          className="step-indicator-fill" 
          style={{ width: `${percent}%` }}
          aria-hidden="true"
        />
      </div>
      <div className="step-indicator-label">
        <span className="step-indicator-text">
          Step {currentStep} of {totalSteps}
        </span>
        {currentStepName && (
          <span className="step-indicator-name" aria-label={`Current step: ${currentStepName}`}>
            {currentStepName}
          </span>
        )}
      </div>
    </div>
  );
}

