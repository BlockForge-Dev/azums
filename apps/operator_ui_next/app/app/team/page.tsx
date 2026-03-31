"use client";

import { FormEvent, useEffect, useState } from "react";
import {
  canManageWorkspace,
  inviteTeamMember,
  listTeamMembers,
  readSession,
  removeTeamMember,
  updateTeamRole,
  type TeamMemberRecord,
  type WorkspaceRole,
} from "@/lib/app-state";
import { formatMs } from "@/lib/client-api";
import { apiGet } from "@/lib/client-api";
import type { UiConfigResponse } from "@/lib/types";

export default function Page() {
  const [config, setConfig] = useState<UiConfigResponse | null>(null);
  const [members, setMembers] = useState<TeamMemberRecord[]>([]);
  const [session, setSession] = useState<Awaited<ReturnType<typeof readSession>>>(null);
  const [inviteEmail, setInviteEmail] = useState("");
  const [inviteRole, setInviteRole] = useState<WorkspaceRole>("developer");
  const [inviteLink, setInviteLink] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const canManage = session ? canManageWorkspace(session.role) : false;

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void Promise.all([apiGet<UiConfigResponse>("config"), readSession()])
      .then(async ([cfg, currentSession]) => {
        if (cancelled) return;
        setConfig(cfg);
        setSession(currentSession);
        if (!currentSession || !canManageWorkspace(currentSession.role)) {
          setMembers([]);
          return;
        }
        const teamRows = await listTeamMembers();
        if (cancelled) return;
        setMembers(teamRows);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function refreshMembers() {
    const data = await listTeamMembers();
    setMembers(data);
  }

  async function invite(event: FormEvent) {
    event.preventDefault();
    if (!canManage) {
      setError("Only workspace owner/admin can invite members.");
      return;
    }
    setError(null);
    setMessage(null);
    setInviteLink(null);
    try {
      if (!inviteEmail.trim()) {
        throw new Error("Invite email is required.");
      }
      const out = await inviteTeamMember(inviteEmail, inviteRole);
      await refreshMembers();
      setInviteEmail("");
      const origin = typeof window !== "undefined" ? window.location.origin : "";
      setInviteLink(`${origin}${out.invite_path}`);
      setMessage("Member invited. Share the acceptance link.");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function remove(id: string) {
    if (!canManage) {
      setError("Only workspace owner/admin can remove members.");
      return;
    }
    if (!window.confirm("Remove this member from workspace?")) return;
    setError(null);
    try {
      await removeTeamMember(id);
      await refreshMembers();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function setRole(id: string, role: WorkspaceRole) {
    if (!canManage) {
      setError("Only workspace owner/admin can update roles.");
      return;
    }
    setError(null);
    try {
      await updateTeamRole(id, role);
      await refreshMembers();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div className="flex flex-col gap-6 p-6 max-w-7xl mx-auto">
      <section className="bg-gradient-to-br from-card to-card/80 rounded-xl border border-border/50 p-6">
        <p className="text-sm font-medium text-muted-foreground mb-1">Team Settings</p>
        <h2 className="text-2xl font-semibold text-foreground">Workspace & Team Access</h2>
        <p className="text-muted-foreground mt-1">Manage workspace members and role assignments.</p>
      </section>

      {error ? <section className="bg-red-500/10 border border-red-500/30 text-red-400 rounded-lg p-4">{error}</section> : null}
      {message ? <section className="bg-green-500/10 border border-green-500/30 text-green-400 rounded-lg p-4">{message}</section> : null}
      {inviteLink ? (
        <section className="bg-green-500/10 border border-green-500/30 rounded-lg p-4">
          <h3 className="text-green-400 font-semibold mb-2">Invite Acceptance Link</h3>
          <p className="text-green-400/80 text-sm mb-3">Send this link to the invited member:</p>
          <code className="text-green-400 block mb-3 text-sm break-all">{inviteLink}</code>
          <button
            className="px-3 py-1.5 text-sm font-medium text-green-400 hover:bg-green-500/20 rounded-md transition-colors"
            type="button"
            onClick={() => void navigator.clipboard.writeText(inviteLink)}
          >
            Copy invite link
          </button>
        </section>
      ) : null}

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Workspace</span>
            <strong className="text-foreground block mt-1">{session?.workspace_name ?? "-"}</strong>
          </div>
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Workspace ID</span>
            <code className="text-foreground block mt-1 text-sm">{session?.workspace_id ?? "-"}</code>
          </div>
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Signed-in User</span>
            <strong className="text-foreground block mt-1">{session?.email ?? "-"}</strong>
          </div>
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Tenant</span>
            <strong className="text-foreground block mt-1">
              {config?.tenant_id ?? "-"}
            </strong>
          </div>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">Invite member</h3>
        {!canManage ? (
          <p className="text-sm text-yellow-400 bg-yellow-500/10 px-3 py-2 rounded-lg mb-4">Read-only for your role. Owner/admin can manage team access.</p>
        ) : null}
        <form className="flex flex-wrap items-end gap-4" onSubmit={(event) => void invite(event)}>
          <label className="flex flex-col gap-2 flex-1 min-w-[200px]">
            <span className="text-sm font-medium text-foreground">Member email</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              type="email"
              value={inviteEmail}
              onChange={(event) => setInviteEmail(event.target.value)}
              placeholder="dev@company.com"
              disabled={!canManage}
            />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Role</span>
            <select
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              value={inviteRole}
              onChange={(event) => setInviteRole(event.target.value as WorkspaceRole)}
              disabled={!canManage}
            >
              <option value="admin">admin</option>
              <option value="developer">developer</option>
              <option value="viewer">viewer</option>
            </select>
          </label>
          <button className="px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed" type="submit" disabled={loading || !canManage}>
            {loading ? "Working..." : "Invite"}
          </button>
        </form>
      </section>

      <section className="bg-card rounded-xl border border-border/50 overflow-hidden">
        <h3 className="text-lg font-semibold text-foreground p-6 pb-0">Members</h3>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Email</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Role</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Status</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Invite Expires</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Added</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {members.length === 0 ? (
                <tr>
                  <td colSpan={6} className="px-4 py-6 text-center text-muted-foreground">No team members yet. Invite your first teammate.</td>
                </tr>
              ) : (
                members.map((member) => (
                  <tr key={member.id} className="hover:bg-muted/30 transition-colors">
                    <td className="px-4 py-3 text-foreground">{member.email}</td>
                    <td className="px-4 py-3">
                      <select
                        className="px-2 py-1 bg-background border border-border rounded text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                        value={member.role}
                        onChange={(event) =>
                          void setRole(member.id, event.target.value as WorkspaceRole)
                        }
                        disabled={!canManage}
                      >
                        <option value="owner">owner</option>
                        <option value="admin">admin</option>
                        <option value="developer">developer</option>
                        <option value="viewer">viewer</option>
                      </select>
                    </td>
                    <td className="px-4 py-3 text-foreground">{member.status}</td>
                    <td className="px-4 py-3 text-foreground">{formatMs(member.invite_expires_at_ms ?? null)}</td>
                    <td className="px-4 py-3 text-foreground">{formatMs(member.added_at_ms)}</td>
                    <td className="px-4 py-3">
                      {member.role === "owner" ? (
                        "-"
                      ) : (
                        <button
                          className="px-2 py-1 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                          type="button"
                          disabled={!canManage}
                          onClick={() => void remove(member.id)}
                        >
                          Remove
                        </button>
                      )}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
