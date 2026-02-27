/**
 * Tests for history export invoke integration.
 *
 * Verifies that the Tauri export_history command is called correctly
 * and handles both success and error responses.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { setMockInvokeHandler } from './setup';
import { COMMAND_EXPORT_HISTORY } from '../types.contracts';

// ============================================================================
// HELPERS
// ============================================================================

/**
 * Thin wrapper around invoke('export_history') matching the expected contract.
 * In production this would live in the store; here we test the invoke contract.
 */
async function exportHistory(format: string): Promise<string> {
  return invoke<string>(COMMAND_EXPORT_HISTORY, { format });
}

// ============================================================================
// TESTS
// ============================================================================

describe('export_history invoke contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setMockInvokeHandler(() => undefined);
  });

  it('invokes export_history with markdown format and returns file path', async () => {
    setMockInvokeHandler((cmd, args) => {
      if (cmd === 'export_history') {
        const params = args as { format: string };
        expect(params.format).toBe('markdown');
        return '/home/user/Downloads/openvoicy-history-20260227-120000-000.md';
      }
      return undefined;
    });

    const path = await exportHistory('markdown');
    expect(path).toBe('/home/user/Downloads/openvoicy-history-20260227-120000-000.md');
    expect(invoke).toHaveBeenCalledWith('export_history', { format: 'markdown' });
  });

  it('invokes export_history with csv format and returns file path', async () => {
    setMockInvokeHandler((cmd, args) => {
      if (cmd === 'export_history') {
        const params = args as { format: string };
        expect(params.format).toBe('csv');
        return '/home/user/Downloads/openvoicy-history-20260227-120000-000.csv';
      }
      return undefined;
    });

    const path = await exportHistory('csv');
    expect(path).toBe('/home/user/Downloads/openvoicy-history-20260227-120000-000.csv');
    expect(invoke).toHaveBeenCalledWith('export_history', { format: 'csv' });
  });

  it('propagates error when export_history command fails', async () => {
    setMockInvokeHandler((cmd) => {
      if (cmd === 'export_history') {
        throw new Error('unsupported export format: json');
      }
      return undefined;
    });

    await expect(exportHistory('json')).rejects.toThrow('unsupported export format: json');
  });

  it('propagates IO error from backend', async () => {
    setMockInvokeHandler((cmd) => {
      if (cmd === 'export_history') {
        throw new Error('failed to export history: Permission denied');
      }
      return undefined;
    });

    await expect(exportHistory('markdown')).rejects.toThrow('failed to export history');
  });

  it('returns a string path (not an object)', async () => {
    setMockInvokeHandler((cmd) => {
      if (cmd === 'export_history') {
        return '/tmp/openvoicy-history-20260227-120000-000.md';
      }
      return undefined;
    });

    const result = await exportHistory('markdown');
    expect(typeof result).toBe('string');
    expect(result).toContain('openvoicy-history-');
  });

  it('uses the correct command constant from contracts', () => {
    expect(COMMAND_EXPORT_HISTORY).toBe('export_history');
  });
});
