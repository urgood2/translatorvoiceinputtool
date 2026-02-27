import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const TAILWIND_CONFIG_PATH = resolve(process.cwd(), 'tailwind.config.js');
const TAILWIND_CONFIG_SOURCE = readFileSync(TAILWIND_CONFIG_PATH, 'utf-8');

describe('Tailwind Theme Strategy', () => {
  it('locks dark mode to class strategy', () => {
    expect(TAILWIND_CONFIG_SOURCE).toMatch(/darkMode:\s*["']class["']/);
    expect(TAILWIND_CONFIG_SOURCE).not.toMatch(/darkMode:\s*["']media["']/);
  });
});

