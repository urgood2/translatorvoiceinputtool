/**
 * Tests for SelfCheck and Diagnostics components.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { SelfCheck } from '../components/Settings/SelfCheck';
import { Diagnostics } from '../components/Settings/Diagnostics';
import type { SelfCheckResult, DiagnosticsReport, Capabilities, AppConfig } from '../types';

// Mock self-check result
const mockSelfCheckResult: SelfCheckResult = {
  hotkey: {
    status: 'ok',
    message: 'Registered and working',
    detail: 'Primary: Ctrl+Shift+Space, Copy Last: Ctrl+Shift+C',
  },
  injection: {
    status: 'ok',
    message: 'Clipboard paste available',
  },
  microphone: {
    status: 'warning',
    message: 'Permission granted',
    detail: 'Using default device: Built-in Microphone',
  },
  sidecar: {
    status: 'ok',
    message: 'Connected and responsive',
  },
  model: {
    status: 'ok',
    message: 'Ready',
    detail: 'Model: parakeet-tdt-0.6b-v3',
  },
};

const mockCapabilities: Capabilities = {
  display_server: { type: 'x11' },
  hotkey_press_available: true,
  hotkey_release_available: true,
  keystroke_injection_available: true,
  clipboard_available: true,
  hotkey_mode: {
    configured: 'hold',
    effective: 'hold',
  },
  injection_method: {
    configured: 'clipboard_paste',
    effective: 'clipboard_paste',
  },
  permissions: {
    microphone: 'granted',
  },
  diagnostics: 'Platform: Linux x86_64\nKernel: 6.1.0',
};

const mockConfig: AppConfig = {
  schema_version: 1,
  audio: {
    device_uid: 'device-123',
    audio_cues_enabled: true,
  },
  hotkeys: {
    primary: 'Ctrl+Shift+Space',
    copy_last: 'Ctrl+Shift+C',
    mode: 'hold',
  },
  injection: {
    paste_delay_ms: 40,
    restore_clipboard: true,
    suffix: ' ',
    focus_guard_enabled: true,
  },
  replacements: [
    { id: '1', enabled: true, kind: 'literal', pattern: 'test', replacement: 'test', word_boundary: true, case_sensitive: false },
  ],
  ui: {
    show_on_startup: false,
    window_width: 800,
    window_height: 600,
  },
  presets: {
    enabled_presets: [],
  },
};

const mockDiagnosticsReport: DiagnosticsReport = {
  version: '0.1.0',
  platform: 'linux-x64',
  capabilities: mockCapabilities,
  config: mockConfig,
  self_check: mockSelfCheckResult,
};

describe('SelfCheck', () => {
  it('shows loading state when result is null', () => {
    render(
      <SelfCheck result={null} onRefresh={vi.fn()} />
    );
    expect(screen.getByText('Running checks...')).toBeDefined();
  });

  it('shows loading state when isLoading is true', () => {
    render(
      <SelfCheck result={mockSelfCheckResult} onRefresh={vi.fn()} isLoading={true} />
    );
    expect(screen.getByText('Running checks...')).toBeDefined();
  });

  it('renders all check items', () => {
    render(
      <SelfCheck result={mockSelfCheckResult} onRefresh={vi.fn()} />
    );

    expect(screen.getByText('Hotkey')).toBeDefined();
    expect(screen.getByText('Injection')).toBeDefined();
    expect(screen.getByText('Microphone')).toBeDefined();
    expect(screen.getByText('Sidecar')).toBeDefined();
    expect(screen.getByText('Model')).toBeDefined();
  });

  it('shows check messages', () => {
    render(
      <SelfCheck result={mockSelfCheckResult} onRefresh={vi.fn()} />
    );

    expect(screen.getByText('Registered and working')).toBeDefined();
    expect(screen.getByText('Clipboard paste available')).toBeDefined();
    expect(screen.getByText('Connected and responsive')).toBeDefined();
  });

  it('shows all systems operational when all ok', () => {
    const allOkResult: SelfCheckResult = {
      ...mockSelfCheckResult,
      microphone: { status: 'ok', message: 'OK' },
    };

    render(
      <SelfCheck result={allOkResult} onRefresh={vi.fn()} />
    );

    expect(screen.getByText('All systems operational')).toBeDefined();
  });

  it('shows warning count', () => {
    render(
      <SelfCheck result={mockSelfCheckResult} onRefresh={vi.fn()} />
    );

    expect(screen.getByText('1 warning')).toBeDefined();
  });

  it('shows error count', () => {
    const errorResult: SelfCheckResult = {
      ...mockSelfCheckResult,
      hotkey: { status: 'error', message: 'Failed to register' },
      sidecar: { status: 'error', message: 'Not responding' },
    };

    render(
      <SelfCheck result={errorResult} onRefresh={vi.fn()} />
    );

    expect(screen.getByText('2 issues found')).toBeDefined();
  });

  it('calls onRefresh when refresh button clicked', async () => {
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    render(
      <SelfCheck result={mockSelfCheckResult} onRefresh={onRefresh} />
    );

    await act(async () => {
      fireEvent.click(screen.getByTitle('Refresh checks'));
    });

    expect(onRefresh).toHaveBeenCalled();
  });

  it('expands detail when item clicked', () => {
    render(
      <SelfCheck result={mockSelfCheckResult} onRefresh={vi.fn()} />
    );

    // Click on hotkey row (has detail)
    fireEvent.click(screen.getByText('Hotkey'));

    // Should show detail
    expect(screen.getByText(/Primary: Ctrl\+Shift\+Space/)).toBeDefined();
  });

  it('shows header', () => {
    render(
      <SelfCheck result={mockSelfCheckResult} onRefresh={vi.fn()} />
    );

    expect(screen.getByText('System Health Check')).toBeDefined();
  });
});

describe('Diagnostics', () => {
  const mockWriteText = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    // Mock clipboard API using vi.stubGlobal
    vi.stubGlobal('navigator', {
      ...navigator,
      clipboard: {
        writeText: mockWriteText,
      },
    });
    mockWriteText.mockClear();
  });

  it('shows loading state when report is null', () => {
    render(
      <Diagnostics report={null} onRefresh={vi.fn()} />
    );
    expect(screen.getByText('Gathering diagnostics...')).toBeDefined();
  });

  it('shows loading state when isLoading is true', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} isLoading={true} />
    );
    expect(screen.getByText('Gathering diagnostics...')).toBeDefined();
  });

  it('renders diagnostics header', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    expect(screen.getByText('Diagnostics')).toBeDefined();
  });

  it('shows copy button', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    expect(screen.getByText('Copy to Clipboard')).toBeDefined();
  });

  it('shows refresh button', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    expect(screen.getByText('Refresh')).toBeDefined();
  });

  it('includes version in output', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    expect(screen.getByText(/Version: 0\.1\.0/)).toBeDefined();
  });

  it('includes platform in output', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    expect(screen.getByText(/Platform: linux-x64/)).toBeDefined();
  });

  it('shows self check results in output', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    expect(screen.getByText(/Hotkey: \[OK\]/)).toBeDefined();
  });

  it('shows stats', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    // Should show line and char count
    expect(screen.getByText(/lines/)).toBeDefined();
    expect(screen.getByText(/chars/)).toBeDefined();
  });

  it('shows privacy notice', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    expect(screen.getByText(/Privacy:/)).toBeDefined();
    expect(screen.getByText(/does not include transcript text/)).toBeDefined();
  });

  it('copies to clipboard when button clicked', async () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    await act(async () => {
      fireEvent.click(screen.getByText('Copy to Clipboard'));
    });

    expect(mockWriteText).toHaveBeenCalled();
    expect(screen.getByText('Copied!')).toBeDefined();
  });

  it('calls onRefresh when refresh button clicked', async () => {
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={onRefresh} />
    );

    await act(async () => {
      fireEvent.click(screen.getByText('Refresh'));
    });

    expect(onRefresh).toHaveBeenCalled();
  });

  it('redacts device UID in config', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    // Should not show actual device UID
    expect(screen.queryByText(/device-123/)).toBeNull();
    // Should show redacted placeholder
    expect(screen.getByText(/\[REDACTED\]/)).toBeDefined();
  });

  it('shows replacement count instead of rules', () => {
    render(
      <Diagnostics report={mockDiagnosticsReport} onRefresh={vi.fn()} />
    );

    // Should show count, not actual patterns
    expect(screen.getByText(/1 rules/)).toBeDefined();
  });
});
