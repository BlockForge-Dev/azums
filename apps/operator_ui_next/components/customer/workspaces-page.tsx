"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import {
  canManageWorkspace,
  getUsageSummary,
  listApiKeys,
  listTeamMembers,
  listWorkspaces,
  readSession,
  switchWorkspace,
  type ApiKeyRecord,
  type TeamMemberRecord,
  type WorkspaceRecord,
} from "@/lib/app-state";
import { formatMs } from "@/lib/client-api";
import { EmptyState } from "@/components/ui/empty-state";

function middleEllipsis(value: string, start = 16, end = 10) {
  if (!value || value.length <= start + end + 3) return value;
  return `${value.slice(0, start)}...${value.slice(-end)}`;
}

type ActivityRow = {
  when: number;
  action: string;
  detail: string;
};

export function WorkspacesPage() {
  const [session, setSession] = useState<Awaited<ReturnType<typeof readSession>>>(null);
  const [usage, setUsage] = useState<Awaited<ReturnType<typeof getUsageSummary>>>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceRecord[]>([]);
  const [teamMembers, setTeamMembers] = useState<TeamMemberRecord[]>([]);
  const [apiKeys, setApiKeys] = useState<ApiKeyRecord[]>([]);
  const [switching, setSwitching] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    setLoading(true);

    void Promise.all([
      readSession(),
      getUsageSummary().catch(() => null),
      listWorkspaces().catch(() => []),
    ])
      .then(([currentSession, usageSummary, memberships]) => {
        if (cancelled) return;

        setSession(currentSession);
        setUsage(usageSummary);
        setWorkspaces(memberships);

        if (!currentSession || !canManageWorkspace(currentSession.role)) {
          return;
        }

        void Promise.all([
          listTeamMembers().catch(() => []),
          listApiKeys().catch(() => []),
        ]).then(([members, keys]) => {
          if (cancelled) return;
          setTeamMembers(members);
          setApiKeys(keys);
        });
      })
      .catch((loadError: unknown) => {
        if (cancelled) return;
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const activeWorkspace = useMemo(
    () => workspaces.find((row) => row.is_current) ?? null,
    [workspaces]
  );

  const workspaceSettingsHref = useMemo(() => {
    const id = activeWorkspace?.workspace_id ?? session?.workspace_id;
    return id ? `/app/workspaces/${encodeURIComponent(id)}` : "/app/workspaces";
  }, [activeWorkspace?.workspace_id, session?.workspace_id]);

  const activeKeyCount = useMemo(
    () => apiKeys.filter((key) => key.revoked_at_ms == null).length,
    [apiKeys]
  );

  const activityRows = useMemo<ActivityRow[]>(() => {
    const membershipEvents = teamMembers.map((member) => ({
      when: member.added_at_ms,
      action: member.status === "invited" ? "Member invited" : "Member added",
      detail: `${member.email} (${member.role})`,
    }));

    const keyEvents = apiKeys.map((key) => ({
      when: key.revoked_at_ms ?? key.created_at_ms,
      action: key.revoked_at_ms ? "API key revoked" : "API key created",
      detail: key.name,
    }));

    return [...membershipEvents, ...keyEvents]
      .sort((a, b) => b.when - a.when)
      .slice(0, 20);
  }, [apiKeys, teamMembers]);

  async function onSwitch(workspaceId: string) {
    setSwitching(workspaceId);
    setError(null);

    try {
      const updatedSession = await switchWorkspace({ workspace_id: workspaceId });
      setSession(updatedSession);

      const memberships = await listWorkspaces();
      setWorkspaces(memberships);
    } catch (switchError: unknown) {
      setError(switchError instanceof Error ? switchError.message : String(switchError));
    } finally {
      setSwitching(null);
    }
  }

  return (
    <div className="space-y-6">
      <section className="bg-gradient-to-br from-primary/20 via-card to-card rounded-2xl p-8 border border-primary/20">
        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-6">
          <div>
            <p className="text-sm font-medium text-primary mb-2">Workspaces</p>
            <h2 className="text-2xl font-bold text-foreground mb-2">Manage your workspace and environment access.</h2>
            <p className="text-muted-foreground max-w-lg">
              Switch between environments, review team access, and keep setup
              aligned with the right workspace.
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            <span className="badge badge-neutral">
              {activeWorkspace?.environment?.toUpperCase() ?? "WORKSPACE"}
            </span>
            <span className="badge badge-neutral">{session?.role ?? "-"}</span>
            <span className="badge badge-green">
              {usage ? `${usage.plan} / ${usage.access_mode === "paid" ? "paid" : "free"}` : "Loading..."}
            </span>
          </div>
        </div>
      </section>

      {error ? <section className="bg-destructive/10 border border-destructive/30 rounded-xl p-4 text-destructive">{error}</section> : null}

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-6">
          <div>
            <h3 className="text-lg font-semibold text-foreground">Current workspace</h3>
            <p className="text-sm text-muted-foreground mt-1">The workspace your current session is using.</p>
          </div>

          <Link className="btn btn-ghost btn-sm" href={workspaceSettingsHref}>
            Open settings
          </Link>
        </div>

        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <article className="bg-muted/30 rounded-lg p-4">
            <span className="text-xs text-muted-foreground">Name</span>
            <strong className="block text-sm text-foreground mt-1" title={activeWorkspace?.workspace_name ?? session?.workspace_name ?? "-"}>
              {activeWorkspace?.workspace_name ?? session?.workspace_name ?? "-"}
            </strong>
            <small className="text-xs text-muted-foreground">Workspace display name</small>
          </article>

          <article className="bg-muted/30 rounded-lg p-4">
            <span className="text-xs text-muted-foreground">Workspace ID</span>
            <strong className="block text-sm text-foreground font-mono mt-1" title={activeWorkspace?.workspace_id ?? session?.workspace_id ?? "-"}>
              {activeWorkspace?.workspace_id
                ? middleEllipsis(activeWorkspace.workspace_id)
                : session?.workspace_id
                  ? middleEllipsis(session.workspace_id)
                  : "-"}
            </strong>
            <small className="text-xs text-muted-foreground">Workspace identifier</small>
          </article>

          <article className="bg-muted/30 rounded-lg p-4">
            <span className="text-xs text-muted-foreground">Tenant ID</span>
            <strong className="block text-sm text-foreground font-mono mt-1" title={activeWorkspace?.tenant_id ?? session?.tenant_id ?? "-"}>
              {activeWorkspace?.tenant_id
                ? middleEllipsis(activeWorkspace.tenant_id)
                : session?.tenant_id
                  ? middleEllipsis(session.tenant_id)
                  : "-"}
            </strong>
            <small className="text-xs text-muted-foreground">Tenant linked to this workspace</small>
          </article>

          <article className="bg-muted/30 rounded-lg p-4">
            <span className="text-xs text-muted-foreground">Your role</span>
            <strong className="block text-sm text-foreground mt-1">{activeWorkspace?.role ?? session?.role ?? "-"}</strong>
            <small className="text-xs text-muted-foreground">Access level in this workspace</small>
          </article>

          <article className="bg-muted/30 rounded-lg p-4">
            <span className="text-xs text-muted-foreground">Plan</span>
            <strong className="block text-sm text-foreground mt-1">{usage?.plan ?? "-"}</strong>
            <small className="text-xs text-muted-foreground">Current workspace plan</small>
          </article>

          <article className="bg-muted/30 rounded-lg p-4">
            <span className="text-xs text-muted-foreground">Access mode</span>
            <strong className="block text-sm text-foreground mt-1">{usage ? (usage.access_mode === "paid" ? "Paid" : "Free") : "-"}</strong>
            <small className="text-xs text-muted-foreground">How this workspace is currently billed</small>
          </article>

          <article className="bg-muted/30 rounded-lg p-4">
            <span className="text-xs text-muted-foreground">Joined</span>
            <strong className="block text-sm text-foreground mt-1">{formatMs(session?.created_at_ms ?? null)}</strong>
            <small className="text-xs text-muted-foreground">Session membership start</small>
          </article>

          <article className="bg-muted/30 rounded-lg p-4">
            <span className="text-xs text-muted-foreground">Active keys</span>
            <strong className="block text-sm text-foreground mt-1">{activeKeyCount}</strong>
            <small className="text-xs text-muted-foreground">Keys available for this workspace</small>
          </article>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground">Switch workspace</h3>
          <p className="text-sm text-muted-foreground mt-1">
            Move between environments without losing track of where you are working.
          </p>
        </div>

        {workspaces.length === 0 ? (
          <EmptyState
            compact
            title="No linked workspaces"
            description="This account is currently linked to a single workspace context."
          />
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {workspaces.map((workspace) => {
              const isActive = workspace.is_current;

              return (
                <article
                  key={workspace.workspace_id}
                  data-testid={`workspace-card-${workspace.workspace_id}`}
                  className={`rounded-xl border p-5 transition-all ${isActive ? "bg-primary/10 border-primary/30" : "bg-muted/20 border-border/50 hover:border-primary/30"}`}
                >
                  <div className="flex items-center justify-between mb-3">
                    <h4 className="text-sm font-semibold text-foreground">{workspace.environment.toUpperCase()}</h4>
                    <span className={`badge ${isActive ? "badge-green" : "badge-neutral"}`}>
                      {isActive ? "Current" : workspace.role}
                    </span>
                  </div>

                  <p className="text-foreground font-medium mb-3">{workspace.workspace_name}</p>

                  <div className="text-xs text-muted-foreground mb-4 space-y-1">
                    <span className="block" title={workspace.tenant_id}>
                      Tenant: {middleEllipsis(workspace.tenant_id, 12, 8)}
                    </span>
                    <span className="block" title={workspace.workspace_id}>
                      ID: {middleEllipsis(workspace.workspace_id, 12, 8)}
                    </span>
                  </div>

                  <div className="flex flex-col gap-2">
                    {isActive ? (
                      <span className="text-xs text-primary">You are currently in this workspace.</span>
                    ) : (
                      <>
                        <button
                          data-testid={`workspace-switch-${workspace.workspace_id}`}
                          className="btn btn-primary btn-sm w-full"
                          type="button"
                          onClick={() => void onSwitch(workspace.workspace_id)}
                          disabled={switching === workspace.workspace_id}
                        >
                          {switching === workspace.workspace_id ? "Switching..." : "Switch here"}
                        </button>

                        <Link
                          className="btn btn-ghost btn-sm w-full"
                          href={`/app/workspaces/${encodeURIComponent(workspace.workspace_id)}`}
                        >
                          Open detail
                        </Link>
                      </>
                    )}
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground">Team access</h3>
          <p className="text-sm text-muted-foreground mt-1">People and roles for the current workspace.</p>
        </div>

        {teamMembers.length === 0 ? (
          <EmptyState
            compact
            title="No team details available"
            description="Owner or admin access is required to view team membership here."
            actionHref="/app/team"
            actionLabel="Open team"
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="bg-muted/50">
                <tr>
                  <th className="text-left px-4 py-3 font-medium text-muted-foreground">Email</th>
                  <th className="text-left px-4 py-3 font-medium text-muted-foreground">Role</th>
                  <th className="text-left px-4 py-3 font-medium text-muted-foreground">Status</th>
                  <th className="text-left px-4 py-3 font-medium text-muted-foreground">Added</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/50">
                {teamMembers.map((member) => (
                  <tr key={member.id} className="hover:bg-muted/30 transition-colors">
                    <td className="px-4 py-3 text-foreground">{member.email}</td>
                    <td className="px-4 py-3 text-foreground">{member.role}</td>
                    <td className="px-4 py-3 text-foreground">{member.status}</td>
                    <td className="px-4 py-3 text-foreground">{formatMs(member.added_at_ms)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        <div className="flex flex-wrap gap-3 mt-6">
          <Link href="/app/team" className="btn btn-ghost btn-sm">
            Open team
          </Link>
          <Link href={workspaceSettingsHref} className="btn btn-ghost btn-sm">
            Open environment settings
          </Link>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground">Recent workspace activity</h3>
          <p className="text-sm text-muted-foreground mt-1">
            Member and API key changes appear here.
          </p>
        </div>

        {activityRows.length === 0 ? (
          <EmptyState
            compact
            title="No recent activity"
            description="Recent membership and API key actions will appear here."
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="bg-muted/50">
                <tr>
                  <th className="text-left px-4 py-3 font-medium text-muted-foreground">When</th>
                  <th className="text-left px-4 py-3 font-medium text-muted-foreground">Action</th>
                  <th className="text-left px-4 py-3 font-medium text-muted-foreground">Detail</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/50">
                {activityRows.map((row, index) => (
                  <tr key={`${row.action}-${row.when}-${index}`} className="hover:bg-muted/30 transition-colors">
                    <td className="px-4 py-3 text-foreground">{formatMs(row.when)}</td>
                    <td className="px-4 py-3 text-foreground">{row.action}</td>
                    <td className="px-4 py-3 text-foreground">{row.detail}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground">Quick actions</h3>
          <p className="text-sm text-muted-foreground mt-1">Move to the next things that usually matter.</p>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          <Link className="group rounded-xl border border-primary/30 bg-primary/5 p-5 hover:bg-primary/10 transition-colors" href="/app/playground">
            <h4 className="font-semibold text-foreground mb-1 group-hover:text-primary transition-colors">Run a test request</h4>
            <p className="text-sm text-muted-foreground">Send a request in Playground and check the result.</p>
          </Link>

          <Link className="group rounded-xl border border-border/50 bg-muted/20 p-5 hover:border-primary/30 transition-colors" href="/app/webhooks">
            <h4 className="font-semibold text-foreground mb-1">Set up webhooks</h4>
            <p className="text-sm text-muted-foreground">Configure inbound webhook headers and test events.</p>
          </Link>

          <Link className="group rounded-xl border border-border/50 bg-muted/20 p-5 hover:border-primary/30 transition-colors" href="/app/callbacks">
            <h4 className="font-semibold text-foreground mb-1">Configure callbacks</h4>
            <p className="text-sm text-muted-foreground">Choose where request updates should be delivered.</p>
          </Link>

          <Link className="group rounded-xl border border-border/50 bg-muted/20 p-5 hover:border-primary/30 transition-colors" href="/app/workspaces">
            <h4 className="font-semibold text-foreground mb-1">Review usage</h4>
            <p className="text-sm text-muted-foreground">Check workspace usage, settings, and billing posture.</p>
          </Link>
        </div>
      </section>
    </div>
  );
}
