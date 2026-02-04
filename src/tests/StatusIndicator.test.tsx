/**
 * Tests for StatusIndicator component.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusIndicator } from '../components/StatusIndicator';
import type { AppState } from '../types';

describe('StatusIndicator', () => {
  it('renders idle state correctly', () => {
    render(<StatusIndicator state="idle" enabled={true} />);
    expect(screen.getByText('Ready')).toBeDefined();
  });

  it('renders recording state with animation', () => {
    const { container } = render(<StatusIndicator state="recording" enabled={true} />);
    expect(screen.getByText('Recording...')).toBeDefined();
    // Check for pulsing animation class
    const dot = container.querySelector('.animate-pulse');
    expect(dot).toBeDefined();
  });

  it('renders transcribing state', () => {
    render(<StatusIndicator state="transcribing" enabled={true} />);
    expect(screen.getByText('Transcribing...')).toBeDefined();
  });

  it('renders loading_model state with progress', () => {
    render(
      <StatusIndicator
        state="loading_model"
        enabled={true}
        progress={{ current: 50, total: 100 }}
      />
    );
    expect(screen.getByText('Loading model...')).toBeDefined();
    expect(screen.getByText('50%')).toBeDefined();
  });

  it('renders error state with detail', () => {
    render(
      <StatusIndicator state="error" enabled={true} detail="Model failed to load" />
    );
    expect(screen.getByText('Error')).toBeDefined();
    expect(screen.getByText('Model failed to load')).toBeDefined();
  });

  it('renders paused state when disabled', () => {
    render(<StatusIndicator state="idle" enabled={false} />);
    expect(screen.getByText('Paused')).toBeDefined();
  });

  it('does not animate when disabled', () => {
    const { container } = render(<StatusIndicator state="recording" enabled={false} />);
    // When disabled, should show "Paused" instead of "Recording..."
    expect(screen.getByText('Paused')).toBeDefined();
    // Should not have animation class
    const dot = container.querySelector('.animate-pulse');
    expect(dot).toBeNull();
  });

  it.each<[AppState, string]>([
    ['idle', 'Ready'],
    ['recording', 'Recording...'],
    ['transcribing', 'Transcribing...'],
    ['loading_model', 'Loading model...'],
    ['error', 'Error'],
  ])('renders %s state with label "%s"', (state, expectedLabel) => {
    render(<StatusIndicator state={state} enabled={true} />);
    expect(screen.getByText(expectedLabel)).toBeDefined();
  });

  it('renders progress bar at correct percentage', () => {
    const { container } = render(
      <StatusIndicator
        state="loading_model"
        enabled={true}
        progress={{ current: 75, total: 100 }}
      />
    );
    const progressBar = container.querySelector('[style*="width"]');
    expect(progressBar?.getAttribute('style')).toContain('75%');
  });

  it('handles progress without total gracefully', () => {
    // Should not show progress bar if total is undefined
    const { container } = render(
      <StatusIndicator
        state="loading_model"
        enabled={true}
        progress={{ current: 50 }}
      />
    );
    expect(screen.getByText('Loading model...')).toBeDefined();
    // Progress bar should not be visible without total
    const progressBar = container.querySelector('[style*="width"]');
    expect(progressBar).toBeNull();
  });
});
