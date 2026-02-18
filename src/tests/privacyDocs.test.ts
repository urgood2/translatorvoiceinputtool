import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

describe('PRIVACY.md config path docs', () => {
  const privacyDoc = readFileSync(resolve(process.cwd(), 'docs/PRIVACY.md'), 'utf8');

  it('documents config paths with OpenVoicy directory casing', () => {
    expect(privacyDoc).toContain('%APPDATA%\\OpenVoicy\\config.json');
    expect(privacyDoc).toContain('~/Library/Application Support/OpenVoicy/config.json');
    expect(privacyDoc).toContain('~/.config/OpenVoicy/config.json');
  });

  it('does not document lowercase config directory paths', () => {
    expect(privacyDoc).not.toContain('%APPDATA%\\openvoicy\\config.json');
    expect(privacyDoc).not.toContain('~/Library/Application Support/openvoicy/config.json');
    expect(privacyDoc).not.toContain('~/.config/openvoicy/config.json');
  });
});

