import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TabBar } from './TabBar';

describe('TabBar', () => {
  const tabs = [
    { id: 'status', label: 'Status' },
    { id: 'history', label: 'History' },
    { id: 'replacements', label: 'Replacements', badge: 3 },
    { id: 'settings', label: 'Settings' },
  ];

  it('renders tabs with correct ARIA semantics', () => {
    render(<TabBar tabs={tabs} activeTab="status" onTabChange={() => {}} />);

    expect(screen.getByRole('tablist')).toBeDefined();

    const renderedTabs = screen.getAllByRole('tab');
    expect(renderedTabs).toHaveLength(4);
    expect(renderedTabs[0].getAttribute('aria-selected')).toBe('true');
    expect(renderedTabs[1].getAttribute('aria-selected')).toBe('false');
    expect(screen.getByText('3')).toBeDefined();
  });

  it('clicking a tab triggers onTabChange', () => {
    const onTabChange = vi.fn();
    render(<TabBar tabs={tabs} activeTab="status" onTabChange={onTabChange} />);

    fireEvent.click(screen.getByRole('tab', { name: 'History' }));
    expect(onTabChange).toHaveBeenCalledWith('history');
  });

  it('ArrowRight/ArrowLeft move focus and Enter/Space activate', () => {
    const onTabChange = vi.fn();
    render(<TabBar tabs={tabs} activeTab="status" onTabChange={onTabChange} />);

    const statusTab = screen.getByRole('tab', { name: 'Status' });
    const historyTab = screen.getByRole('tab', { name: 'History' });
    const settingsTab = screen.getByRole('tab', { name: 'Settings' });

    statusTab.focus();
    fireEvent.keyDown(statusTab, { key: 'ArrowRight' });
    expect(document.activeElement).toBe(historyTab);
    expect(onTabChange).not.toHaveBeenCalled();

    fireEvent.keyDown(historyTab, { key: 'Enter' });
    expect(onTabChange).toHaveBeenCalledWith('history');

    historyTab.focus();
    fireEvent.keyDown(historyTab, { key: 'ArrowLeft' });
    expect(document.activeElement).toBe(statusTab);

    fireEvent.keyDown(settingsTab, { key: ' ' });
    expect(onTabChange).toHaveBeenCalledWith('settings');
  });
});
