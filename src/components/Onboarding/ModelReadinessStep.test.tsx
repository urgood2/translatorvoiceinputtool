/**
 * Tests for ModelReadinessStep onboarding component.
 *
 * Covers: status check on mount, download trigger, progress display,
 * auto-advance on ready, error handling and retry, per bead bdp.1.4.
 */

import { describe, test, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react';
import { ModelReadinessStep } from './ModelReadinessStep';
import { useAppStore } from '../../store/appStore';
import type { ModelStatus } from '../../types';

// ── Mock Tauri invoke ─────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

// ── Helpers ───────────────────────────────────────────────────────

function setModelState(status: ModelStatus['status'], extra: Partial<ModelStatus> = {}) {
  useAppStore.setState({
    modelStatus: { status, model_id: 'parakeet-tdt-0.6b-v3', ...extra },
  });
}

// ── Setup ─────────────────────────────────────────────────────────

beforeEach(() => {
  vi.clearAllMocks();
  useAppStore.setState({
    modelStatus: null,
    downloadProgress: null,
    refreshModelStatus: vi.fn().mockResolvedValue(undefined),
  });
});

// ── Tests ─────────────────────────────────────────────────────────

describe('ModelReadinessStep', () => {
  test('shows checking status message when model status is unknown', () => {
    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);
    expect(screen.getByText('Checking model status...')).toBeDefined();
  });

  test('calls refreshModelStatus on mount', () => {
    const refreshSpy = vi.fn();
    useAppStore.setState({ refreshModelStatus: refreshSpy });

    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);
    expect(refreshSpy).toHaveBeenCalled();
  });

  test('shows download button when model is missing', () => {
    setModelState('missing');
    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    expect(screen.getByText('Download Model')).toBeDefined();
    expect(screen.getByText(/needs to be downloaded/)).toBeDefined();
  });

  test('download button triggers downloadModel', async () => {
    setModelState('missing');
    const downloadSpy = vi.fn().mockResolvedValue(undefined);
    useAppStore.setState({ downloadModel: downloadSpy });

    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    await act(async () => {
      fireEvent.click(screen.getByText('Download Model'));
    });

    expect(downloadSpy).toHaveBeenCalled();
  });

  test('shows progress bar during download', () => {
    setModelState('downloading');
    useAppStore.setState({
      downloadProgress: { current: 50, total: 100, unit: 'bytes' },
    });

    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    expect(screen.getByText('Downloading model...')).toBeDefined();
    expect(screen.getByText('50%')).toBeDefined();
    expect(screen.getByRole('progressbar')).toBeDefined();
  });

  test('shows verifying message during verification', () => {
    setModelState('verifying');
    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    expect(screen.getByText('Verifying model integrity...')).toBeDefined();
  });

  test('calls onReady when model is ready', () => {
    setModelState('ready');
    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    expect(onReady).toHaveBeenCalled();
  });

  test('shows ready message when model is ready', () => {
    setModelState('ready');
    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    expect(screen.getByText(/Model is ready/)).toBeDefined();
  });

  test('shows error state with retry button', () => {
    setModelState('error', { error: 'Network timeout' });
    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    expect(screen.getByText('Network timeout')).toBeDefined();
    expect(screen.getByText('Retry Download')).toBeDefined();
  });

  test('retry button triggers download again', async () => {
    setModelState('error', { error: 'Failed' });
    const downloadSpy = vi.fn().mockResolvedValue(undefined);
    useAppStore.setState({ downloadModel: downloadSpy });

    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    await act(async () => {
      fireEvent.click(screen.getByText('Retry Download'));
    });

    expect(downloadSpy).toHaveBeenCalled();
  });

  test('shows error from failed download attempt', async () => {
    setModelState('missing');
    const downloadSpy = vi.fn().mockRejectedValue(new Error('Disk full'));
    useAppStore.setState({ downloadModel: downloadSpy });

    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    await act(async () => {
      fireEvent.click(screen.getByText('Download Model'));
    });

    await waitFor(() => {
      expect(screen.getByText('Disk full')).toBeDefined();
    });
  });

  test('auto-advances to next step when status changes to ready', async () => {
    const onReady = vi.fn();
    useAppStore.setState({ modelStatus: { status: 'downloading', model_id: 'test' } });
    const { rerender } = render(<ModelReadinessStep onReady={onReady} />);

    expect(onReady).not.toHaveBeenCalled();

    // Simulate model becoming ready
    act(() => {
      setModelState('ready');
    });
    rerender(<ModelReadinessStep onReady={onReady} />);

    expect(onReady).toHaveBeenCalled();
  });

  test('progress bar shows 0% when total is undefined', () => {
    setModelState('downloading');
    useAppStore.setState({
      downloadProgress: { current: 50, total: undefined, unit: 'bytes' },
    });

    const onReady = vi.fn();
    render(<ModelReadinessStep onReady={onReady} />);

    // Progress bar exists but percentage text should not show
    expect(screen.getByText('Downloading model...')).toBeDefined();
    expect(screen.queryByText('%')).toBeNull();
  });
});
