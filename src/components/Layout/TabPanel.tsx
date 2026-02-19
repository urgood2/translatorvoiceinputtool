import type { ReactNode } from 'react';

export interface TabPanelProps {
  id: string;
  activeTab: string;
  children: ReactNode;
}

export function TabPanel({ id, activeTab, children }: TabPanelProps) {
  if (id !== activeTab) {
    return null;
  }

  return (
    <section
      id={`panel-${id}`}
      role="tabpanel"
      aria-labelledby={`${id}-tab`}
      className="h-full min-h-0 overflow-y-auto rounded-lg border border-gray-700 bg-gray-800/80 p-4"
    >
      {children}
    </section>
  );
}

export default TabPanel;
