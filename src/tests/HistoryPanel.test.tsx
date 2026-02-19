/**
 * Tests for HistoryPanel component.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { HistoryPanel } from '../components/Settings/HistoryPanel';
import type { TranscriptEntry } from '../types';

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
    expect(screen.getByText('No recent transcripts')).toBeDefined();
    expect(screen.getByText('Press the hotkey to start recording.')).toBeDefined();
  });

  it('renders transcript entries', () => {
    render(<HistoryPanel entries={mockEntries} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('This is a test transcript.')).toBeDefined();
    expect(screen.getByText('Another transcript that was clipboard only.')).toBeDefined();
    expect(screen.getByText('Failed transcript.')).toBeDefined();
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
      session_id: 'session-123',
    };

    render(<HistoryPanel entries={[enrichedEntry]} onCopy={vi.fn().mockResolvedValue(undefined)} />);
    expect(screen.getByText('Final text used for display')).toBeDefined();
    expect(screen.getByText('Raw: Original raw text')).toBeDefined();
    expect(screen.getByText('Language: en')).toBeDefined();
    expect(screen.getByText('Confidence: 93%')).toBeDefined();
    expect(screen.getByText('Session: session-123')).toBeDefined();
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
});
