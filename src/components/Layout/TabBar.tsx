import { useRef } from 'react';
import type { KeyboardEvent } from 'react';

export interface TabBarTab {
  id: string;
  label: string;
  badge?: number;
}

export interface TabBarProps {
  tabs: TabBarTab[];
  activeTab: string;
  onTabChange: (tabId: string) => void;
}

export function TabBar({ tabs, activeTab, onTabChange }: TabBarProps) {
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const focusTabByIndex = (index: number) => {
    const boundedIndex = ((index % tabs.length) + tabs.length) % tabs.length;
    tabRefs.current[boundedIndex]?.focus();
  };

  const handleKeyDown = (
    event: KeyboardEvent<HTMLButtonElement>,
    index: number,
    tabId: string
  ) => {
    if (tabs.length === 0) return;

    switch (event.key) {
      case 'ArrowRight':
        event.preventDefault();
        focusTabByIndex(index + 1);
        break;
      case 'ArrowLeft':
        event.preventDefault();
        focusTabByIndex(index - 1);
        break;
      case 'Home':
        event.preventDefault();
        focusTabByIndex(0);
        break;
      case 'End':
        event.preventDefault();
        focusTabByIndex(tabs.length - 1);
        break;
      case 'Enter':
      case ' ':
        event.preventDefault();
        onTabChange(tabId);
        break;
      default:
        break;
    }
  };

  return (
    <div
      role="tablist"
      aria-label="Primary navigation"
      className="flex w-full items-stretch gap-1 rounded-lg border border-gray-700 bg-gray-900/80 p-1"
    >
      {tabs.map((tab, index) => {
        const isActive = tab.id === activeTab;
        return (
          <button
            key={tab.id}
            ref={(element) => {
              tabRefs.current[index] = element;
            }}
            id={`${tab.id}-tab`}
            type="button"
            role="tab"
            aria-selected={isActive}
            aria-controls={`panel-${tab.id}`}
            tabIndex={isActive ? 0 : -1}
            className={`flex min-w-0 flex-1 items-center justify-center gap-2 rounded-md px-3 py-2 text-sm font-medium transition-colors ${
              isActive
                ? 'bg-gray-700 text-white shadow-sm'
                : 'text-gray-300 hover:bg-gray-800 hover:text-white'
            }`}
            onClick={() => onTabChange(tab.id)}
            onKeyDown={(event) => handleKeyDown(event, index, tab.id)}
          >
            <span className="truncate">{tab.label}</span>
            {typeof tab.badge === 'number' && tab.badge > 0 ? (
              <span className="inline-flex h-5 min-w-[1.25rem] items-center justify-center rounded-full bg-sky-500 px-1 text-[11px] font-semibold text-white">
                {tab.badge}
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}

export default TabBar;
