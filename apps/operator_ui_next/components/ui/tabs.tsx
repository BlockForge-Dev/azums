import { ReactNode, createContext, useContext, useState } from "react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface Tab {
  id: string;
  label: string;
  icon?: ReactNode;
}

interface TabsContextValue {
  activeTab: string;
  setActiveTab: (id: string) => void;
}

const TabsContext = createContext<TabsContextValue | null>(null);

function useTabsContext() {
  const context = useContext(TabsContext);
  if (!context) {
    throw new Error("Tabs components must be used within a Tabs provider");
  }
  return context;
}

export interface TabsProps {
  tabs: Tab[];
  defaultTab: string;
  children: ReactNode;
  onChange?: (tabId: string) => void;
  className?: string;
}

export function Tabs({
  tabs,
  defaultTab,
  children,
  onChange,
  className,
}: TabsProps) {
  const [activeTab, setActiveTab] = useState(defaultTab);

  const handleTabChange = (id: string) => {
    setActiveTab(id);
    onChange?.(id);
  };

  return (
    <TabsContext.Provider value={{ activeTab, setActiveTab: handleTabChange }}>
      <div className={twMerge("", className)}>
        <TabsList tabs={tabs} />
        {children}
      </div>
    </TabsContext.Provider>
  );
}

interface TabsListProps {
  tabs: Tab[];
}

export function TabsList({ tabs }: TabsListProps) {
  const { activeTab, setActiveTab } = useTabsContext();

  return (
    <div className="tabs">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          onClick={() => setActiveTab(tab.id)}
          className={clsx(
            "tab",
            activeTab === tab.id && "active"
          )}
        >
          {tab.icon && <span className="mr-2">{tab.icon}</span>}
          {tab.label}
        </button>
      ))}
    </div>
  );
}

export interface TabPanelProps {
  tabId: string;
  children: ReactNode;
  className?: string;
}

export function TabPanel({ tabId, children, className }: TabPanelProps) {
  const { activeTab } = useTabsContext();

  if (activeTab !== tabId) {
    return null;
  }

  return (
    <div className={twMerge("tab-body", className)}>
      {children}
    </div>
  );
}
