"use client";

import Link from "next/link";
import { ReactNode, useEffect, useState } from "react";
import {
  capabilityLabel,
  hasWorkspaceCapability,
  readSession,
  type SessionRecord,
  type WorkspaceCapability,
} from "@/lib/app-state";
import { EmptyState } from "@/components/ui/empty-state";

export function RoleGuard({
  capability,
  children,
  title,
  description,
  fallbackHref = "/app/dashboard",
}: {
  capability: WorkspaceCapability;
  children: ReactNode;
  title?: string;
  description?: string;
  fallbackHref?: string;
}) {
  const [session, setSession] = useState<SessionRecord | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    void readSession()
      .then((nextSession) => {
        if (cancelled) return;
        setSession(nextSession);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (loading) {
    return (
      <section className="surface">
        <p>Checking workspace permissions...</p>
      </section>
    );
  }

  if (!session) {
    return (
      <section className="surface">
        <EmptyState
          title="Sign in required"
          description="You need an active workspace session to access this page."
          actionHref="/login"
          actionLabel="Go to login"
        />
      </section>
    );
  }

  if (!hasWorkspaceCapability(session.role, capability)) {
    return (
      <section className="surface warn-surface">
        <h3>{title ?? "Access limited for your role"}</h3>
        <p>
          {description ??
            `This page requires ${capabilityLabel(capability)}. Your current role is ${session.role}.`}
        </p>
        <div className="top-link-row">
          <Link href={fallbackHref}>Open allowed workspace page</Link>
        </div>
      </section>
    );
  }

  return <>{children}</>;
}
