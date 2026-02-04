/**
 * Tests for Replacement components.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { ReplacementList } from '../components/Replacements/ReplacementList';
import { ReplacementEditor } from '../components/Replacements/ReplacementEditor';
import { ReplacementPreview } from '../components/Replacements/ReplacementPreview';
import { PresetsPanel } from '../components/Replacements/PresetsPanel';
import type { ReplacementRule, PresetInfo } from '../types';

// Mock rules for testing
const mockRules: ReplacementRule[] = [
  {
    id: '1',
    enabled: true,
    kind: 'literal',
    pattern: 'brb',
    replacement: 'be right back',
    word_boundary: true,
    case_sensitive: false,
    description: 'Expand brb',
    origin: 'user',
  },
  {
    id: '2',
    enabled: true,
    kind: 'regex',
    pattern: '\\bteh\\b',
    replacement: 'the',
    word_boundary: false,
    case_sensitive: false,
    origin: 'user',
  },
  {
    id: '3',
    enabled: false,
    kind: 'literal',
    pattern: 'lol',
    replacement: 'laughing out loud',
    word_boundary: true,
    case_sensitive: false,
    origin: 'user',
  },
];

const mockPresets: PresetInfo[] = [
  {
    id: 'common-typos',
    name: 'Common Typos',
    description: 'Fix common spelling mistakes',
    rule_count: 10,
  },
  {
    id: 'abbreviations',
    name: 'Abbreviations',
    description: 'Expand common abbreviations',
    rule_count: 25,
  },
];

describe('ReplacementList', () => {
  it('renders empty state when no rules', () => {
    render(<ReplacementList rules={[]} onChange={vi.fn()} />);
    expect(screen.getByText('No custom replacement rules yet')).toBeDefined();
    expect(screen.getByText('Add your first rule')).toBeDefined();
  });

  it('renders list of rules', () => {
    render(<ReplacementList rules={mockRules} onChange={vi.fn()} />);
    expect(screen.getByText('brb')).toBeDefined();
    expect(screen.getByText('be right back')).toBeDefined();
    expect(screen.getByText('\\bteh\\b')).toBeDefined();
  });

  it('shows rule count', () => {
    render(<ReplacementList rules={mockRules} onChange={vi.fn()} />);
    expect(screen.getByText('3 rules total')).toBeDefined();
    expect(screen.getByText('2 enabled')).toBeDefined();
  });

  it('toggles rule enabled state', () => {
    const onChange = vi.fn();
    render(<ReplacementList rules={mockRules} onChange={onChange} />);

    // Find and click the first checkbox
    const checkboxes = screen.getAllByRole('checkbox');
    fireEvent.click(checkboxes[0]);

    expect(onChange).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({ id: '1', enabled: false }),
      ])
    );
  });

  it('opens editor when add button clicked', () => {
    render(<ReplacementList rules={[]} onChange={vi.fn()} />);

    fireEvent.click(screen.getByText('Add Rule'));

    expect(screen.getByText('Add Replacement Rule')).toBeDefined();
  });

  it('shows type badges', () => {
    render(<ReplacementList rules={mockRules} onChange={vi.fn()} />);

    // Literal type badges
    const litBadges = screen.getAllByText('lit');
    expect(litBadges.length).toBe(2);

    // Regex type badge
    expect(screen.getByText('rx')).toBeDefined();
  });

  it('deletes rule when delete button clicked', async () => {
    const onChange = vi.fn();
    render(<ReplacementList rules={mockRules} onChange={onChange} />);

    // Hover to reveal delete buttons (simulate by finding them)
    const deleteButtons = document.querySelectorAll('button[title="Delete rule"]');
    expect(deleteButtons.length).toBe(3);

    fireEvent.click(deleteButtons[0]);

    expect(onChange).toHaveBeenCalledWith(
      expect.not.arrayContaining([expect.objectContaining({ id: '1' })])
    );
  });
});

describe('ReplacementEditor', () => {
  it('renders in add mode', () => {
    render(
      <ReplacementEditor
        rule={null}
        onSave={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    expect(screen.getByText('Add Replacement Rule')).toBeDefined();
    expect(screen.getByText('Add Rule')).toBeDefined();
  });

  it('renders in edit mode', () => {
    render(
      <ReplacementEditor
        rule={mockRules[0]}
        onSave={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    expect(screen.getByText('Edit Replacement Rule')).toBeDefined();
    expect(screen.getByText('Save Changes')).toBeDefined();
  });

  it('populates fields when editing', () => {
    render(
      <ReplacementEditor
        rule={mockRules[0]}
        onSave={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    const patternInput = screen.getByPlaceholderText('Text to find') as HTMLInputElement;
    expect(patternInput.value).toBe('brb');
  });

  it('validates regex patterns', () => {
    render(
      <ReplacementEditor
        rule={null}
        onSave={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    // Switch to regex mode
    fireEvent.click(screen.getByLabelText('Regular expression'));

    // Enter invalid regex
    const patternInput = screen.getByPlaceholderText('Regular expression');
    fireEvent.change(patternInput, { target: { value: '[invalid' } });

    // Should show error
    expect(screen.getByText(/Invalid/)).toBeDefined();
  });

  it('shows valid regex feedback', () => {
    render(
      <ReplacementEditor
        rule={null}
        onSave={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    // Switch to regex mode
    fireEvent.click(screen.getByLabelText('Regular expression'));

    // Enter valid regex
    const patternInput = screen.getByPlaceholderText('Regular expression');
    fireEvent.change(patternInput, { target: { value: '\\btest\\b' } });

    // Should show valid feedback
    expect(screen.getByText('Valid regex')).toBeDefined();
  });

  it('calls onSave with correct data', () => {
    const onSave = vi.fn();
    render(
      <ReplacementEditor
        rule={null}
        onSave={onSave}
        onCancel={vi.fn()}
      />
    );

    // Fill in form
    fireEvent.change(screen.getByPlaceholderText('Text to find'), {
      target: { value: 'hello' },
    });
    fireEvent.change(screen.getByPlaceholderText(/Replace with/), {
      target: { value: 'hi' },
    });

    // Click save
    fireEvent.click(screen.getByText('Add Rule'));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: 'literal',
        pattern: 'hello',
        replacement: 'hi',
        enabled: true,
      })
    );
  });

  it('calls onCancel when cancel clicked', () => {
    const onCancel = vi.fn();
    render(
      <ReplacementEditor
        rule={null}
        onSave={vi.fn()}
        onCancel={onCancel}
      />
    );

    fireEvent.click(screen.getByText('Cancel'));

    expect(onCancel).toHaveBeenCalled();
  });

  it('disables save when pattern is empty', () => {
    render(
      <ReplacementEditor
        rule={null}
        onSave={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    const saveButton = screen.getByText('Add Rule');
    expect(saveButton).toHaveProperty('disabled', true);
  });

  it('shows word boundary option only for literal', () => {
    render(
      <ReplacementEditor
        rule={null}
        onSave={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    // Literal mode - should show word boundary
    expect(screen.getByText('Match whole words only')).toBeDefined();

    // Switch to regex mode
    fireEvent.click(screen.getByLabelText('Regular expression'));

    // Should not show word boundary
    expect(screen.queryByText('Match whole words only')).toBeNull();
  });
});

describe('ReplacementPreview', () => {
  it('renders empty state', () => {
    render(<ReplacementPreview rules={[]} />);

    expect(screen.getByText('Test Rules')).toBeDefined();
    expect(screen.getByPlaceholderText(/Type or paste text/)).toBeDefined();
  });

  it('shows output when text entered', () => {
    render(<ReplacementPreview rules={mockRules} />);

    const input = screen.getByPlaceholderText(/Type or paste text/);
    fireEvent.change(input, { target: { value: 'I will brb' } });

    // Should show transformed output
    expect(screen.getByText('I will be right back')).toBeDefined();
  });

  it('shows no changes indicator when nothing changed', () => {
    render(<ReplacementPreview rules={[]} />);

    const input = screen.getByPlaceholderText(/Type or paste text/);
    fireEvent.change(input, { target: { value: 'hello world' } });

    expect(screen.getByText('No changes')).toBeDefined();
  });

  it('shows rule count', () => {
    render(<ReplacementPreview rules={mockRules} />);

    expect(screen.getByText('2 rules active')).toBeDefined();
  });

  it('shows macro buttons', () => {
    render(<ReplacementPreview rules={[]} />);

    expect(screen.getByText('{{date}}')).toBeDefined();
    expect(screen.getByText('{{time}}')).toBeDefined();
    expect(screen.getByText('{{datetime}}')).toBeDefined();
  });

  it('inserts macro when clicked', () => {
    render(<ReplacementPreview rules={[]} />);

    // Click macro button
    fireEvent.click(screen.getByText('{{date}}'));

    // Input should contain the macro
    const input = screen.getByPlaceholderText(/Type or paste text/) as HTMLTextAreaElement;
    expect(input.value).toBe('{{date}}');
  });

  it('expands macros when enabled', () => {
    render(<ReplacementPreview rules={[]} />);

    const input = screen.getByPlaceholderText(/Type or paste text/);
    fireEvent.change(input, { target: { value: 'Today is {{date}}' } });

    // Output should contain expanded date (not the macro)
    const output = screen.getByText(/Today is/);
    expect(output.textContent).not.toContain('{{date}}');
  });

  it('can disable macro expansion', () => {
    render(<ReplacementPreview rules={[]} />);

    // Disable macros
    fireEvent.click(screen.getByText('Expand macros'));

    const input = screen.getByPlaceholderText(/Type or paste text/);
    fireEvent.change(input, { target: { value: '{{date}}' } });

    // Output should still have macro
    expect(screen.getByText('{{date}}')).toBeDefined();
  });
});

describe('PresetsPanel', () => {
  it('renders empty state when no presets', () => {
    render(
      <PresetsPanel
        presets={[]}
        enabledPresets={[]}
        onTogglePreset={vi.fn()}
      />
    );

    expect(screen.getByText('No presets available')).toBeDefined();
  });

  it('renders list of presets', () => {
    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={[]}
        onTogglePreset={vi.fn()}
      />
    );

    expect(screen.getByText('Common Typos')).toBeDefined();
    expect(screen.getByText('Fix common spelling mistakes')).toBeDefined();
    expect(screen.getByText('Abbreviations')).toBeDefined();
  });

  it('shows rule count per preset', () => {
    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={[]}
        onTogglePreset={vi.fn()}
      />
    );

    expect(screen.getByText('10 rules')).toBeDefined();
    expect(screen.getByText('25 rules')).toBeDefined();
  });

  it('shows enabled count', () => {
    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={['common-typos']}
        onTogglePreset={vi.fn()}
      />
    );

    expect(screen.getByText('1 of 2 enabled')).toBeDefined();
  });

  it('toggles preset when switch clicked', () => {
    const onTogglePreset = vi.fn();
    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={[]}
        onTogglePreset={onTogglePreset}
      />
    );

    // Find toggle switches (checkboxes in sr-only)
    const toggles = document.querySelectorAll('input[type="checkbox"]');
    fireEvent.click(toggles[0]);

    expect(onTogglePreset).toHaveBeenCalledWith('common-typos', true);
  });

  it('shows view rules button', () => {
    const presetRules = new Map<string, ReplacementRule[]>();
    presetRules.set('common-typos', [mockRules[0]]);

    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={[]}
        onTogglePreset={vi.fn()}
        presetRules={presetRules}
      />
    );

    expect(screen.getByText('View rules')).toBeDefined();
  });

  it('expands rules when view clicked', () => {
    const presetRules = new Map<string, ReplacementRule[]>();
    presetRules.set('common-typos', [mockRules[0]]);

    render(
      <PresetsPanel
        presets={mockPresets}
        enabledPresets={[]}
        onTogglePreset={vi.fn()}
        presetRules={presetRules}
      />
    );

    fireEvent.click(screen.getByText('View rules'));

    // Should show the rule pattern
    expect(screen.getByText('brb')).toBeDefined();
    expect(screen.getByText('Hide rules')).toBeDefined();
  });
});
