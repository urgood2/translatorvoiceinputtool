/**
 * Wire-format parity tests between Rust serde output and TypeScript types.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { CannotRecordReason, DisplayServer } from '../types';

interface TauriWireSnapshotV1 {
  cannot_record_reason: string[];
  display_server: Array<{ type: string; compositor?: string }>;
}

function readWireSnapshot(): TauriWireSnapshotV1 {
  const snapshotPath = resolve(process.cwd(), 'shared/contracts/tauri_wire.v1.json');
  return JSON.parse(readFileSync(snapshotPath, 'utf-8')) as TauriWireSnapshotV1;
}

describe('tauri wire snapshots', () => {
  it('matches Rust CannotRecordReason wire enum values', () => {
    const snapshot = readWireSnapshot();

    const expected: CannotRecordReason[] = [
      'paused',
      'model_loading',
      'already_recording',
      'still_transcribing',
      'in_error_state',
    ];

    expect(snapshot.cannot_record_reason).toEqual(expected);
  });

  it('matches Rust DisplayServer tagged enum values', () => {
    const snapshot = readWireSnapshot();

    const expected: DisplayServer[] = [
      { type: 'windows' },
      { type: 'mac_os' },
      { type: 'x11' },
      { type: 'wayland', compositor: 'sway' },
      { type: 'unknown' },
    ];

    expect(snapshot.display_server).toEqual(expected);
  });
});
