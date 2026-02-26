import { describe, expect, test, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
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

  test('shows install button for missing state', () => {
    const status = makeStatus({ status: 'missing' });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    const downloadButton = screen.getByRole('button', { name: 'Install Model' });
    expect(downloadButton).toBeDefined();
    expect(downloadButton).toHaveProperty('disabled', false);
  });

  test('shows retry install button for error state', () => {
    const status = makeStatus({ status: 'error', error: 'network timeout' });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    expect(screen.getByRole('button', { name: 'Retry Install' })).toBeDefined();
    expect(screen.queryByRole('button', { name: 'Install Model' })).toBeNull();
  });

  test('shows in-progress install state when downloading', () => {
    const status = makeStatus({
      status: 'downloading',
      progress: { current: 64, total: 128, unit: 'bytes' },
    });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    expect(screen.getByRole('button', { name: 'Installing...' })).toHaveProperty('disabled', true);
    expect(screen.queryByRole('button', { name: 'Install Model' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Retry Install' })).toBeNull();
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
      fireEvent.click(screen.getByRole('button', { name: 'Install Model' }));
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

  test('shows download speed and ETA when progress advances over time', async () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date('2026-01-01T00:00:00.000Z'));
      const { rerender } = render(
        <ModelSettings
          status={makeStatus({
            status: 'downloading',
            progress: { current: 0, total: 1000, unit: 'bytes' },
          })}
          onDownload={vi.fn()}
          onPurgeCache={vi.fn()}
        />
      );

      expect(screen.queryByText(/Speed:/)).toBeNull();
      expect(screen.queryByText(/ETA:/)).toBeNull();

      vi.setSystemTime(new Date('2026-01-01T00:00:02.000Z'));
      await act(async () => {
        rerender(
          <ModelSettings
            status={makeStatus({
              status: 'downloading',
              progress: { current: 500, total: 1000, unit: 'bytes' },
            })}
            onDownload={vi.fn()}
            onPurgeCache={vi.fn()}
          />
        );
      });

      expect(screen.getByText('Speed: 250 B/s')).toBeDefined();
      expect(screen.getByText('ETA: 2s')).toBeDefined();
    } finally {
      vi.useRealTimers();
    }
  });

  test('loads model catalog and renders model cards', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_model_catalog') {
        return Promise.resolve([
          {
            model_id: 'nvidia/parakeet-tdt-0.6b-v3',
            family: 'parakeet',
            display_name: 'Parakeet 0.6B',
            description: 'Fast and accurate',
            supported_languages: ['en'],
            default_language: 'en',
            size_bytes: 1024,
            manifest_path: 'model/MODEL_MANIFEST.json',
          },
          {
            model_id: 'openai/whisper-small',
            family: 'whisper',
            display_name: 'Whisper Small',
            description: 'Multilingual transcription',
            supported_languages: ['en', 'es'],
            default_language: 'auto',
            size_bytes: 2048,
            manifest_path: 'model/MODEL_MANIFEST.json',
          },
        ] as unknown);
      }
      return Promise.resolve(undefined as unknown);
    });

    const status = makeStatus({ status: 'ready', model_id: 'nvidia/parakeet-tdt-0.6b-v3' });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    expect(await screen.findByText('Available Models')).toBeDefined();
    expect(await screen.findByText('Parakeet 0.6B')).toBeDefined();
    expect(await screen.findByText('Whisper Small')).toBeDefined();
  });

  test('calls onSelectModel when selecting a different model', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_model_catalog') {
        return Promise.resolve([
          {
            model_id: 'nvidia/parakeet-tdt-0.6b-v3',
            family: 'parakeet',
            display_name: 'Parakeet 0.6B',
            description: 'Fast and accurate',
            supported_languages: ['en'],
            default_language: 'en',
            size_bytes: 1024,
            manifest_path: 'model/MODEL_MANIFEST.json',
          },
          {
            model_id: 'openai/whisper-small',
            family: 'whisper',
            display_name: 'Whisper Small',
            description: 'Multilingual transcription',
            supported_languages: ['en', 'es'],
            default_language: 'auto',
            size_bytes: 2048,
            manifest_path: 'model/MODEL_MANIFEST.json',
          },
        ] as unknown);
      }
      return Promise.resolve(undefined as unknown);
    });

    const onSelectModel = vi.fn().mockResolvedValue(undefined);
    const status = makeStatus({ status: 'ready', model_id: 'nvidia/parakeet-tdt-0.6b-v3' });
    render(
      <ModelSettings
        status={status}
        onDownload={vi.fn()}
        onPurgeCache={vi.fn()}
        onSelectModel={onSelectModel}
      />
    );

    await screen.findByText('Whisper Small');

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Select' }));
    });

    expect(onSelectModel).toHaveBeenCalledWith('openai/whisper-small');
  });

  test('shows language dropdown when selected model family is whisper', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_model_catalog') {
        return Promise.resolve([
          {
            model_id: 'openai/whisper-small',
            family: 'whisper',
            display_name: 'Whisper Small',
            description: 'Multilingual transcription',
            supported_languages: ['en', 'de'],
            default_language: 'auto',
            size_bytes: 2048,
            manifest_path: 'model/MODEL_MANIFEST.json',
          },
        ] as unknown);
      }
      if (cmd === 'get_config') {
        return Promise.resolve({
          schema_version: 1,
          audio: {},
          hotkeys: {},
          injection: {},
          model: {
            model_id: 'openai/whisper-small',
            device: null,
            preferred_device: 'auto',
            language: 'auto',
          },
          replacements: [],
          ui: {},
          history: {},
          presets: {},
        } as unknown);
      }
      return Promise.resolve(undefined as unknown);
    });

    const status = makeStatus({ status: 'ready', model_id: 'openai/whisper-small' });
    render(<ModelSettings status={status} onDownload={vi.fn()} onPurgeCache={vi.fn()} />);

    const dropdown = await screen.findByRole('combobox', { name: /language/i });
    expect(dropdown).toBeDefined();
    expect(screen.getByRole('option', { name: 'Auto detect' })).toBeDefined();
    expect(screen.getByRole('option', { name: 'EN' })).toBeDefined();
    expect(screen.getByRole('option', { name: 'DE' })).toBeDefined();
  });
});
