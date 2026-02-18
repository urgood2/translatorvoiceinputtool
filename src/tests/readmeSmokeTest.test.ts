import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

describe('README smoke test instructions', () => {
  const readme = readFileSync(resolve(process.cwd(), 'README.md'), 'utf8');

  it('does not reference deprecated echo-command flow', () => {
    expect(readme).not.toContain('Call Rust Echo Command');
  });

  it('documents the current UI and Rust integration smoke path', () => {
    expect(readme).toContain('Model Status');
    expect(readme).toContain('Audio Devices');
    expect(readme).toContain('Recent Transcripts');
    expect(readme).toContain(
      'Click "Refresh" in "Audio Devices" to exercise the Rust command path (list_audio_devices)'
    );
  });
});
