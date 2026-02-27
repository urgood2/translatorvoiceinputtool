export type DedupeStreamKey =
  | 'state'
  | 'transcript'
  | 'transcriptError'
  | 'sidecar'
  | 'modelStatus'
  | 'modelProgress'
  | 'recording'
  | 'audio'
  | 'error';

export interface SeqDedupeTracker {
  shouldProcess(streamKey: DedupeStreamKey, payload: unknown): boolean;
  reset(): void;
}

function extractSeq(payload: unknown): number | undefined {
  if (!payload || typeof payload !== 'object') {
    return undefined;
  }

  const seq = (payload as { seq?: unknown }).seq;
  if (typeof seq === 'number' && Number.isFinite(seq)) {
    return seq;
  }
  return undefined;
}

export function createSeqDedupeTracker(): SeqDedupeTracker {
  const lastSeqByStream = new Map<DedupeStreamKey, number>();

  return {
    shouldProcess(streamKey, payload) {
      const seq = extractSeq(payload);
      // Older payloads do not include seq; process to preserve backward compatibility.
      if (seq === undefined) {
        return true;
      }

      const previous = lastSeqByStream.get(streamKey);
      if (previous !== undefined && seq <= previous) {
        return false;
      }

      lastSeqByStream.set(streamKey, seq);
      return true;
    },
    reset() {
      lastSeqByStream.clear();
    },
  };
}
