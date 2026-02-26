import { describe, expect, test, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { ModelSettings } from '../ModelSettings';
import type { ModelStatus } from '../../../types';

function makeStatus(overrides: Partial<ModelStatus>): ModelStatus {
  return {
    model_id: 'nvidia/parakeet-tdt-0.6b-v3',
    status: 'ready',
    ...overrides,
  };
}

describe('ModelSettings', () => {
  test('shows selected model id', () => {
    const status = makeStatus({ model_id: 'openai/whisper-large-v3', status: 'ready' });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    expect(screen.getByText('openai/whisper-large-v3')).toBeDefined();
  });

  test('does not show language dropdown by default', () => {
    const status = makeStatus({ status: 'ready' });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    expect(screen.queryByRole('combobox', { name: /language/i })).toBeNull();
    expect(screen.queryByLabelText(/language/i)).toBeNull();
  });

  test('shows download button for missing state', () => {
    const status = makeStatus({ status: 'missing' });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    const downloadButton = screen.getByRole('button', { name: 'Download Model' });
    expect(downloadButton).toBeDefined();
    expect(downloadButton).toHaveProperty('disabled', false);
  });

  test('shows retry button for error state', () => {
    const status = makeStatus({ status: 'error', error: 'network timeout' });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    expect(screen.getByRole('button', { name: 'Retry Download' })).toBeDefined();
    expect(screen.queryByRole('button', { name: 'Download Model' })).toBeNull();
  });

  test('shows in-progress install state when downloading', () => {
    const status = makeStatus({
      status: 'downloading',
      progress: { current: 64, total: 128, unit: 'bytes' },
    });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    expect(screen.getByRole('button', { name: 'Downloading...' })).toHaveProperty('disabled', true);
    expect(screen.queryByRole('button', { name: 'Download Model' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Retry Download' })).toBeNull();
  });

  test('shows temporary install button state while download action is starting', async () => {
    let resolveDownload: (() => void) | null = null;
    const onDownload = vi.fn().mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveDownload = resolve;
        }),
    );

    const status = makeStatus({ status: 'missing' });
    render(<ModelSettings status={status} onDownload={onDownload} onPurgeCache={vi.fn()} />);

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Download Model' }));
    });

    expect(screen.getByRole('button', { name: 'Starting...' })).toHaveProperty('disabled', true);

    await act(async () => {
      resolveDownload?.();
    });
  });

  test('shows download progress text and percentage', () => {
    const status = makeStatus({
      status: 'downloading',
      progress: { current: 256, total: 1024, unit: 'bytes' },
    });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    expect(screen.getByText('25%')).toBeDefined();
    expect(screen.getByText(/256 B/)).toBeDefined();
    expect(screen.getByText(/1 KB/)).toBeDefined();
  });
});
