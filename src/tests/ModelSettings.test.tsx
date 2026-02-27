/**
 * Tests for ModelSettings component.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { ModelSettings } from '../components/Settings/ModelSettings';
import type { ModelStatus } from '../types';

describe('ModelSettings', () => {
  it('shows loading state when status is null', () => {
    render(
      <ModelSettings
        status={null}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );
    expect(screen.getByText('Loading model status...')).toBeDefined();
  });

  it('shows model ID', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'ready',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );
    expect(screen.getByText('parakeet-tdt-0.6b-v3')).toBeDefined();
  });

  it('shows ready status', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'ready',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );
    expect(screen.getByText('Ready')).toBeDefined();
    expect(screen.getByText('Model is ready for transcription.')).toBeDefined();
  });

  it('shows missing status with install button', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'missing',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );
    expect(screen.getByText('Available')).toBeDefined();
    expect(screen.getByText('Install Model')).toBeDefined();
    expect(screen.getByText(/available but not installed/)).toBeDefined();
  });

  it('calls onDownload when download button clicked', async () => {
    const onDownload = vi.fn().mockResolvedValue(undefined);
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'missing',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={onDownload}
        onPurgeCache={vi.fn()}
      />
    );

    await act(async () => {
      fireEvent.click(screen.getByText('Install Model'));
    });

    expect(onDownload).toHaveBeenCalled();
  });

  it('shows downloading status with progress', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'downloading',
      progress: { current: 512 * 1024 * 1024, total: 2.5 * 1024 * 1024 * 1024, unit: 'bytes' },
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );
    // Text appears in both status label and disabled button
    expect(screen.getAllByText('Installing...')).toHaveLength(2);
    // formatBytes uses parseFloat which removes trailing zeros (512.0 -> 512)
    expect(screen.getByText(/512 MB/)).toBeDefined();
  });

  it('shows verifying status', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'verifying',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );
    // Text appears in both status label and disabled button
    expect(screen.getAllByText('Installing...')).toHaveLength(2);
  });

  it('shows error status with retry button', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'error',
      error: 'Download failed: network error',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );
    expect(screen.getByText('Error')).toBeDefined();
    expect(screen.getByText('Download failed: network error')).toBeDefined();
    expect(screen.getByText('Retry Install')).toBeDefined();
  });

  it('shows unknown status safely', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'unknown',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );

    expect(screen.getByText('Available')).toBeDefined();
  });

  it('shows purge cache button when ready', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'ready',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );
    expect(screen.getByText('Purge Cache')).toBeDefined();
  });

  it('shows confirmation before purging', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'ready',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );

    fireEvent.click(screen.getByText('Purge Cache'));

    expect(screen.getByText('Delete model and redownload?')).toBeDefined();
    expect(screen.getByText('Yes, Delete')).toBeDefined();
    expect(screen.getByText('Cancel')).toBeDefined();
  });

  it('calls onPurgeCache when confirmed', async () => {
    const onPurgeCache = vi.fn().mockResolvedValue(undefined);
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'ready',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={onPurgeCache}
      />
    );

    fireEvent.click(screen.getByText('Purge Cache'));

    await act(async () => {
      fireEvent.click(screen.getByText('Yes, Delete'));
    });

    expect(onPurgeCache).toHaveBeenCalledWith('parakeet-tdt-0.6b-v3');
  });

  it('cancels purge confirmation', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'ready',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );

    fireEvent.click(screen.getByText('Purge Cache'));
    fireEvent.click(screen.getByText('Cancel'));

    // Should go back to showing Purge Cache button
    expect(screen.getByText('Purge Cache')).toBeDefined();
    expect(screen.queryByText('Yes, Delete')).toBeNull();
  });

  it('shows error when download fails', async () => {
    const onDownload = vi.fn().mockRejectedValue(new Error('Network error'));
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'missing',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={onDownload}
        onPurgeCache={vi.fn()}
      />
    );

    await act(async () => {
      fireEvent.click(screen.getByText('Install Model'));
    });

    expect(screen.getByText('Network error')).toBeDefined();
  });

  it('disables buttons when loading', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'missing',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
        isLoading={true}
      />
    );

    expect(screen.getByText('Install Model')).toHaveProperty('disabled', true);
  });

  it('does not show download button when downloading', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'downloading',
      progress: { current: 100, total: 1000, unit: 'bytes' },
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );

    expect(screen.queryByText('Install Model')).toBeNull();
  });

  it('does not show purge button when not ready', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'missing',
    };
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
      />
    );

    expect(screen.queryByText('Purge Cache')).toBeNull();
  });

  // ── Install button state matrix ──────────────────────────────────

  it('shows "Install Model" for missing state, not "Retry"', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'missing',
    };
    render(
      <ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />
    );
    expect(screen.getByText('Install Model')).toBeDefined();
    expect(screen.queryByText('Retry Install')).toBeNull();
  });

  it('shows "Retry Install" for error state, not "Install Model"', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'error',
      error: 'Network timeout',
    };
    render(
      <ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />
    );
    expect(screen.getByText('Retry Install')).toBeDefined();
    expect(screen.queryByText('Install Model')).toBeNull();
  });

  it('hides download button during verifying state', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'verifying',
    };
    render(
      <ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />
    );
    expect(screen.queryByText('Install Model')).toBeNull();
    expect(screen.queryByText('Retry Install')).toBeNull();
  });

  // ── Progress display ──────────────────────────────────────────

  it('shows percentage in progress bar', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'downloading',
      progress: { current: 250, total: 1000, unit: 'bytes' },
    };
    render(
      <ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />
    );
    expect(screen.getByText('25%')).toBeDefined();
  });

  it('shows 0% progress when total is 0', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'downloading',
      progress: { current: 100, total: 0, unit: 'bytes' },
    };
    render(
      <ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />
    );
    expect(screen.getByText('0%')).toBeDefined();
  });

  it('does not show progress bar for downloading without progress data', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'downloading',
    };
    render(
      <ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />
    );
    // Installing label in status and button
    expect(screen.getAllByText('Installing...')).toHaveLength(2);
    // But no progress percentage
    expect(screen.queryByText('%')).toBeNull();
  });

  // ── Model info display ──────────────────────────────────────────

  it('shows model description text', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'ready',
    };
    render(
      <ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />
    );
    expect(screen.getByText(/NVIDIA Parakeet/)).toBeDefined();
  });

  it('renders heading "Speech Recognition Model"', () => {
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'ready',
    };
    render(
      <ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />
    );
    expect(screen.getByText('Speech Recognition Model')).toBeDefined();
  });

  // ── Error recovery ──────────────────────────────────────────

  it('shows error from purge failure', async () => {
    const onPurge = vi.fn().mockRejectedValue(new Error('Permission denied'));
    const status: ModelStatus = {
      model_id: 'parakeet-tdt-0.6b-v3',
      status: 'ready',
    };
    render(
      <ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={onPurge} />
    );

    fireEvent.click(screen.getByText('Purge Cache'));
    await act(async () => {
      fireEvent.click(screen.getByText('Yes, Delete'));
    });

    expect(screen.getByText('Permission denied')).toBeDefined();
  });

  // ── Loading state ──────────────────────────────────────────

  it('loading state shows spinner indicator', () => {
    render(
      <ModelSettings status={null} onDownload={vi.fn()} onPurgeCache={vi.fn()} />
    );
    // The spinner has animate-spin class
    const spinner = document.querySelector('.animate-spin');
    expect(spinner).not.toBeNull();
  });
});
