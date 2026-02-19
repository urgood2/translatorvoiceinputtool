import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

interface ContractItem {
  name?: string;
}

interface TauriCommandsContractV1 {
  version: number;
  items: ContractItem[];
}

function readContract(): TauriCommandsContractV1 {
  const contractPath = resolve(process.cwd(), 'shared/contracts/tauri.commands.v1.json');
  return JSON.parse(readFileSync(contractPath, 'utf-8')) as TauriCommandsContractV1;
}

describe('tauri.commands.v1 contract', () => {
  it('has version 1 and an items array', () => {
    const contract = readContract();
    expect(contract.version).toBe(1);
    expect(Array.isArray(contract.items)).toBe(true);
    expect(contract.items.length).toBeGreaterThan(0);
  });

  it('includes required command names from the v1 plan', () => {
    const contract = readContract();
    const names = new Set(contract.items.map((item) => item.name).filter(Boolean));

    const required = [
      'get_app_state',
      'get_capabilities',
      'get_capability_issues',
      'can_start_recording',
      'run_self_check',
      'get_config',
      'update_config',
      'reset_config_to_defaults',
      'list_audio_devices',
      'set_audio_device',
      'start_mic_test',
      'stop_mic_test',
      'get_model_status',
      'download_model',
      'purge_model_cache',
      'get_model_catalog',
      'get_transcript_history',
      'copy_transcript',
      'copy_last_transcript',
      'clear_history',
      'get_hotkey_status',
      'set_hotkey',
      'get_replacement_rules',
      'set_replacement_rules',
      'preview_replacement',
      'get_available_presets',
      'load_preset',
      'toggle_enabled',
      'is_enabled',
      'set_enabled',
      'generate_diagnostics',
      'get_recent_logs',
      'start_recording',
      'stop_recording',
      'cancel_recording',
      'restart_sidecar',
      'export_history',
    ];

    required.forEach((name) => {
      expect(names.has(name)).toBe(true);
    });
  });
});
