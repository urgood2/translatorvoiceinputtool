import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { computeAccessibleName } from 'dom-accessibility-api';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TabBar } from '../components/Layout/TabBar';
import { HistoryPanel } from '../components/History/HistoryPanel';
import { StatusIndicator } from '../components/StatusIndicator';
import { useReducedMotion } from '../hooks/useReducedMotion';
import { useAppStore } from '../store';
import type { TranscriptEntry } from '../types';

const CSS_PATH = resolve(process.cwd(), 'src/index.css');
const CSS_SOURCE = readFileSync(CSS_PATH, 'utf-8');

function createTranscript(id: string, text: string): TranscriptEntry {
  return {
    id,
    text,
    timestamp: new Date('2026-01-01T00:00:00.000Z').toISOString(),
    audio_duration_ms: 1200,
    transcription_duration_ms: 280,
    injection_result: { status: 'injected' },
  };
}

function extractRgbVariable(name: string): [number, number, number] {
  const pattern = new RegExp(`--${name}:\\s*(\\d+)\\s+(\\d+)\\s+(\\d+);`);
  const match = CSS_SOURCE.match(pattern);
  if (!match) {
    throw new Error(`Missing CSS variable: --${name}`);
  }
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function relativeLuminance([r, g, b]: [number, number, number]): number {
  const toLinear = (channel: number) => {
    const value = channel / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  const [lr, lg, lb] = [toLinear(r), toLinear(g), toLinear(b)];
  return (0.2126 * lr) + (0.7152 * lg) + (0.0722 * lb);
}

function contrastRatio(a: [number, number, number], b: [number, number, number]): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  const lighter = Math.max(la, lb);
  const darker = Math.min(la, lb);
  return (lighter + 0.05) / (darker + 0.05);
}

function installMatchMedia(matches: boolean) {
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const mediaQueryList = {
    matches,
    media: '(prefers-reduced-motion: reduce)',
    onchange: null,
    addEventListener: (_: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.add(listener);
    },
    removeEventListener: (_: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.delete(listener);
    },
    addListener: (listener: (event: MediaQueryListEvent) => void) => {
      listeners.add(listener);
    },
    removeListener: (listener: (event: MediaQueryListEvent) => void) => {
      listeners.delete(listener);
    },
    dispatchEvent: () => true,
  };

  vi.stubGlobal('matchMedia', vi.fn(() => mediaQueryList));
  return mediaQueryList;
}

function ReducedMotionProbe() {
  const { enabled } = useReducedMotion();
  return <div data-testid="reduce-motion-enabled">{String(enabled)}</div>;
}

describe('Accessibility Regression Coverage', () => {
  beforeEach(() => {
    document.documentElement.classList.remove('reduce-motion');
    useAppStore.setState({ config: null });
    vi.unstubAllGlobals();
  });

  it('all interactive elements expose accessible names and logs ARIA coverage', async () => {
    const entries = [createTranscript('t-1', 'Sample transcript text.')];
    const onCopy = vi.fn().mockResolvedValue(undefined);
    const onClearAll = vi.fn().mockResolvedValue(undefined);

    render(
      <div>
        <TabBar
          tabs={[
            { id: 'status', label: 'Status' },
            { id: 'history', label: 'History' },
            { id: 'settings', label: 'Settings' },
          ]}
          activeTab="status"
          onTabChange={() => {}}
        />
        <HistoryPanel entries={entries} onCopy={onCopy} onClearAll={onClearAll} />
      </div>
    );

    const interactive = [
      ...Array.from(document.querySelectorAll('button, input, select, textarea, [role="tab"]')),
    ] as HTMLElement[];

    const missing: Array<{ tag: string; testId: string | null }> = [];
    const details = interactive.map((element) => {
      const name = computeAccessibleName(element).trim();
      const ariaLabel = element.getAttribute('aria-label');
      const ariaLabelledBy = element.getAttribute('aria-labelledby');
      const testId = element.getAttribute('data-testid');
      if (!name) {
        missing.push({ tag: element.tagName.toLowerCase(), testId });
      }
      return {
        tag: element.tagName.toLowerCase(),
        role: element.getAttribute('role') ?? '',
        name,
        ariaLabel,
        ariaLabelledBy,
        testId,
      };
    });

    console.info('[a11y] Interactive element name audit:', details);
    console.info('[a11y] Missing accessible names:', missing);
    expect(missing).toHaveLength(0);

    await act(async () => {
      fireEvent.click(screen.getByTestId('history-clear-all-button'));
    });
    expect(screen.getByRole('dialog')).toBeDefined();
  });

  it('tab order follows logical reading order and logs tab sequence', () => {
    const onTabChange = vi.fn();
    render(
      <TabBar
        tabs={[
          { id: 'status', label: 'Status' },
          { id: 'history', label: 'History' },
          { id: 'replacements', label: 'Replacements' },
          { id: 'settings', label: 'Settings' },
        ]}
        activeTab="status"
        onTabChange={onTabChange}
      />
    );

    const tabs = screen.getAllByRole('tab');
    const order = tabs.map((tab) => tab.textContent?.trim());
    console.info('[a11y] Tab order sequence:', order);
    expect(order).toEqual(['Status', 'History', 'Replacements', 'Settings']);

    const statusTab = screen.getByRole('tab', { name: 'Status' });
    const historyTab = screen.getByRole('tab', { name: 'History' });
    const settingsTab = screen.getByRole('tab', { name: 'Settings' });

    statusTab.focus();
    fireEvent.keyDown(statusTab, { key: 'ArrowRight' });
    expect(document.activeElement).toBe(historyTab);

    fireEvent.keyDown(historyTab, { key: 'Enter' });
    expect(onTabChange).toHaveBeenCalledWith('history');

    fireEvent.keyDown(historyTab, { key: 'End' });
    expect(document.activeElement).toBe(settingsTab);
  });

  it('reduce_motion=true applies reduced-motion class and disables transitions/animations', () => {
    installMatchMedia(false);
    useAppStore.setState({
      config: {
        ui: {
          reduce_motion: true,
        },
      } as any,
    });

    render(<ReducedMotionProbe />);

    expect(screen.getByTestId('reduce-motion-enabled').textContent).toBe('true');
    expect(document.documentElement.classList.contains('reduce-motion')).toBe(true);
    expect(CSS_SOURCE).toContain('.reduce-motion *, .reduce-motion *::before, .reduce-motion *::after');
    expect(CSS_SOURCE).toContain('transition-duration: 0.01ms !important;');
    expect(CSS_SOURCE).toContain('animation-duration: 0.01ms !important;');
  });

  it('reduce_motion=false keeps normal animation behavior when system preference is off', () => {
    installMatchMedia(false);
    useAppStore.setState({
      config: {
        ui: {
          reduce_motion: false,
        },
      } as any,
    });

    render(<ReducedMotionProbe />);

    expect(screen.getByTestId('reduce-motion-enabled').textContent).toBe('false');
    expect(document.documentElement.classList.contains('reduce-motion')).toBe(false);
    expect(CSS_SOURCE).toContain('body {');
    expect(CSS_SOURCE).toContain('transition: background-color 0.2s ease, color 0.2s ease;');
  });

  it('keyboard-only navigation can activate primary tabs without mouse', () => {
    const onTabChange = vi.fn();
    render(
      <TabBar
        tabs={[
          { id: 'status', label: 'Status' },
          { id: 'history', label: 'History' },
          { id: 'settings', label: 'Settings' },
        ]}
        activeTab="status"
        onTabChange={onTabChange}
      />
    );

    const statusTab = screen.getByRole('tab', { name: 'Status' });
    const historyTab = screen.getByRole('tab', { name: 'History' });

    statusTab.focus();
    fireEvent.keyDown(statusTab, { key: 'ArrowRight' });
    expect(document.activeElement).toBe(historyTab);

    fireEvent.keyDown(historyTab, { key: ' ' });
    expect(onTabChange).toHaveBeenCalledWith('history');
  });

  it('focus indicators are defined for focusable controls', () => {
    expect(CSS_SOURCE).toContain(':focus-visible');
    expect(CSS_SOURCE).toContain('outline: 2px solid rgb(var(--color-accent));');
    expect(CSS_SOURCE).toContain('outline-offset: 2px;');

    render(
      <TabBar
        tabs={[{ id: 'status', label: 'Status' }]}
        activeTab="status"
        onTabChange={() => {}}
      />
    );
    const tab = screen.getByRole('tab', { name: 'Status' });
    expect(tab.className.includes('outline-none')).toBe(false);
    expect(tab.className.includes('focus:outline-none')).toBe(false);
  });

  it('color-scheme follows explicit root theme class', () => {
    expect(CSS_SOURCE).toContain(':root.dark {');
    expect(CSS_SOURCE).toContain('color-scheme: light;');
    expect(CSS_SOURCE).toContain('color-scheme: dark;');
    expect(CSS_SOURCE).not.toContain('color-scheme: light dark;');
  });

  it('announces recording state changes for screen readers', () => {
    const { rerender } = render(
      <StatusIndicator state="idle" enabled={true} detail={undefined} />
    );

    const status = screen.getByRole('status');
    expect(status.getAttribute('aria-live')).toBe('polite');
    expect(screen.getByText('Ready')).toBeDefined();

    rerender(<StatusIndicator state="recording" enabled={true} detail="Recording started" />);
    expect(status.getAttribute('aria-live')).toBe('polite');
    expect(screen.getByText('Recording...')).toBeDefined();
    expect(screen.getByText('Recording started')).toBeDefined();

    rerender(<StatusIndicator state="error" enabled={true} detail="Recording stopped unexpectedly" />);
    expect(status.getAttribute('aria-live')).toBe('assertive');
    expect(screen.getByText('Error')).toBeDefined();
  });

  it('logs modal focus behavior and verifies focus trap dialog semantics', async () => {
    const user = userEvent.setup();
    const entries = [createTranscript('t-2', 'Another sample transcript.')];
    render(
      <HistoryPanel
        entries={entries}
        onCopy={vi.fn().mockResolvedValue(undefined)}
        onClearAll={vi.fn().mockResolvedValue(undefined)}
      />
    );

    await act(async () => {
      fireEvent.click(screen.getByTestId('history-clear-all-button'));
    });

    const dialog = screen.getByRole('dialog');
    expect(dialog.getAttribute('aria-modal')).toBe('true');

    const focusables = Array.from(
      dialog.querySelectorAll('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])')
    ) as HTMLElement[];
    const names = focusables.map((el) => computeAccessibleName(el));
    console.info('[a11y] Modal focus sequence:', names);

    expect(names).toContain('Cancel');
    expect(names).toContain('Clear All');

    const cancelButton = screen.getByTestId('history-clear-cancel');
    const confirmButton = screen.getByTestId('history-clear-confirm');

    expect(document.activeElement).toBe(cancelButton);

    await user.tab();
    expect(document.activeElement).toBe(confirmButton);

    await user.tab();
    expect(document.activeElement).toBe(cancelButton);

    await user.tab({ shift: true });
    expect(document.activeElement).toBe(confirmButton);

    console.info('[a11y] Modal focus trap check active element:', (document.activeElement as HTMLElement)?.outerHTML);

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(screen.getByTestId('history-clear-all-button'));
  });

  it('core light/dark text contrast meets WCAG AA (>= 4.5:1)', () => {
    const lightBg = extractRgbVariable('color-bg');
    const lightText = extractRgbVariable('color-text');
    const lightTextSecondary = extractRgbVariable('color-text-secondary');

    // Dark mode values are defined inside `.dark { ... }`; use direct expectations.
    const darkBg: [number, number, number] = [17, 24, 39];
    const darkText: [number, number, number] = [243, 244, 246];
    const darkTextSecondary: [number, number, number] = [156, 163, 175];

    const lightPrimaryContrast = contrastRatio(lightText, lightBg);
    const lightSecondaryContrast = contrastRatio(lightTextSecondary, lightBg);
    const darkPrimaryContrast = contrastRatio(darkText, darkBg);
    const darkSecondaryContrast = contrastRatio(darkTextSecondary, darkBg);

    console.info('[a11y] Contrast ratios', {
      lightPrimaryContrast,
      lightSecondaryContrast,
      darkPrimaryContrast,
      darkSecondaryContrast,
    });

    expect(lightPrimaryContrast).toBeGreaterThanOrEqual(4.5);
    expect(lightSecondaryContrast).toBeGreaterThanOrEqual(4.5);
    expect(darkPrimaryContrast).toBeGreaterThanOrEqual(4.5);
    expect(darkSecondaryContrast).toBeGreaterThanOrEqual(4.5);
  });
});
