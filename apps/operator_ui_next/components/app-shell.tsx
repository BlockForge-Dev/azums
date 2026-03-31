"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { type FormEvent, ReactNode, useEffect, useMemo, useRef, useState } from "react";
import {
  canAccessOperator,
  canManageWorkspace,
  canViewBilling,
  canWriteRequests,
  clearSession,
  listWorkspaces,
  onboardingProgress,
  readSession,
  switchWorkspace,
  type WorkspaceRole,
  type SessionRecord,
  type WorkspaceRecord,
} from "@/lib/app-state";
import { apiGet, shortId } from "@/lib/client-api";
import type {
  AdminOverviewResponse,
  NotificationItem,
  NotificationStreamEnvelope,
  SearchResponse,
  SearchResultItem,
  UiConfigResponse,
  UiHealthResponse,
} from "@/lib/types";

type ShellKind = "customer" | "operator" | "platform";
type SurfaceScope = "customer" | "customer_operator" | "platform";

type NavItem = {
  href: string;
  label: string;
  visible?: (role: WorkspaceRole) => boolean;
};

const QUICK_NAV_ITEMS: SearchResultItem[] = [
  {
    kind: "navigation",
    object_id: "nav-playground",
    title: "Playground",
    subtitle: "Run and inspect full lifecycle in sandbox.",
    href: "/app/playground",
    updated_at_ms: 0,
    score: 50,
  },
  {
    kind: "navigation",
    object_id: "nav-dashboard",
    title: "Dashboard",
    subtitle: "Workspace metrics and quick actions.",
    href: "/app/dashboard",
    updated_at_ms: 0,
    score: 40,
  },
  {
    kind: "navigation",
    object_id: "nav-requests",
    title: "Requests",
    subtitle: "Browse execution requests and states.",
    href: "/app/requests",
    updated_at_ms: 0,
    score: 40,
  },
  {
    kind: "navigation",
    object_id: "nav-callbacks",
    title: "Callbacks",
    subtitle: "Outbound callback destinations and delivery history.",
    href: "/app/callbacks",
    updated_at_ms: 0,
    score: 40,
  },
  {
    kind: "navigation",
    object_id: "nav-webhooks",
    title: "Inbound Webhooks",
    subtitle: "Receiver endpoint, signature verification, and intake audits.",
    href: "/app/webhooks",
    updated_at_ms: 0,
    score: 40,
  },
  {
    kind: "navigation",
    object_id: "nav-api-keys",
    title: "API Keys",
    subtitle: "Create/revoke keys and copy one-time credentials.",
    href: "/app/api-keys",
    updated_at_ms: 0,
    score: 40,
  },
  {
    kind: "navigation",
    object_id: "nav-workspace",
    title: "Workspace",
    subtitle: "Workspace profile, environments, members, and settings.",
    href: "/app/workspaces",
    updated_at_ms: 0,
    score: 40,
  },
];

const CUSTOMER_NAV: NavItem[] = [
  { href: "/app/onboarding", label: "Onboarding" },
  { href: "/app/dashboard", label: "Dashboard" },
  { href: "/app/playground", label: "Playground", visible: canWriteRequests },
  { href: "/app/requests", label: "Requests" },
  { href: "/app/callbacks", label: "Callbacks", visible: canWriteRequests },
  { href: "/app/webhooks", label: "Inbound Webhooks", visible: canManageWorkspace },
  { href: "/app/api-keys", label: "API Keys", visible: canWriteRequests },
  { href: "/app/usage", label: "Usage" },
  { href: "/app/billing", label: "Billing", visible: canViewBilling },
  { href: "/app/workspaces", label: "Workspace" },
  { href: "/app/docs", label: "Docs" },
];

const OPERATOR_NAV: NavItem[] = [
  { href: "/ops", label: "Overview" },
  { href: "/ops/jobs", label: "Jobs" },
  { href: "/ops/replay", label: "Replay" },
  { href: "/ops/dead-letters", label: "Dead Letters" },
  { href: "/ops/deliveries", label: "Deliveries" },
  { href: "/ops/intake-audits", label: "Intake Audits" },
  { href: "/ops/adapter-health", label: "Adapter Health" },
  { href: "/ops/security", label: "Security" },
  { href: "/ops/workspaces", label: "Workspaces" },
  { href: "/ops/activity", label: "Activity" },
];

const PLATFORM_NAV: NavItem[] = [
  { href: "/admin", label: "Overview" },
  { href: "/admin/tenants", label: "Tenants" },
  { href: "/admin/workspaces", label: "Workspaces" },
  { href: "/admin/dead-letters", label: "Dead Letters" },
  { href: "/admin/incidents", label: "Incidents" },
  { href: "/admin/adapter-health", label: "Adapter Health" },
];

export function AppShell({
  kind,
  children,
}: {
  kind: ShellKind;
  children: ReactNode;
}) {
  const pathname = usePathname();
  const router = useRouter();
  const [config, setConfig] = useState<UiConfigResponse | null>(null);
  const [health, setHealth] = useState<UiHealthResponse | null>(null);
  const [session, setSession] = useState<SessionRecord | null>(null);
  const [sessionReady, setSessionReady] = useState(false);
  const [workspaces, setWorkspaces] = useState<WorkspaceRecord[]>([]);
  const [switchingWorkspace, setSwitchingWorkspace] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [commandOpen, setCommandOpen] = useState(false);
  const [commandQuery, setCommandQuery] = useState("");
  const [commandLoading, setCommandLoading] = useState(false);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [commandResults, setCommandResults] = useState<SearchResultItem[]>([]);
  const [commandCursor, setCommandCursor] = useState(0);
  const commandInputRef = useRef<HTMLInputElement | null>(null);
  const [notificationsOpen, setNotificationsOpen] = useState(false);
  const [notificationsLoading, setNotificationsLoading] = useState(false);
  const [notificationsStatus, setNotificationsStatus] = useState<
    "connecting" | "live" | "reconnecting" | "down"
  >("connecting");
  const [notifications, setNotifications] = useState<NotificationItem[]>([]);
  const [platformEnvironment, setPlatformEnvironment] = useState<
    "all" | "sandbox" | "staging" | "production"
  >("all");
  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const userMenuRef = useRef<HTMLDivElement | null>(null);
  const surfaceScope = useMemo<SurfaceScope>(() => {
    if (pathname.startsWith("/admin") || kind === "platform") return "platform";
    if (kind === "operator" || pathname.startsWith("/ops")) return "customer_operator";
    return "customer";
  }, [kind, pathname]);

  useEffect(() => {
    let cancelled = false;
    Promise.all([apiGet<UiConfigResponse>("config"), apiGet<UiHealthResponse>("health")])
      .then(([cfg, hlth]) => {
        if (cancelled) return;
        setConfig(cfg);
        setHealth(hlth);
      })
      .catch(() => {
        if (!cancelled) {
          setConfig(null);
          setHealth(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!sessionReady || !session) return;

    if (surfaceScope === "platform") {
      let active = true;
      let intervalId: number | undefined;
      async function pollPlatformNotifications() {
        setNotificationsLoading(true);
        try {
          const overview = await apiGet<AdminOverviewResponse>("admin/overview");
          if (!active) return;
          const incidentRows = (overview.incidents ?? []).map((incident) => ({
            tenant_id: incident.tenant_id,
            intent_id: incident.intent_id,
            state: incident.state,
            classification: `${incident.kind}/${incident.severity}`,
            updated_at_ms: incident.updated_at_ms,
          }));
          const deadLetterRows = (overview.dead_letters ?? []).map((job) => ({
            tenant_id: job.tenant_id,
            intent_id: job.intent_id,
            state: job.state,
            classification: job.classification,
            updated_at_ms: job.updated_at_ms,
          }));
          const rows = [...incidentRows, ...deadLetterRows].sort(
            (left, right) => right.updated_at_ms - left.updated_at_ms
          );
          setNotifications(rows.slice(0, 8));
          setNotificationsStatus("live");
        } catch {
          if (!active) return;
          setNotificationsStatus("down");
        } finally {
          if (active) setNotificationsLoading(false);
        }
      }
      void pollPlatformNotifications();
      intervalId = window.setInterval(() => {
        void pollPlatformNotifications();
      }, 15000);

      return () => {
        active = false;
        if (intervalId) window.clearInterval(intervalId);
      };
    }

    let active = true;
    let reconnectTimer: number | undefined;
    let ws: WebSocket | null = null;

    function streamUrl(): string {
      const explicit = process.env.NEXT_PUBLIC_OPERATOR_UI_NOTIFICATIONS_WS_URL?.trim();
      if (explicit) return explicit;
      if (typeof window === "undefined") return "";
      const proto = window.location.protocol === "https:" ? "wss" : "ws";
      return `${proto}://${window.location.host}/api/ui/stream/notifications`;
    }

    function connect() {
      if (!active) return;
      const url = streamUrl();
      if (!url) {
        setNotificationsStatus("down");
        setNotificationsLoading(false);
        return;
      }
      setNotificationsLoading(true);
      setNotificationsStatus((previous) => (previous === "live" ? "reconnecting" : "connecting"));
      ws = new WebSocket(url);

      ws.onopen = () => {
        if (!active) return;
        setNotificationsLoading(false);
        setNotificationsStatus("live");
      };

      ws.onmessage = (event) => {
        if (!active) return;
        const payload = parseNotificationEnvelope(event.data);
        if (!payload) return;
        const rows = [...(payload.notifications ?? [])].sort(
          (left, right) => right.updated_at_ms - left.updated_at_ms
        );
        setNotifications(rows.slice(0, 8));
      };

      ws.onerror = () => {
        if (!active) return;
        setNotificationsStatus("reconnecting");
      };

      ws.onclose = () => {
        if (!active) return;
        setNotificationsLoading(false);
        setNotificationsStatus("reconnecting");
        reconnectTimer = window.setTimeout(() => connect(), 3000);
      };
    }

    connect();
    return () => {
      active = false;
      if (reconnectTimer) window.clearTimeout(reconnectTimer);
      try {
        ws?.close();
      } catch {
        // ignore close errors during teardown
      }
    };
  }, [session, sessionReady, surfaceScope]);

  useEffect(() => {
    let cancelled = false;
    void readSession().then((nextSession) => {
      if (cancelled) return;
      setSession(nextSession);
      setSessionReady(true);
      if (nextSession) {
        void listWorkspaces()
          .then((rows) => {
            if (!cancelled) setWorkspaces(rows);
          })
          .catch(() => {
            if (!cancelled) setWorkspaces([]);
          });
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!sessionReady) return;
    if (!session) {
      const next = encodeURIComponent(pathname || "/app/dashboard");
      router.replace(`/login?next=${next}`);
      return;
    }
    if (kind === "platform" && !canAccessOperator(session.role)) {
      router.replace("/app/dashboard");
      return;
    }
    if (kind === "platform" && !pathname.startsWith("/admin")) {
      router.replace("/admin");
      return;
    }
    if (kind === "operator" && !canAccessOperator(session.role)) {
      router.replace("/app/dashboard");
      return;
    }
    if (kind === "customer" && !canAccessCustomerPath(pathname, session.role)) {
      router.replace("/app/dashboard");
    }
  }, [kind, pathname, router, session, sessionReady]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const isPaletteTrigger = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k";
      if (isPaletteTrigger) {
        event.preventDefault();
        setCommandOpen((previous) => !previous);
        return;
      }
      if (!commandOpen) return;
      if (event.key === "Escape") {
        event.preventDefault();
        setCommandOpen(false);
        return;
      }
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setCommandCursor((previous) =>
          Math.min(previous + 1, Math.max(0, commandResults.length - 1))
        );
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setCommandCursor((previous) => Math.max(0, previous - 1));
        return;
      }
      if (event.key === "Enter") {
        const selected = commandResults[commandCursor];
        if (!selected) return;
        event.preventDefault();
        setCommandOpen(false);
        router.push(resolveShellHref(selected.href, kind));
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [commandCursor, commandOpen, commandResults, kind, router]);

  useEffect(() => {
    if (!commandOpen) return;
    const timer = window.setTimeout(() => {
      const query = commandQuery.trim();
      if (!query) {
        setCommandResults(QUICK_NAV_ITEMS);
        setCommandLoading(false);
        setCommandError(null);
        setCommandCursor(0);
        return;
      }
      setCommandLoading(true);
      setCommandError(null);
      const params = new URLSearchParams({
        q: query,
        limit: "12",
        scope:
          surfaceScope === "platform"
            ? "platform"
            : surfaceScope === "customer_operator"
            ? "tenant"
            : "workspace",
      });
      if (surfaceScope === "platform") {
        params.set("environment", platformEnvironment);
      }
      void apiGet<SearchResponse>(`search?${params.toString()}`)
        .then((response) => {
          setCommandResults(response.results ?? []);
          setCommandCursor(0);
        })
        .catch((searchErr: unknown) => {
          setCommandResults([]);
          setCommandError(searchErr instanceof Error ? searchErr.message : String(searchErr));
        })
        .finally(() => setCommandLoading(false));
    }, 180);

    return () => window.clearTimeout(timer);
  }, [commandOpen, commandQuery, platformEnvironment, surfaceScope]);

  useEffect(() => {
    if (!commandOpen) return;
    commandInputRef.current?.focus();
  }, [commandOpen]);

  useEffect(() => {
    if (!userMenuOpen) return;
    function onPointerDown(event: MouseEvent) {
      const target = event.target as Node | null;
      if (!target) return;
      if (userMenuRef.current?.contains(target)) return;
      setUserMenuOpen(false);
    }
    window.addEventListener("mousedown", onPointerDown);
    return () => window.removeEventListener("mousedown", onPointerDown);
  }, [userMenuOpen]);

  const sessionRole = session?.role ?? "viewer";
  const nav = useMemo(() => {
    const base =
      kind === "platform" ? PLATFORM_NAV : kind === "customer" ? CUSTOMER_NAV : OPERATOR_NAV;
    const visible = base.filter((item) => (item.visible ? item.visible(sessionRole) : true));
    if (kind !== "customer" || !session) return visible;
    const done = onboardingProgress(session).percent >= 100;
    if (!done) return visible;
    return visible.filter((item) => item.href !== "/app/onboarding");
  }, [kind, session, sessionRole]);
  const title =
    kind === "customer"
      ? "Azums Customer Console"
      : kind === "platform"
      ? "Azums Platform Console"
      : "Azums Operator";
  const subtitle =
    kind === "customer"
      ? "Use Playground, inspect durable receipts, and manage callback delivery with backend truth."
      : kind === "platform"
      ? "Cross-tenant platform operations, incidents, adapter health, and controls."
      : "Replay controls, intake audits, and deep system visibility.";
  const searchScopeLabel =
    surfaceScope === "platform"
      ? "Search scope: all tenants (platform)"
      : surfaceScope === "customer_operator"
      ? "Search scope: tenant operations"
      : "Search scope: current workspace";
  const notificationsScopeHint =
    surfaceScope === "platform"
      ? "Platform alerts: incidents, adapter health, provider degradation, and abuse signals."
      : surfaceScope === "customer_operator"
      ? "Ops alerts: DLQ items, delivery failures, and replay outcomes."
      : "Workspace alerts: request failures, callback failures, and quota warnings.";
  const healthLabel =
    surfaceScope === "platform"
      ? "Platform Healthy"
      : surfaceScope === "customer_operator"
      ? "Ops API Healthy"
      : "Workspace API Healthy";
  const onboarding = session ? onboardingProgress(session) : null;

  async function logout() {
    await clearSession();
    router.replace("/login");
  }

  async function onWorkspaceChange(nextWorkspaceId: string) {
    if (!nextWorkspaceId || !session || nextWorkspaceId === session.workspace_id) return;
    setSwitchingWorkspace(true);
    try {
      const updated = await switchWorkspace({ workspace_id: nextWorkspaceId });
      setSession(updated);
      const rows = await listWorkspaces();
      setWorkspaces(rows);
      router.refresh();
    } catch {
      // leave existing session untouched; shell surfaces auth error through normal API calls
    } finally {
      setSwitchingWorkspace(false);
    }
  }

  function resolveSearchPath(raw: string): string {
    const query = raw.trim();
    const lowered = query.toLowerCase();
    const requestPathPrefix = kind === "customer" ? "/app/requests/" : "/ops/requests/";
    const requestListPath = kind === "customer" ? "/app/requests" : "/ops/jobs";

    if (!query) return requestListPath;
    if (kind !== "customer" && (lowered.startsWith("job:") || lowered.startsWith("job_"))) {
      const value = lowered.startsWith("job:") ? query.split(":").slice(1).join(":").trim() : query;
      return `/ops/jobs?search=${encodeURIComponent(value)}`;
    }
    if (lowered.startsWith("intent:") || lowered.startsWith("request:")) {
      const value = query.split(":").slice(1).join(":").trim();
      return `${requestPathPrefix}${encodeURIComponent(value)}`;
    }
    if (lowered.startsWith("receipt:")) {
      const value = query.split(":").slice(1).join(":").trim();
      return `/app/receipts/${encodeURIComponent(value)}`;
    }
    if (lowered.startsWith("callback:")) {
      const value = query.split(":").slice(1).join(":").trim();
      return `${requestListPath}?search=${encodeURIComponent(value)}`;
    }
    if (lowered.startsWith("corr:") || lowered.startsWith("correlation:")) {
      const value = query.split(":").slice(1).join(":").trim();
      return `${requestListPath}?search=${encodeURIComponent(value)}`;
    }
    if (lowered.startsWith("intent_")) {
      return `${requestPathPrefix}${encodeURIComponent(query)}`;
    }
    if (lowered.startsWith("receipt_")) {
      return `/app/receipts/${encodeURIComponent(query)}`;
    }
    return `${requestListPath}?search=${encodeURIComponent(query)}`;
  }

function resolveShellHref(href: string, shellKind: ShellKind): string {
  if ((shellKind === "operator" || shellKind === "platform") && href.startsWith("/app/requests/")) {
    return href.replace("/app/requests/", "/ops/requests/");
  }
  return href;
}

  async function executeGlobalSearch(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const query = searchQuery.trim();
    if (!query) {
      setSearchError("Enter request, receipt, callback, or correlation id.");
      return;
    }
    setSearchError(null);
    setSearching(true);
    try {
      const params = new URLSearchParams({
        q: query,
        limit: "1",
        scope:
          surfaceScope === "platform"
            ? "platform"
            : surfaceScope === "customer_operator"
            ? "tenant"
            : "workspace",
      });
      if (surfaceScope === "platform") {
        params.set("environment", platformEnvironment);
      }
      const response = await apiGet<SearchResponse>(`search?${params.toString()}`);
      const first = response.results?.[0];
      if (first) {
        router.push(resolveShellHref(first.href, kind));
      } else {
        router.push(resolveSearchPath(query));
      }
    } catch (searchErr: unknown) {
      const message = searchErr instanceof Error ? searchErr.message : String(searchErr);
      setSearchError(message || "Search failed. Try again.");
      router.push(resolveSearchPath(query));
    } finally {
      setSearching(false);
    }
  }

  if (!sessionReady || !session) {
    return (
      <div className="auth-page">
        <section className="auth-card">
          <h1>Loading workspace...</h1>
        </section>
      </div>
    );
  }

  return (
    <div className="shell">
      <aside className="shell-sidebar">
        <div className="shell-brand">
          <Link href="/" className="brand-link">
            Durable Execution Platform
          </Link>
          <h1>{title}</h1>
          <p>{subtitle}</p>
        </div>
        <nav className="shell-nav">
          {nav.map((item) => {
            const active =
              pathname === item.href ||
              (item.href !== "/app/requests" &&
                pathname.startsWith(`${item.href}/`)) ||
              (item.href === "/app/requests" && pathname.startsWith("/app/requests"));
            return (
              <Link
                key={item.href}
                href={item.href}
                className={`shell-nav-link ${active ? "active" : ""}`}
              >
                {item.label}
              </Link>
            );
          })}
        </nav>
        <div className="shell-meta">
          {config ? (
            <>
              <div>
                <span>workspace</span>
                <strong>{session.workspace_name}</strong>
              </div>
              <div>
                <span>tenant</span>
                <strong>{config.tenant_id}</strong>
              </div>
              <div>
                <span>user</span>
                <strong>{session.full_name}</strong>
              </div>
              <div>
                <span>role</span>
                <strong>{session.role}</strong>
              </div>
              <div>
                <span>onboarding</span>
                <strong>
                  {onboarding?.completed}/{onboarding?.total}
                </strong>
              </div>
            </>
          ) : (
            <div>
              <span>connection</span>
              <strong>loading...</strong>
            </div>
          )}
        </div>
      </aside>
      <div className="shell-main">
        <header className="shell-top">
            <div className="shell-top-links">
            <Link href="/app/dashboard">Customer Console</Link>
            {canAccessOperator(session.role) ? <Link href="/ops">Customer Operator</Link> : null}
            {canAccessOperator(session.role) ? <Link href="/admin">Platform Console</Link> : null}
            <Link href="/">Landing</Link>
          </div>
          <div className="shell-top-health stack-row">
            <form className="shell-search" onSubmit={executeGlobalSearch}>
              <input
                value={searchQuery}
                onChange={(event) => setSearchQuery(event.target.value)}
                placeholder={
                  kind !== "customer"
                    ? "Search: intent_id, receipt_id, correlation_id, callback_id, job_id"
                    : "Search: intent_id, receipt_id, correlation_id, callback_id"
                }
                aria-label="Global search"
              />
              <button className="btn ghost btn-tight" type="submit">
                {searching ? "Searching..." : "Search"}
              </button>
              <button
                className="btn ghost btn-tight"
                type="button"
                onClick={() => setCommandOpen(true)}
              >
                Ctrl/Cmd+K
              </button>
            </form>
            <span className="health-pill neutral">{searchScopeLabel}</span>
            <div className="shell-notify-wrap">
              <button
                className="btn ghost btn-tight"
                type="button"
                onClick={() => setNotificationsOpen((previous) => !previous)}
              >
                Notifications ({notifications.length}) · {notificationsStatus}
              </button>
              {notificationsOpen ? (
                <section className="shell-notify-panel">
                  <div className="shell-notify-head">
                    <strong>
                      {surfaceScope === "platform"
                        ? "Platform Notifications"
                        : surfaceScope === "customer_operator"
                        ? "Ops Notifications"
                        : "Workspace Notifications"}
                    </strong>
                    <button
                      className="btn ghost btn-tight"
                      type="button"
                      onClick={() => setNotificationsOpen(false)}
                    >
                      Close
                    </button>
                  </div>
                  <p className="empty-note">{notificationsScopeHint}</p>
                  {notificationsLoading ? <p>Connecting to stream...</p> : null}
                  {!notificationsLoading && notificationsStatus !== "live" ? (
                    <p className="empty-note">
                      Live stream status: {notificationsStatus}. Updates reconnect automatically.
                    </p>
                  ) : null}
                  {!notificationsLoading && notifications.length === 0 ? (
                    <p>No recent updates.</p>
                  ) : null}
                  {notifications.map((notification) => (
                    <button
                      key={`${notification.intent_id}-${notification.updated_at_ms}`}
                      className="shell-notify-item"
                      type="button"
                    onClick={() => {
                      setNotificationsOpen(false);
                      const tenantQuery = notification.tenant_id
                        ? `?tenant_id=${encodeURIComponent(notification.tenant_id)}`
                        : "";
                      const target = surfaceScope === "platform"
                        ? `/ops/requests/${encodeURIComponent(notification.intent_id)}${tenantQuery}`
                        : resolveShellHref(
                            `/app/requests/${encodeURIComponent(notification.intent_id)}`,
                            kind
                          );
                      router.push(
                        target
                      );
                    }}
                  >
                      <div>
                        <strong>{shortId(notification.intent_id)}</strong>
                        <span>
                          {notification.state} / {notification.classification}
                        </span>
                      </div>
                      <span className="badge neutral">
                        {new Date(notification.updated_at_ms).toLocaleTimeString()}
                      </span>
                    </button>
                  ))}
                </section>
              ) : null}
            </div>
            {workspaces.length > 0 ? (
              <label className="shell-workspace-select">
                <span>Environment | Workspace</span>
                <select
                  value={session.workspace_id}
                  onChange={(event) => void onWorkspaceChange(event.target.value)}
                  disabled={switchingWorkspace}
                >
                  {workspaces.map((workspace) => (
                    <option key={workspace.workspace_id} value={workspace.workspace_id}>
                      {workspace.environment.toUpperCase()} | {workspace.workspace_name}
                    </option>
                  ))}
                </select>
              </label>
            ) : null}
            {surfaceScope === "platform" ? (
              <label className="shell-workspace-select">
                <span>Platform Environment</span>
                <select
                  value={platformEnvironment}
                  onChange={(event) =>
                    setPlatformEnvironment(
                      event.target.value as "all" | "sandbox" | "staging" | "production"
                    )
                  }
                >
                  <option value="all">ALL ENVIRONMENTS</option>
                  <option value="sandbox">SANDBOX</option>
                  <option value="staging">STAGING</option>
                  <option value="production">PRODUCTION</option>
                </select>
              </label>
            ) : null}
            <span className="health-pill identity-chip">
              {session.email} · {session.role} · {shortId(session.workspace_id)}
            </span>
            <span
              className={`health-pill ${health?.status_api_reachable ? "ok" : "down"}`}
            >
              {health?.status_api_reachable
                ? `${healthLabel} (${health.status_api_status_code ?? 200})`
                : `${healthLabel.replace("Healthy", "Unreachable")}`}
            </span>
            <div className="shell-user-wrap" ref={userMenuRef}>
              <button
                className="btn ghost btn-tight"
                type="button"
                onClick={() => setUserMenuOpen((previous) => !previous)}
              >
                User Menu
              </button>
              {userMenuOpen ? (
                <section className="shell-user-panel">
                  <div className="shell-user-head">
                    <strong>{session.full_name}</strong>
                    <span>{session.email}</span>
                    <span>
                      {session.role} · {session.workspace_name}
                    </span>
                  </div>
                  <Link href="/app/profile" onClick={() => setUserMenuOpen(false)}>
                    Profile
                  </Link>
                  <Link href="/security" onClick={() => setUserMenuOpen(false)}>
                    Security
                  </Link>
                  <Link href="/app/api-keys" onClick={() => setUserMenuOpen(false)}>
                    API Tokens
                  </Link>
                  <button
                    className="btn ghost btn-tight"
                    type="button"
                    onClick={() => {
                      setUserMenuOpen(false);
                      void logout();
                    }}
                  >
                    Logout
                  </button>
                </section>
              ) : null}
            </div>
          </div>
          {searchError ? <p className="shell-search-error">{searchError}</p> : null}
        </header>
        <main className="shell-content">{children}</main>
      </div>
      {commandOpen ? (
        <section
          className="command-palette-backdrop"
          role="dialog"
          aria-modal="true"
          aria-label="Global command palette"
          onClick={() => setCommandOpen(false)}
        >
          <div className="command-palette" onClick={(event) => event.stopPropagation()}>
            <div className="command-head">
              <strong>Search + Navigate</strong>
              <button className="btn ghost btn-tight" type="button" onClick={() => setCommandOpen(false)}>
                Esc
              </button>
            </div>
            <input
              ref={commandInputRef}
              value={commandQuery}
              onChange={(event) => setCommandQuery(event.target.value)}
              placeholder={
                kind !== "customer"
                  ? "intent_id, request_id, receipt_id, correlation_id, callback_delivery_id, job_id"
                  : "intent_id, request_id, receipt_id, correlation_id, callback_delivery_id"
              }
              aria-label="Command palette search"
            />
            <p className="empty-note">{searchScopeLabel}</p>
            {commandLoading ? <p className="empty-note">Searching...</p> : null}
            {commandError ? <p className="inline-error">{commandError}</p> : null}
            <div className="command-results">
              {commandResults.length === 0 && !commandLoading ? (
                <p className="empty-note">No results yet. Try another query.</p>
              ) : null}
              {commandResults.map((result, index) => (
                <button
                  key={`${result.kind}-${result.object_id}-${result.updated_at_ms}`}
                  className={`command-item ${index === commandCursor ? "active" : ""}`}
                  type="button"
                  onMouseEnter={() => setCommandCursor(index)}
                  onClick={() => {
                    setCommandOpen(false);
                    router.push(resolveShellHref(result.href, kind));
                  }}
                >
                  <div>
                    <strong>{result.title}</strong>
                    <p>{result.subtitle}</p>
                  </div>
                  <span className="badge neutral">{result.kind}</span>
                </button>
              ))}
            </div>
          </div>
        </section>
      ) : null}
    </div>
  );
}

function canAccessCustomerPath(pathname: string, role: WorkspaceRole): boolean {
  if (pathname.startsWith("/app/playground")) return canWriteRequests(role);
  if (pathname.startsWith("/app/api")) return canWriteRequests(role);
  if (pathname.startsWith("/app/api-keys")) return canWriteRequests(role);
  if (pathname.startsWith("/app/callbacks")) return canWriteRequests(role);
  if (pathname.startsWith("/app/webhooks")) return canManageWorkspace(role);
  if (pathname.startsWith("/app/billing")) return canViewBilling(role);
  if (pathname.startsWith("/app/team")) return canManageWorkspace(role);
  return true;
}

function parseNotificationEnvelope(raw: unknown): NotificationStreamEnvelope | null {
  if (typeof raw !== "string" || !raw.trim()) return null;
  try {
    const parsed = JSON.parse(raw) as {
      event?: unknown;
      generated_at_ms?: unknown;
      notifications?: unknown;
    };
    if (!Array.isArray(parsed.notifications)) return null;
    const notifications: NotificationItem[] = [];
    for (const entry of parsed.notifications) {
      if (!entry || typeof entry !== "object") continue;
      const record = entry as Record<string, unknown>;
      const intent = String(record.intent_id ?? "");
      if (!intent) continue;
      const item: NotificationItem = {
        intent_id: intent,
        state: String(record.state ?? ""),
        classification: String(record.classification ?? ""),
        updated_at_ms: Number(record.updated_at_ms ?? Date.now()),
      };
      if (typeof record.tenant_id === "string" && record.tenant_id.trim()) {
        item.tenant_id = record.tenant_id;
      }
      notifications.push(item);
    }
    return {
      event: String(parsed.event ?? "notifications.snapshot"),
      generated_at_ms: Number(parsed.generated_at_ms ?? Date.now()),
      notifications,
    };
  } catch {
    return null;
  }
}
