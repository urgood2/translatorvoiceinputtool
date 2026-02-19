import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { TabPanel } from './TabPanel';

describe('TabPanel', () => {
  it('renders content only when panel id matches active tab', () => {
    const { rerender } = render(
      <TabPanel id="status" activeTab="status">
        <p>Status content</p>
      </TabPanel>
    );

    expect(screen.getByText('Status content')).toBeDefined();

    rerender(
      <TabPanel id="status" activeTab="history">
        <p>Status content</p>
      </TabPanel>
    );

    expect(screen.queryByText('Status content')).toBeNull();
  });

  it('sets tabpanel ARIA linkage to corresponding tab id', () => {
    render(
      <TabPanel id="history" activeTab="history">
        <p>History content</p>
      </TabPanel>
    );

    const panel = screen.getByRole('tabpanel');
    expect(panel.getAttribute('id')).toBe('panel-history');
    expect(panel.getAttribute('aria-labelledby')).toBe('history-tab');
    expect(panel.className).toContain('overflow-y-auto');
  });
});
