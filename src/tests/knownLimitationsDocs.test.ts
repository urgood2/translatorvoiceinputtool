import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

describe('KNOWN_LIMITATIONS.md remediation docs', () => {
  const knownLimitationsDoc = readFileSync(
    resolve(process.cwd(), 'docs/KNOWN_LIMITATIONS.md'),
    'utf8'
  );

  it('documents Windows antivirus and SmartScreen remediation guidance', () => {
    expect(knownLimitationsDoc).toContain('Antivirus / SmartScreen Friction');
    expect(knownLimitationsDoc).toContain('Windows Security -> Protection history');
    expect(knownLimitationsDoc).toContain('Run anyway');
  });

  it('documents macOS Gatekeeper quarantine remediation guidance', () => {
    expect(knownLimitationsDoc).toContain('Gatekeeper / Quarantine Friction');
    expect(knownLimitationsDoc).toContain('Open Anyway');
    expect(knownLimitationsDoc).toContain('xattr -dr com.apple.quarantine');
  });
});
