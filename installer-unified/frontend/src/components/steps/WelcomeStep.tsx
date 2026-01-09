import type { InstallMode } from '../../types';

export interface WelcomeStepProps {
  installMode: InstallMode;
}

export function WelcomeStep({ installMode }: WelcomeStepProps) {
  return (
    <div>
      <p>This wizard will guide you through installing CADalytix.</p>
      <p>Mode: {installMode === 'windows' ? 'Windows' : 'Docker / Linux'}</p>
    </div>
  );
}

