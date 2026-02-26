/**
 * Onboarding wizard shown to new users on first launch.
 *
 * Guides through: welcome, microphone setup, hotkey configuration,
 * model readiness, and completion.
 */

import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../store/appStore';
import { MicSetupStep } from './MicSetupStep';
import { HotkeySetupStep } from './HotkeySetupStep';
import { ModelReadinessStep } from './ModelReadinessStep';

const STEPS = ['Welcome', 'Microphone', 'Hotkey', 'Model', 'Complete'] as const;
type Step = (typeof STEPS)[number];

interface OnboardingWizardProps {
  onComplete: () => void;
}

export function OnboardingWizard({ onComplete }: OnboardingWizardProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const config = useAppStore((s) => s.config);

  const step: Step = STEPS[currentStep] ?? 'Welcome';
  const stepHandlesContinue = step === 'Microphone' || step === 'Model';

  const handleNext = useCallback(() => {
    if (currentStep < STEPS.length - 1) {
      setCurrentStep((s) => s + 1);
    }
  }, [currentStep]);

  const handleBack = useCallback(() => {
    if (currentStep > 0) {
      setCurrentStep((s) => s - 1);
    }
  }, [currentStep]);

  const handleSkip = useCallback(async () => {
    if (!config) return;
    const updated = {
      ...config,
      ui: { ...config.ui, onboarding_completed: true },
    };
    await invoke('update_config', { config: updated });
    useAppStore.setState({ config: updated });
    onComplete();
  }, [config, onComplete]);

  const handleFinish = useCallback(async () => {
    await handleSkip();
  }, [handleSkip]);

  return (
    <div className="min-h-screen flex flex-col items-center justify-center p-8">
      {/* Step indicator */}
      <div className="flex gap-2 mb-8" role="progressbar" aria-valuenow={currentStep + 1} aria-valuemin={1} aria-valuemax={STEPS.length}>
        {STEPS.map((_, i) => (
          <div
            key={i}
            className={`w-3 h-3 rounded-full transition-colors ${
              i <= currentStep ? 'bg-blue-500' : 'bg-gray-300 dark:bg-gray-600'
            }`}
            aria-label={`Step ${i + 1}${i === currentStep ? ' (current)' : i < currentStep ? ' (completed)' : ''}`}
          />
        ))}
      </div>

      {/* Step content */}
      <div className="w-full max-w-md text-center">
        {step === 'Welcome' && (
          <div>
            <h2 className="text-2xl font-bold mb-4">Welcome to Voice Input Tool</h2>
            <p className="text-gray-600 dark:text-gray-400 mb-8">
              Let&rsquo;s get you set up in a few quick steps.
            </p>
          </div>
        )}

        {step === 'Microphone' && (
          <MicSetupStep onReady={handleNext} />
        )}

        {step === 'Hotkey' && (
          <HotkeySetupStep onReady={handleNext} />
        )}

        {step === 'Model' && (
          <ModelReadinessStep onReady={handleNext} />
        )}

        {step === 'Complete' && (
          <div>
            <h2 className="text-2xl font-bold mb-4">All Set!</h2>
            <p className="text-gray-600 dark:text-gray-400 mb-8">
              You&rsquo;re ready to start using Voice Input Tool.
            </p>
          </div>
        )}

        {/* Navigation */}
        <div className="flex justify-between gap-4">
          <button
            type="button"
            onClick={handleSkip}
            className="px-4 py-2 text-sm text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
          >
            Skip
          </button>

          <div className="flex gap-2">
            {currentStep > 0 && (
              <button
                type="button"
                onClick={handleBack}
                className="px-4 py-2 text-sm rounded bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600"
              >
                Back
              </button>
            )}

            {step === 'Complete' ? (
              <button
                type="button"
                onClick={handleFinish}
                className="px-6 py-2 text-sm rounded bg-blue-600 text-white hover:bg-blue-700"
              >
                Get Started
              </button>
            ) : !stepHandlesContinue ? (
              <button
                type="button"
                onClick={handleNext}
                className="px-6 py-2 text-sm rounded bg-blue-600 text-white hover:bg-blue-700"
              >
                Next
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
