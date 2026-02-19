import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

interface ContractItem {
  name?: string;
  deprecated_aliases?: string[];
  payload_schema?: unknown;
}

interface TauriEventsContractV1 {
  version: number;
  items: ContractItem[];
}

function readContract(): TauriEventsContractV1 {
  const contractPath = resolve(process.cwd(), 'shared/contracts/tauri.events.v1.json');
  return JSON.parse(readFileSync(contractPath, 'utf-8')) as TauriEventsContractV1;
}

describe('tauri.events.v1 contract', () => {
  it('has version 1 and items with payload schemas', () => {
    const contract = readContract();

    expect(contract.version).toBe(1);
    expect(Array.isArray(contract.items)).toBe(true);
    expect(contract.items.length).toBeGreaterThan(0);

    contract.items.forEach((item) => {
      expect(typeof item.name).toBe('string');
      expect(Array.isArray(item.deprecated_aliases)).toBe(true);
      expect(item.payload_schema).toBeDefined();
    });
  });

  it('includes required canonical event names and alias mappings', () => {
    const contract = readContract();
    const byName = new Map(
      contract.items
        .filter((item): item is ContractItem & { name: string } => typeof item.name === 'string')
        .map((item) => [item.name, item])
    );

    const requiredNames = [
      'state:changed',
      'recording:status',
      'model:status',
      'model:progress',
      'audio:level',
      'transcript:complete',
      'transcript:error',
      'app:error',
      'sidecar:status',
    ];

    requiredNames.forEach((name) => {
      expect(byName.has(name)).toBe(true);
    });

    expect(byName.get('state:changed')?.deprecated_aliases).toContain('state_changed');
    expect(byName.get('transcript:complete')?.deprecated_aliases).toContain('transcription:complete');
    expect(byName.get('transcript:error')?.deprecated_aliases).toContain('transcription:error');
    expect(byName.get('sidecar:status')?.deprecated_aliases).toContain('status:changed');
    expect(byName.get('recording:status')?.deprecated_aliases).toEqual([]);
  });
});
