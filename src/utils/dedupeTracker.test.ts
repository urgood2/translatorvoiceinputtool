import { describe, expect, test } from 'vitest';
import { createSeqDedupeTracker } from './dedupeTracker';

describe('createSeqDedupeTracker', () => {
  test('processes payloads without seq for backward compatibility', () => {
    const tracker = createSeqDedupeTracker();

    expect(tracker.shouldProcess('state', { state: 'idle' })).toBe(true);
    expect(tracker.shouldProcess('state', { state: 'recording' })).toBe(true);
  });

  test('dedupes non-increasing seq values per stream', () => {
    const tracker = createSeqDedupeTracker();

    expect(tracker.shouldProcess('transcript', { seq: 10 })).toBe(true);
    expect(tracker.shouldProcess('transcript', { seq: 10 })).toBe(false);
    expect(tracker.shouldProcess('transcript', { seq: 9 })).toBe(false);
    expect(tracker.shouldProcess('transcript', { seq: 11 })).toBe(true);
  });

  test('tracks streams independently', () => {
    const tracker = createSeqDedupeTracker();

    expect(tracker.shouldProcess('state', { seq: 5 })).toBe(true);
    expect(tracker.shouldProcess('transcript', { seq: 1 })).toBe(true);
    expect(tracker.shouldProcess('state', { seq: 5 })).toBe(false);
    expect(tracker.shouldProcess('transcript', { seq: 1 })).toBe(false);
    expect(tracker.shouldProcess('transcript', { seq: 2 })).toBe(true);
  });

  test('reset clears all stream state', () => {
    const tracker = createSeqDedupeTracker();

    expect(tracker.shouldProcess('error', { seq: 3 })).toBe(true);
    expect(tracker.shouldProcess('error', { seq: 3 })).toBe(false);

    tracker.reset();

    expect(tracker.shouldProcess('error', { seq: 3 })).toBe(true);
  });
});
