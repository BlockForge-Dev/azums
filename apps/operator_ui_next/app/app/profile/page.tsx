"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { readSession } from "@/lib/app-state";
import { formatMs } from "@/lib/client-api";

export default function Page() {
  const [session, setSession] = useState<Awaited<ReturnType<typeof readSession>>>(null);

  useEffect(() => {
    void readSession().then((current) => setSession(current));
  }, []);

  return (
    <div className="space-y-6">
      <section className="bg-gradient-to-br from-primary/20 via-card to-card rounded-2xl p-8 border border-primary/20">
        <p className="text-sm font-medium text-primary mb-2">Profile</p>
        <h2 className="text-2xl font-bold text-foreground mb-2">User Identity</h2>
        <p className="text-muted-foreground">Account and workspace identity summary for the active session.</p>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          <div>
            <span className="text-sm text-muted-foreground">Name</span>
            <strong className="block text-lg text-foreground">{session?.full_name ?? "-"}</strong>
          </div>
          <div>
            <span className="text-sm text-muted-foreground">Email</span>
            <strong className="block text-lg text-foreground">{session?.email ?? "-"}</strong>
          </div>
          <div>
            <span className="text-sm text-muted-foreground">Role</span>
            <strong className="block text-lg text-foreground">{session?.role ?? "-"}</strong>
          </div>
          <div>
            <span className="text-sm text-muted-foreground">Workspace</span>
            <strong className="block text-lg text-foreground">{session?.workspace_name ?? "-"}</strong>
          </div>
          <div>
            <span className="text-sm text-muted-foreground">Tenant</span>
            <strong className="block text-lg text-foreground font-mono text-sm">{session?.tenant_id ?? "-"}</strong>
          </div>
          <div>
            <span className="text-sm text-muted-foreground">Member since</span>
            <strong className="block text-lg text-foreground">{formatMs(session?.created_at_ms ?? null)}</strong>
          </div>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">Security & access</h3>
        <div className="flex flex-wrap gap-3">
          <Link href="/security" className="btn btn-ghost">
            Security guide
          </Link>
          <Link href="/app/api-keys" className="btn btn-ghost">
            API tokens
          </Link>
          <Link href="/app/workspaces" className="btn btn-ghost">
            Workspace settings
          </Link>
        </div>
      </section>
    </div>
  );
}
