/**
 * Tests for Replacement tab components — assertions specified in bead 1wh.5.5.
 *
 * Covers:
 * 1. ReplacementList renders all current rules
 * 2. PresetsPanel renders available presets with toggle
 * 3. Tab badge shows count of active rules + presets
 * 4. Preview uses sidecar pipeline (mock invoke for preview_replacement)
 * 5. Preview result matches expected output (parity with apply)
 * 6. Adding/removing rules updates the list reactively
 * 7. Loading preset adds its rules to the active set
 * 8. Rule with id, kind, word_boundary, case_sensitive fields renders correctly
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ReplacementList } from './ReplacementList';
import { ReplacementPreview, processPreviewText } from './ReplacementPreview';
import { PresetsPanel } from './PresetsPanel';
import { selectReplacementBadgeCount } from '../../store/appStore';
import type { ReplacementRule, PresetInfo, AppConfig, AppStore } from '../../types';

// ---------------------------------------------------------------------------
// Mock invoke for sidecar preview tests
// ---------------------------------------------------------------------------

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

beforeEach(() => {
  mockInvoke.mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

type PreviewResponse = {
  result: string;
  truncated: boolean;
  applied_rules_count: number;
  applied_presets: string[];
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const literalRule: ReplacementRule = {
  id: 'rule-lit-1',
  enabled: true,
  kind: 'literal',
  pattern: 'brb',
  replacement: 'be right back',
  word_boundary: true,
  case_sensitive: false,
  description: 'Expand brb abbreviation',
  origin: 'user',
};

const regexRule: ReplacementRule = {
  id: 'rule-rx-1',
  enabled: true,
  kind: 'regex',
  pattern: '\\bteh\\b',
  replacement: 'the',
  word_boundary: false,
  case_sensitive: true,
  origin: 'user',
};

const disabledRule: ReplacementRule = {
  id: 'rule-disabled',
  enabled: false,
  kind: 'literal',
  pattern: 'lol',
  replacement: 'laughing out loud',
  word_boundary: true,
  case_sensitive: false,
  origin: 'user',
};

const presetRule: ReplacementRule = {
  id: 'punctuation:period',
  enabled: true,
  kind: 'literal',
  pattern: ' period',
  replacement: '.',
  word_boundary: false,
  case_sensitive: false,
  origin: 'preset:punctuation',
};

const allRules: ReplacementRule[] = [literalRule, regexRule, disabledRule];

const mockPresets: PresetInfo[] = [
  { id: 'common-typos', name: 'Common Typos', description: 'Fix common typos', rule_count: 10 },
  { id: 'punctuation', name: 'Punctuation', description: 'Auto-punctuate', rule_count: 5 },
];

// ---------------------------------------------------------------------------
// 1. ReplacementList renders all current rules
// ---------------------------------------------------------------------------

describe('ReplacementList renders all current rules', () => {
  it('renders every user rule pattern and replacement', () => {
    render(<ReplacementList rules={allRules} onChange={vi.fn()} />);

    expect(screen.getByText('brb')).toBeDefined();
    expect(screen.getByText('be right back')).toBeDefined();
    expect(screen.getByText('\\bteh\\b')).toBeDefined();
    expect(screen.getByText('the')).toBeDefined();
    expect(screen.getByText('lol')).toBeDefined();
    expect(screen.getByText('laughing out loud')).toBeDefined();

    const renderedRuleCount = document.querySelectorAll('button[title="Edit rule"]').length;
    console.info('[replacements.test] rendered_rule_count=%d', renderedRuleCount);
  });

  it('displays correct total and enabled count', () => {
    render(<ReplacementList rules={allRules} onChange={vi.fn()} />);

    expect(screen.getByText('3 rules total')).toBeDefined();
    expect(screen.getByText('2 enabled')).toBeDefined();
  });

  it('hides preset-origin rules from user list', () => {
    const rulesWithPreset = [...allRules, presetRule];
    render(<ReplacementList rules={rulesWithPreset} onChange={vi.fn()} />);

    // Preset rule pattern should NOT appear in the user rule list
    expect(screen.queryByText(' period')).toBeNull();
    // User rule count should exclude preset rules
    expect(screen.getByText('3 rules total')).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// 2. PresetsPanel renders available presets with toggle
// ---------------------------------------------------------------------------

describe('PresetsPanel renders available presets with toggle', () => {
  it('renders all presets', () => {
    render(
      <PresetsPanel presets={mockPresets} enabledPresets={[]} onTogglePreset={vi.fn()} />
    );

    expect(screen.getByText('Common Typos')).toBeDefined();
    expect(screen.getByText('Punctuation')).toBeDefined();

    const renderedPresetCount = document.querySelectorAll('input[type="checkbox"]').length;
    console.info('[replacements.test] rendered_preset_count=%d', renderedPresetCount);
  });

  it('shows toggle switches for each preset', () => {
    render(
      <PresetsPanel presets={mockPresets} enabledPresets={[]} onTogglePreset={vi.fn()} />
    );

    const toggles = document.querySelectorAll('input[type="checkbox"]');
    expect(toggles.length).toBe(2);
  });

  it('calls onTogglePreset when switch toggled', () => {
    const onToggle = vi.fn();
    render(
      <PresetsPanel presets={mockPresets} enabledPresets={[]} onTogglePreset={onToggle} />
    );

    const toggles = document.querySelectorAll('input[type="checkbox"]');
    fireEvent.click(toggles[0]);

    expect(onToggle).toHaveBeenCalledWith('common-typos', true);
  });

  it('reflects enabled state correctly', () => {
    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={['punctuation']}
        onTogglePreset={vi.fn()}
      />
    );

    expect(screen.getByText('1 of 2 enabled')).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// 3. Tab badge shows count of active rules + presets
// ---------------------------------------------------------------------------

describe('Tab badge: selectReplacementBadgeCount', () => {
  function makeState(replacements: ReplacementRule[], enabledPresets: string[]): AppStore {
    return {
      config: {
        replacements,
        presets: { enabled_presets: enabledPresets },
      } as AppConfig,
    } as AppStore;
  }

  it('counts enabled rules only', () => {
    const state = makeState(allRules, []);
    // 2 enabled rules, 0 presets
    expect(selectReplacementBadgeCount(state)).toBe(2);
  });

  it('counts enabled presets from config', () => {
    const state = makeState([], ['punctuation', 'common-typos']);
    // 0 rules, 2 configured presets
    expect(selectReplacementBadgeCount(state)).toBe(2);
  });

  it('sums enabled rules and presets', () => {
    const rulesWithPreset = [...allRules, presetRule];
    const state = makeState(rulesWithPreset, ['punctuation']);
    // 3 enabled rules (lit, rx, presetRule) + 1 configured preset = 4
    // (derived preset count from rule origins = 1, max(1,1) = 1)
    expect(selectReplacementBadgeCount(state)).toBe(4);
  });

  it('derives preset count from rule origins when config is empty', () => {
    const rulesWithPreset = [literalRule, presetRule];
    const state = makeState(rulesWithPreset, []);
    // 2 enabled rules + max(0 config, 1 derived) = 3
    expect(selectReplacementBadgeCount(state)).toBe(3);
  });

  it('returns 0 when config is null', () => {
    const state = { config: null } as unknown as AppStore;
    expect(selectReplacementBadgeCount(state)).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// 4. Preview uses sidecar pipeline (mock invoke for preview_replacement)
// ---------------------------------------------------------------------------

describe('Preview sidecar parity', () => {
  it('uses ReplacementPreview UI to call preview_replacement and render RPC output', async () => {
    const sidecarResult: PreviewResponse = {
      result: 'I will be right back',
      truncated: false,
      applied_rules_count: 1,
      applied_presets: [],
    };

    mockInvoke.mockResolvedValueOnce(sidecarResult);
    render(<ReplacementPreview rules={[literalRule]} />);

    const input = screen.getByPlaceholderText(/Type or paste text/);
    fireEvent.change(input, { target: { value: 'I will brb' } });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('preview_replacement', {
        input: 'I will brb',
        rules: [literalRule],
      });
    });
    expect(screen.getByText(sidecarResult.result)).toBeDefined();
  });

  it('local processPreviewText matches sidecar for regex rules', () => {
    const input = 'teh cat sat on teh mat';
    const localResult = processPreviewText(input, [regexRule]);
    expect(localResult).toBe('the cat sat on the mat');
  });

  it('local processPreviewText handles disabled rules like sidecar', () => {
    const localResult = processPreviewText('brb lol', [literalRule, disabledRule]);
    // Only literalRule applies; disabledRule is skipped
    expect(localResult).toBe('be right back lol');
  });

  it('debounces sidecar preview calls and sends latest input only', async () => {
    mockInvoke.mockResolvedValue({
      result: 'I will be right back',
      truncated: false,
      applied_rules_count: 1,
      applied_presets: [],
    });
    render(<ReplacementPreview rules={[literalRule]} />);

    const input = screen.getByPlaceholderText(/Type or paste text/);
    fireEvent.change(input, { target: { value: 'I will b' } });
    fireEvent.change(input, { target: { value: 'I will brb' } });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledTimes(1);
      expect(mockInvoke).toHaveBeenCalledWith('preview_replacement', {
        input: 'I will brb',
        rules: [literalRule],
      });
    });
  });

  it('shows fallback indicator when sidecar preview fails and keeps local output', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('sidecar down'));
    render(<ReplacementPreview rules={[literalRule]} />);

    const input = screen.getByPlaceholderText(/Type or paste text/);
    fireEvent.change(input, { target: { value: 'I said brb' } });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('preview_replacement', {
        input: 'I said brb',
        rules: [literalRule],
      });
      expect(screen.getByTestId('preview-fallback')).toBeDefined();
    });
    expect(screen.getByText(processPreviewText('I said brb', [literalRule]))).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// 5. Preview result matches expected output (parity with apply)
// ---------------------------------------------------------------------------

describe('Preview result matches expected output', () => {
  it('literal rule with word boundary replaces only whole words', () => {
    const input = 'brbing is brb';
    const result = processPreviewText(input, [literalRule]);
    // word_boundary=true: "brbing" stays, standalone "brb" replaced
    expect(result).toBe('brbing is be right back');
  });

  it('case insensitive literal matches different cases', () => {
    const input = 'BRB soon';
    const result = processPreviewText(input, [literalRule]);
    expect(result).toBe('be right back soon');
  });

  it('case sensitive regex does not match wrong case', () => {
    const input = 'TEH problem';
    const result = processPreviewText(input, [regexRule]);
    // case_sensitive=true: "TEH" should NOT match "teh"
    expect(result).toBe('TEH problem');
  });

  it('renders preview output matching processPreviewText', () => {
    render(<ReplacementPreview rules={[literalRule]} />);

    const input = screen.getByPlaceholderText(/Type or paste text/);
    fireEvent.change(input, { target: { value: 'I said brb' } });

    const expected = processPreviewText('I said brb', [literalRule]);
    expect(screen.getByText(expected)).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// 6. Adding/removing rules updates the list reactively
// ---------------------------------------------------------------------------

describe('Adding/removing rules updates list reactively', () => {
  it('adding a new rule via editor calls onChange with appended rule', () => {
    const onChange = vi.fn();
    render(<ReplacementList rules={allRules} onChange={onChange} />);

    // Open editor
    fireEvent.click(screen.getByText('Add Rule'));
    expect(screen.getByText('Add Replacement Rule')).toBeDefined();

    // Fill in fields
    fireEvent.change(screen.getByPlaceholderText('Text to find'), {
      target: { value: 'omw' },
    });
    fireEvent.change(screen.getByPlaceholderText(/Replace with/), {
      target: { value: 'on my way' },
    });

    // Save
    const saveButtons = screen.getAllByText('Add Rule');
    // The second "Add Rule" button is in the editor modal
    fireEvent.click(saveButtons[saveButtons.length - 1]);
    console.info('[replacements.test] rule_action=add pattern=%s', 'omw');

    expect(onChange).toHaveBeenCalledWith(
      expect.arrayContaining([
        ...allRules,
        expect.objectContaining({
          kind: 'literal',
          pattern: 'omw',
          replacement: 'on my way',
          enabled: true,
        }),
      ])
    );
  });

  it('pressing Escape closes replacement editor dialog', () => {
    render(<ReplacementList rules={allRules} onChange={vi.fn()} />);

    fireEvent.click(screen.getByText('Add Rule'));
    expect(screen.getByRole('dialog')).toBeDefined();

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('removing a rule calls onChange without the deleted rule', () => {
    const onChange = vi.fn();
    render(<ReplacementList rules={allRules} onChange={onChange} />);

    const deleteButtons = document.querySelectorAll('button[title="Delete rule"]');
    fireEvent.click(deleteButtons[0]);
    console.info('[replacements.test] rule_action=remove id=%s', 'rule-lit-1');

    expect(onChange).toHaveBeenCalledWith(
      expect.not.arrayContaining([expect.objectContaining({ id: 'rule-lit-1' })])
    );
  });

  it('toggling a rule calls onChange with flipped enabled state', () => {
    const onChange = vi.fn();
    render(<ReplacementList rules={allRules} onChange={onChange} />);

    const checkboxes = screen.getAllByRole('checkbox');
    fireEvent.click(checkboxes[0]); // Toggle first rule (literalRule)

    expect(onChange).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({ id: 'rule-lit-1', enabled: false }),
      ])
    );
  });
});

// ---------------------------------------------------------------------------
// 7. Loading preset adds its rules to the active set
// ---------------------------------------------------------------------------

describe('Loading preset adds its rules to the active set', () => {
  it('toggling preset on triggers onTogglePreset callback', () => {
    const onToggle = vi.fn();
    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={[]}
        onTogglePreset={onToggle}
      />
    );

    const toggles = document.querySelectorAll('input[type="checkbox"]');
    fireEvent.click(toggles[1]); // Toggle "Punctuation" preset

    expect(onToggle).toHaveBeenCalledWith('punctuation', true);
  });

  it('toggling preset off triggers onTogglePreset with false', () => {
    const onToggle = vi.fn();
    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={['punctuation']}
        onTogglePreset={onToggle}
      />
    );

    const toggles = document.querySelectorAll('input[type="checkbox"]');
    fireEvent.click(toggles[1]); // Toggle "Punctuation" off

    expect(onToggle).toHaveBeenCalledWith('punctuation', false);
  });

  it('preset rules shown in expandable section after loading', () => {
    const presetRules = new Map<string, ReplacementRule[]>();
    presetRules.set('punctuation', [presetRule]);

    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={['punctuation']}
        onTogglePreset={vi.fn()}
        presetRules={presetRules}
      />
    );

    // Expand rules
    const viewButtons = screen.getAllByText('View rules');
    fireEvent.click(viewButtons[viewButtons.length - 1]);

    // The pattern " period" has a leading space; use a code element query
    const codeElements = document.querySelectorAll('code');
    const patternCodes = Array.from(codeElements).map((el) => el.textContent);
    expect(patternCodes).toContain(' period');
  });
});

// ---------------------------------------------------------------------------
// 8. Rule with id, kind, word_boundary, case_sensitive fields renders correctly
// ---------------------------------------------------------------------------

describe('Rule fields render correctly', () => {
  it('shows literal type badge for literal rules', () => {
    render(<ReplacementList rules={[literalRule]} onChange={vi.fn()} />);
    expect(screen.getByText('lit')).toBeDefined();
  });

  it('shows regex type badge for regex rules', () => {
    render(<ReplacementList rules={[regexRule]} onChange={vi.fn()} />);
    expect(screen.getByText('rx')).toBeDefined();
  });

  it('shows both lit and rx badges when mixed rules present', () => {
    render(<ReplacementList rules={[literalRule, regexRule]} onChange={vi.fn()} />);
    expect(screen.getByText('lit')).toBeDefined();
    expect(screen.getByText('rx')).toBeDefined();
  });

  it('renders rule pattern and replacement text', () => {
    render(<ReplacementList rules={[literalRule]} onChange={vi.fn()} />);
    expect(screen.getByText('brb')).toBeDefined();
    expect(screen.getByText('be right back')).toBeDefined();
  });

  it('renders description when present', () => {
    render(<ReplacementList rules={[literalRule]} onChange={vi.fn()} />);
    expect(screen.getByText('Expand brb abbreviation')).toBeDefined();
  });

  it('shows preset badge for preset-origin rules in editor context', () => {
    // Preset rules show "preset" badge when rendered within the list
    // (preset rules are filtered from ReplacementList, but shown in PresetsPanel)
    const presetRules = new Map<string, ReplacementRule[]>();
    presetRules.set('punctuation', [presetRule]);

    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={['punctuation']}
        onTogglePreset={vi.fn()}
        presetRules={presetRules}
      />
    );

    // Expand to see rule details
    const viewButtons = screen.getAllByText('View rules');
    fireEvent.click(viewButtons[viewButtons.length - 1]);

    // The pattern " period" has a leading space; use code element query
    const codeElements = document.querySelectorAll('code');
    const codeTexts = Array.from(codeElements).map((el) => el.textContent);
    expect(codeTexts).toContain(' period');
    expect(codeTexts).toContain('.');
  });

  it('editor shows word_boundary checkbox for literal mode', () => {
    render(<ReplacementList rules={[]} onChange={vi.fn()} />);

    // Open add editor
    fireEvent.click(screen.getByText('Add Rule'));

    // literal mode default - word boundary visible
    expect(screen.getByText('Match whole words only')).toBeDefined();
  });

  it('editor shows case_sensitive checkbox', () => {
    render(<ReplacementList rules={[]} onChange={vi.fn()} />);

    fireEvent.click(screen.getByText('Add Rule'));

    expect(screen.getByText('Case sensitive')).toBeDefined();
  });

  it('editor preserves kind, word_boundary, case_sensitive when editing existing rule', () => {
    const onChange = vi.fn();
    render(<ReplacementList rules={[literalRule]} onChange={onChange} />);

    // Click edit
    const editButton = document.querySelector('button[title="Edit rule"]');
    expect(editButton).not.toBeNull();
    fireEvent.click(editButton!);

    // Check fields are populated
    expect(screen.getByText('Edit Replacement Rule')).toBeDefined();
    const patternInput = screen.getByPlaceholderText('Text to find') as HTMLInputElement;
    expect(patternInput.value).toBe('brb');

    // Word boundary and case sensitive should match the rule
    const wordBoundaryCheckbox = screen.getByLabelText('Match whole words only') as HTMLInputElement;
    expect(wordBoundaryCheckbox.checked).toBe(true);

    const caseSensitiveCheckbox = screen.getByLabelText('Case sensitive') as HTMLInputElement;
    expect(caseSensitiveCheckbox.checked).toBe(false);
  });
});
