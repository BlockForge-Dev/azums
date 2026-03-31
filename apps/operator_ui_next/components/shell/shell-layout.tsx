"use client";

import { ReactNode } from "react";
import { Sidebar } from "./sidebar";
import { TopNav } from "./top-nav";

export interface ShellLayoutProps {
  children: ReactNode;
  activePath: string;
  workspaceName?: string;
  selectedWorkspaceId?: string;
  user?: {
    email: string;
    name?: string;
  };
  onWorkspaceChange?: (workspaceId: string) => void;
  workspaces?: Array<{
    id: string;
    name: string;
  }>;
}

export function ShellLayout({
  children,
  activePath,
  workspaceName,
  selectedWorkspaceId,
  user,
  onWorkspaceChange,
  workspaces,
}: ShellLayoutProps) {
  return (
    <div className="flex min-h-screen bg-background">
      <Sidebar
        activePath={activePath}
        workspaceName={workspaceName}
        workspaces={workspaces}
        selectedWorkspaceId={selectedWorkspaceId}
        onWorkspaceChange={onWorkspaceChange}
      />
      <div className="flex-1 flex flex-col min-w-0">
        <TopNav user={user} />
        <main className="flex-1 p-6 overflow-auto">
          {children}
        </main>
      </div>
    </div>
  );
}
