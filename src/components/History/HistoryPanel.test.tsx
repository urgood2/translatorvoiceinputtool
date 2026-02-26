/**
 * Tests for HistoryPanel component.
 */

import { describe, it, expect, vi } from 'vitest';
import { useState } from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { HistoryPanel } from './HistoryPanel';
import type { TranscriptEntry } from '../../types';

// Mock entries for testing
const mockEntries: TranscriptEntry[] = [
  {
    id: '1',
    text: 'This is a test transcript.',
    raw_text: 'This is a test transcript.',
    final_text: 'This is a test transcript.',
    timestamp: new Date(Date.now() - 60000).toISOString(), // 1 minute ago
    audio_duration_ms: 3500,
    transcription_duration_ms: 1200,
    injection_result: { status: 'injected' },
  },
  {
    id: '2',
    text: 'Another transcript that was clipboard only.',
    raw_text: 'Another transcript that was clipboard only.',
    final_text: 'Another transcript that was clipboard only.',
    timestamp: new Date(Date.now() - 3600000).toISOString(), // 1 hour ago
    audio_duration_ms: 5000,
    transcription_duration_ms: 1800,
    injection_result: { status: 'clipboard_only', reason: 'Focus changed' },
  },
  {
    id: '3',
    text: 'Failed transcript.',
    raw_text: 'Failed transcript.',
    final_text: 'Failed transcript.',
    timestamp: new Date(Date.now() - 86400000).toISOString(), // 1 day ago
    audio_duration_ms: 2000,
    transcription_duration_ms: 500,
    injection_result: { status: 'error', message: 'Permission denied' },
  },
];

describe('HistoryPanel', () => {
  it('renders empty state when no entries', () => {
    render(<HistoryPanel entries={[]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('No transcripts yet')).toBeDefined();
    expect(screen.getByText('Press the hotkey to start recording.')).toBeDefined();
    expect(screen.getByTestId('history-clear-all-button')).toBeDisabled();
  });

  it('renders transcript entries', () => {
    render(<HistoryPanel entries={mockEntries} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('This is a test transcript.')).toBeDefined();
    expect(screen.getByText('Another transcript that was clipboard only.')).toBeDefined();
    expect(screen.getByText('Failed transcript.')).toBeDefined();
    expect(screen.getByTestId('history-scroll-region')).toBeDefined();
  });

  it('renders legacy entries without additive fields', () => {
    const legacyEntry: TranscriptEntry = {
      id: 'legacy',
      text: 'Legacy transcript text.',
      timestamp: new Date().toISOString(),
      audio_duration_ms: 1200,
      transcription_duration_ms: 320,
      injection_result: { status: 'injected' },
    };

    render(<HistoryPanel entries={[legacyEntry]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('Legacy transcript text.')).toBeDefined();
    expect(screen.queryByText(/Language:/)).toBeNull();
    expect(screen.queryByText(/Confidence:/)).toBeNull();
    expect(screen.queryByText(/Session:/)).toBeNull();
  });

  it('shows transcript metadata when additive fields are present', () => {
    const enrichedEntry: TranscriptEntry = {
      ...mockEntries[0],
      text: 'Fallback text',
      final_text: 'Final text used for display',
      raw_text: 'Original raw text',
      language: 'en',
      confidence: 0.93,
      session_id: 'session-1234567890',
      timings: {
        ipc_ms: 12,
        transcribe_ms: 230,
        postprocess_ms: 17,
        inject_ms: 9,
        total_ms: 268,
      },
    };

    render(<HistoryPanel entries={[enrichedEntry]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('Final text used for display')).toBeDefined();
    expect(screen.getByTestId('history-entry-toggle-1')).toBeDefined();
    fireEvent.click(screen.getByTestId('history-entry-toggle-1'));
    expect(screen.getByText('Original raw text')).toBeDefined();
    expect(screen.getByText('Language: en')).toBeDefined();
    expect(screen.getByText('Confidence: 93%')).toBeDefined();
    expect(screen.getByTestId('history-entry-session-1').textContent).toContain('Session: session-');
    expect(screen.getByText('IPC: 12ms')).toBeDefined();
    expect(screen.getByText('Transcribe: 230ms')).toBeDefined();
    expect(screen.getByText('Post: 17ms')).toBeDefined();
    expect(screen.getByText('Inject: 9ms')).toBeDefined();
    expect(screen.getByText('Total: 268ms')).toBeDefined();
  });

  it('shows entry count', () => {
    render(<HistoryPanel entries={mockEntries} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('3 entries')).toBeDefined();
  });

  it('shows singular "entry" for one entry', () => {
    render(<HistoryPanel entries={[mockEntries[0]]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('1 entry')).toBeDefined();
  });

  it('calls onCopy when copy button clicked', async () => {
    const onCopy = vi.fn().mockResolvedValue(undefined);
    render(<HistoryPanel entries={[mockEntries[0]]} onCopy={onCopy} />);

    const copyButton = screen.getByText('Copy');
    fireEvent.click(copyButton);

    expect(onCopy).toHaveBeenCalledWith('1');
    expect(await screen.findByText('✓ Copied')).toBeDefined();
  });

  it('shows copied feedback after clicking copy', async () => {
    render(<HistoryPanel entries={[mockEntries[0]]} onCopy={vi.fn().mockResolvedValue(undefined)} />);

    const copyButton = screen.getByText('Copy');
    fireEvent.click(copyButton);

    expect(await screen.findByText('✓ Copied')).toBeDefined();
  });

  it('shows error feedback when copy fails', async () => {
    render(
      <HistoryPanel
        entries={[mockEntries[0]]}
        onCopy={vi.fn().mockRejectedValue(new Error('copy failed'))}
      />
    );

    fireEvent.click(screen.getByText('Copy'));

    expect(await screen.findByText('copy failed')).toBeDefined();
    expect(screen.queryByText('✓ Copied')).toBeNull();
  });

  it('displays injected badge correctly', () => {
    render(<HistoryPanel entries={[mockEntries[0]]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('✅')).toBeDefined();
    expect(screen.getByText('Injected')).toBeDefined();
  });

  it('displays clipboard-only badge correctly', () => {
    render(<HistoryPanel entries={[mockEntries[1]]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('📋')).toBeDefined();
    expect(screen.getByText('Clipboard')).toBeDefined();
  });

  it('displays error badge correctly', () => {
    render(<HistoryPanel entries={[mockEntries[2]]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('⚠️')).toBeDefined();
    expect(screen.getByText('Error')).toBeDefined();
  });

  it('displays relative timestamps', () => {
    render(<HistoryPanel entries={mockEntries} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    // Check for relative time format
    expect(screen.getByText('1 min ago')).toBeDefined();
    expect(screen.getByText('1 hr ago')).toBeDefined();
  });

  it('displays audio duration', () => {
    render(<HistoryPanel entries={[mockEntries[0]]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('3.5s audio')).toBeDefined();
  });

  it('displays audio duration in minutes for longer clips', () => {
    const longEntry: TranscriptEntry = {
      ...mockEntries[0],
      audio_duration_ms: 125000, // 2m 5s
    };
    render(<HistoryPanel entries={[longEntry]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('2m 5s audio')).toBeDefined();
  });

  it('has tooltip for clipboard-only reason', () => {
    const { container } = render(
      <HistoryPanel entries={[mockEntries[1]]} onCopy={vi.fn().mockResolvedValue(undefined)} />
    );
    const badge = container.querySelector('[title="Focus changed"]');
    expect(badge).toBeDefined();
  });

  it('has tooltip for error message', () => {
    const { container } = render(
      <HistoryPanel entries={[mockEntries[2]]} onCopy={vi.fn().mockResolvedValue(undefined)} />
    );
    const badge = container.querySelector('[title="Permission denied"]');
    expect(badge).toBeDefined();
  });

  it('filters entries by text after debounce and shows match summary', () => {
    vi.useFakeTimers();
    try {
      render(<HistoryPanel entries={mockEntries} onCopy={vi.fn().mockResolvedValue(undefined)} />);

      fireEvent.change(screen.getByTestId('history-search-input'), { target: { value: 'clipboard' } });

      expect(screen.getByText('Failed transcript.')).toBeDefined();

      act(() => {
        vi.advanceTimersByTime(300);
      });

      expect(screen.getByText('Another transcript that was clipboard only.')).toBeDefined();
      expect(screen.queryByText('This is a test transcript.')).toBeNull();
      expect(screen.getByText('Showing 1 of 3 entries')).toBeDefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it('matches search query against language field', () => {
    vi.useFakeTimers();
    try {
      const languageEntries: TranscriptEntry[] = [
        {
          ...mockEntries[0],
          id: 'lang-fr',
          text: 'Bonjour transcript',
          raw_text: 'Bonjour transcript',
          final_text: 'Bonjour transcript',
          language: 'fr',
        },
        {
          ...mockEntries[1],
          id: 'lang-en',
          text: 'Hello transcript',
          raw_text: 'Hello transcript',
          final_text: 'Hello transcript',
          language: 'en',
        },
      ];

      render(<HistoryPanel entries={languageEntries} onCopy={vi.fn().mockResolvedValue(undefined)} />);
      fireEvent.change(screen.getByTestId('history-search-input'), { target: { value: 'fr' } });

      act(() => {
        vi.advanceTimersByTime(300);
      });

      expect(screen.getByText('Bonjour transcript')).toBeDefined();
      expect(screen.queryByText('Hello transcript')).toBeNull();
      expect(screen.getByText('Showing 1 of 2 entries')).toBeDefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it('shows no-match state and clear button resets search', () => {
    vi.useFakeTimers();
    try {
      render(<HistoryPanel entries={mockEntries} onCopy={vi.fn().mockResolvedValue(undefined)} />);

      fireEvent.change(screen.getByTestId('history-search-input'), { target: { value: 'no-hit-term' } });
      act(() => {
        vi.advanceTimersByTime(300);
      });

      expect(screen.getByText('No matching transcripts')).toBeDefined();
      expect(screen.getByText('Showing 0 of 3 entries')).toBeDefined();
      expect(screen.getByTestId('history-search-clear')).toBeDefined();

      fireEvent.click(screen.getByTestId('history-search-clear'));
      act(() => {
        vi.advanceTimersByTime(300);
      });

      expect(screen.queryByText('No matching transcripts')).toBeNull();
      expect(screen.getByText('3 entries')).toBeDefined();
      expect(screen.queryByTestId('history-search-clear')).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('renders many entries inside scrollable overflow container', () => {
    const manyEntries: TranscriptEntry[] = Array.from({ length: 50 }, (_, i) => ({
      id: `entry-${i}`,
      text: `Transcript entry number ${i}`,
      raw_text: `Transcript entry number ${i}`,
      final_text: `Transcript entry number ${i}`,
      timestamp: new Date(Date.now() - i * 60000).toISOString(),
      audio_duration_ms: 2000,
      transcription_duration_ms: 500,
      injection_result: { status: 'injected' as const },
    }));

    render(<HistoryPanel entries={manyEntries} onCopy={vi.fn().mockResolvedValue(undefined)} />);

    const scrollRegion = screen.getByTestId('history-scroll-region');
    expect(scrollRegion).toBeDefined();
    expect(scrollRegion.className).toContain('overflow-y-auto');

    expect(screen.getByText('50 entries')).toBeDefined();
    expect(screen.getByText('Transcript entry number 0')).toBeDefined();
    expect(screen.getByText('Transcript entry number 49')).toBeDefined();
  });

  it('opens confirmation dialog and cancels clear-all', () => {
    const onClearAll = vi.fn().mockResolvedValue(undefined);
    render(
      <HistoryPanel
        entries={mockEntries}
        onCopy={vi.fn().mockResolvedValue(undefined)}
        onClearAll={onClearAll}
      />
    );

    fireEvent.click(screen.getByTestId('history-clear-all-button'));
    expect(screen.getByRole('dialog')).toBeDefined();
    expect(screen.getByText('Clear all transcript history?')).toBeDefined();

    fireEvent.click(screen.getByTestId('history-clear-cancel'));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(onClearAll).not.toHaveBeenCalled();
  });

  it('closes clear-all dialog on Escape key', () => {
    render(
      <HistoryPanel
        entries={mockEntries}
        onCopy={vi.fn().mockResolvedValue(undefined)}
        onClearAll={vi.fn().mockResolvedValue(undefined)}
      />
    );

    fireEvent.click(screen.getByTestId('history-clear-all-button'));
    expect(screen.getByRole('dialog')).toBeDefined();

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('confirms clear-all and calls handler', async () => {
    const onClearAll = vi.fn().mockResolvedValue(undefined);
    render(
      <HistoryPanel
        entries={mockEntries}
        onCopy={vi.fn().mockResolvedValue(undefined)}
        onClearAll={onClearAll}
      />
    );

    fireEvent.click(screen.getByTestId('history-clear-all-button'));
    fireEvent.click(screen.getByTestId('history-clear-confirm'));

    expect(onClearAll).toHaveBeenCalledTimes(1);
  });

  it('shows empty placeholder after successful clear', async () => {
    function Harness() {
      const [entries, setEntries] = useState(mockEntries);
      return (
        <HistoryPanel
          entries={entries}
          onCopy={vi.fn().mockResolvedValue(undefined)}
          onClearAll={async () => {
            setEntries([]);
          }}
        />
      );
    }

    render(<Harness />);
    fireEvent.click(screen.getByTestId('history-clear-all-button'));
    fireEvent.click(screen.getByTestId('history-clear-confirm'));

    expect(await screen.findByText('No transcripts yet')).toBeDefined();
  });
});
