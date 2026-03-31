"use client";

import { ReactNode, useEffect, useMemo, useState } from "react";
import { usePathname, useRouter } from "next/navigation";
import {
  listWorkspaces,
  readSession,
  switchWorkspace,
  type SessionRecord,
} from "@/lib/app-state";
import { ShellLayout } from "@/components/shell/shell-layout";

interface AppLayoutProps {
  children: ReactNode;
}

export default function CustomerLayout({ children }: AppLayoutProps) {
  const pathname = usePathname();
  const router = useRouter();
  const [session, setSession] = useState<SessionRecord | null>(null);
  const [workspaceId, setWorkspaceId] = useState<string>("");
  const [ready, setReady] = useState(false);
  const [workspaces, setWorkspaces] = useState<
    Array<{
      id: string;
      name: string;
    }>
  >([]);

  useEffect(() => {
    let cancelled = false;

    void Promise.all([readSession(), listWorkspaces().catch(() => [])])
      .then(([nextSession, nextWorkspaces]) => {
        if (cancelled) return;

        setSession(nextSession);
        const mapped = nextWorkspaces.map((workspace) => ({
          id: workspace.workspace_id,
          name: workspace.workspace_name,
        }));
        setWorkspaces(mapped);
        setWorkspaceId(nextSession?.workspace_id ?? mapped[0]?.id ?? "");
      })
      .finally(() => {
        if (!cancelled) {
          setReady(true);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!ready || session) return;
    const next = encodeURIComponent(pathname || "/app/dashboard");
    router.replace(`/login?next=${next}`);
  }, [pathname, ready, router, session]);

  const workspaceName = useMemo(() => {
    if (session?.workspace_name) {
      return session.workspace_name;
    }
    return workspaces.find((workspace) => workspace.id === workspaceId)?.name;
  }, [session?.workspace_name, workspaceId, workspaces]);

  async function handleWorkspaceChange(nextWorkspaceId: string) {
    if (!nextWorkspaceId || nextWorkspaceId === workspaceId) return;

    const previousWorkspaceId = workspaceId;
    setWorkspaceId(nextWorkspaceId);

    try {
      const nextSession = await switchWorkspace({ workspace_id: nextWorkspaceId });
      setSession(nextSession);
      const nextWorkspaces = await listWorkspaces().catch(() => []);
      setWorkspaces(
        nextWorkspaces.map((workspace) => ({
          id: workspace.workspace_id,
          name: workspace.workspace_name,
        }))
      );
      setWorkspaceId(nextSession.workspace_id);
      router.refresh();
    } catch {
      setWorkspaceId(previousWorkspaceId);
    }
  }

  return (
    <ShellLayout
      activePath={pathname || ""}
      workspaceName={workspaceName}
      user={
        session
          ? {
              email: session.email,
              name: session.full_name,
            }
          : undefined
      }
      workspaces={workspaces}
      selectedWorkspaceId={workspaceId}
      onWorkspaceChange={(nextWorkspaceId) => {
        void handleWorkspaceChange(nextWorkspaceId);
      }}
    >
      {children}
    </ShellLayout>
  );
}
