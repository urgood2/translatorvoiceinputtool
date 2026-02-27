import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { SUPPORTED_MODEL_FAMILIES } from '../types';

interface CatalogModelEntry {
  family?: string;
}

interface ModelCatalog {
  models?: CatalogModelEntry[];
}

function readCatalog(): ModelCatalog {
  const catalogPath = resolve(process.cwd(), 'shared/model/MODEL_CATALOG.json');
  return JSON.parse(readFileSync(catalogPath, 'utf-8')) as ModelCatalog;
}

describe('Model Family Compatibility', () => {
  it('keeps frontend model family allowlist aligned with supported backend families', () => {
    expect(SUPPORTED_MODEL_FAMILIES).toEqual(['parakeet', 'whisper']);
  });

  it('ensures catalog family values are accepted by frontend model family allowlist', () => {
    const catalog = readCatalog();
    const allowed = new Set<string>(SUPPORTED_MODEL_FAMILIES);
    const models = catalog.models ?? [];

    models.forEach((model, index) => {
      expect(typeof model.family).toBe('string');
      expect(allowed.has(model.family as string)).toBe(
        true,
        `models[${index}].family (${String(model.family)}) is not supported`,
      );
    });
  });
});
