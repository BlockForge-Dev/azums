"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

// SVG Icons as components
const DashboardIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect width="7" height="9" x="3" y="3" rx="1" />
    <rect width="7" height="5" x="14" y="3" rx="1" />
    <rect width="7" height="9" x="14" y="12" rx="1" />
    <rect width="7" height="5" x="3" y="16" rx="1" />
  </svg>
);

const RequestsIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
    <polyline points="14,2 14,8 20,8" />
    <line x1="16" x2="8" y1="13" y2="13" />
    <line x1="16" x2="8" y1="17" y2="17" />
    <line x1="10" x2="8" y1="9" y2="9" />
  </svg>
);

const CallbacksIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z" />
    <path d="m9 12 2 2 4-4" />
  </svg>
);

const ReceiptsIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M4 2v20l2-1 2 1 2-1 2 1 2-1 2 1 2-1 2 1V2l-2 1-2-1-2 1-2-1-2 1-2-1-2 1Z" />
    <path d="M16 8h-6a2 2 0 1 0 0 4h4a2 2 0 1 1 0 4H8" />
    <path d="M12 17V7" />
  </svg>
);

const WebhooksIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M18 16.98h-5.99c-1.1 0-1.95.68-2.95 1.76" />
    <path d="M18 21h-6.97c-1.09 0-1.94.62-2.94 1.7" />
    <path d="M12 21h-1.01c-.56 0-1.03-.3-1.33-.78" />
    <path d="M18 4.01h-6.98C10.92 4 10.06 4.68 9.06 5.76" />
    <path d="M18 8.02h-6.97C10.92 8 10.06 8.68 9.06 9.76" />
    <path d="M12 8.02H1.99C1.43 8.02 1 8.45 1 9.01v6c0 .55.43.99.99.99H12c.55 0 .99-.44.99-1V9c0-.56-.44-1-1-1Z" />
    <path d="M23 12v-3" />
    <path d="M20 12v-3" />
    <circle cx="18" cy="9" r="3" />
  </svg>
);

const PlaygroundIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polygon points="5,3 19,12 5,21 5,3" />
  </svg>
);

// Ops Icons
const JobsIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect width="4" height="4" x="4" y="4" />
    <rect width="4" height="4" x="16" y="4" />
    <rect width="4" height="4" x="4" y="16" />
    <rect width="4" height="4" x="16" y="16" />
  </svg>
);

const ActivityIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="22,12 18,12 15,21 9,3 6,12 2,12" />
  </svg>
);

const DeliveriesIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="12" cy="12" r="10" />
    <path d="m9 12 2 2 4-4" />
  </svg>
);

const ExceptionsIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 9v4" />
    <path d="M12 17h.01" />
    <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
  </svg>
);

const DeadLettersIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="m3.2 3.2 17.6 17.6" />
    <path d="m20 4-4 4" />
    <rect width="16" height="16" x="4" y="4" rx="2" />
  </svg>
);

const AdapterHealthIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
  </svg>
);

const SystemIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </svg>
);

const DocsIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" />
  </svg>
);

const SupportIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="12" cy="12" r="10" />
    <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
    <path d="M12 17h.01" />
  </svg>
);

const BoltIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
  </svg>
);

export interface SidebarProps {
  activePath: string;
  workspaceName?: string;
  selectedWorkspaceId?: string;
  workspaces?: Array<{
    id: string;
    name: string;
  }>;
  onWorkspaceChange?: (workspaceId: string) => void;
}

const NAV_ITEMS = [
  { href: "/app/dashboard", label: "Dashboard", icon: DashboardIcon },
  { href: "/app/requests", label: "Requests", icon: RequestsIcon },
  { href: "/app/callbacks", label: "Callbacks", icon: CallbacksIcon },
  { href: "/app/receipts", label: "Receipts", icon: ReceiptsIcon },
  { href: "/app/webhooks", label: "Webhooks", icon: WebhooksIcon },
  { href: "/app/playground", label: "Playground", icon: PlaygroundIcon },
];

const OPS_NAV_ITEMS = [
  { href: "/ops/requests", label: "Requests", icon: RequestsIcon },
  { href: "/ops/jobs", label: "Jobs", icon: JobsIcon },
  { href: "/ops/exceptions", label: "Exceptions", icon: ExceptionsIcon },
  { href: "/ops/activity", label: "Activity", icon: ActivityIcon },
  { href: "/ops/deliveries", label: "Deliveries", icon: DeliveriesIcon },
  { href: "/ops/callback-deliveries", label: "Callbacks", icon: CallbacksIcon },
  { href: "/ops/dead-letters", label: "Dead Letters", icon: DeadLettersIcon },
  { href: "/ops/adapter-health", label: "Adapter Health", icon: AdapterHealthIcon },
  { href: "/ops/system", label: "System", icon: SystemIcon },
];

export function Sidebar({
  activePath,
  workspaceName,
  selectedWorkspaceId,
  workspaces,
  onWorkspaceChange,
}: SidebarProps) {
  const pathname = usePathname();

  const isOps = pathname.startsWith("/ops");
  const navItems = isOps ? OPS_NAV_ITEMS : NAV_ITEMS;

  function isActive(href: string) {
    return activePath === href || activePath.startsWith(`${href}/`);
  }

  return (
    <aside className="w-[260px] border-r border-border bg-gradient-to-b from-card/95 to-background backdrop-blur-xl p-4 sticky top-0 h-screen overflow-y-auto">
      <div className="px-2 mb-6">
        <Link href="/" className="inline-flex items-center gap-2 px-3 py-2 rounded-lg bg-primary/10 text-primary text-xs uppercase tracking-widest font-bold hover:bg-primary/15 transition-colors">
          <BoltIcon className="w-[18px] h-[18px]" />
          Azums
        </Link>
        <h1 className="text-xl font-bold mt-3 bg-gradient-to-r from-foreground to-primary bg-clip-text text-transparent">Durable Execution</h1>
        {workspaceName && (
          <p className="text-muted-foreground text-xs mt-1 font-medium">{workspaceName}</p>
        )}
      </div>

      {workspaces && workspaces.length > 1 && (
        <div className="px-3 py-3 mb-4 bg-muted/30 rounded-xl border border-border/50">
          <span className="text-[10px] uppercase tracking-wider text-muted-foreground block mb-1">Workspace</span>
          <select
            className="w-full px-3 py-2 text-sm rounded-lg border border-border bg-input text-foreground cursor-pointer"
            value={selectedWorkspaceId ?? workspaces[0]?.id}
            onChange={(e) => onWorkspaceChange?.(e.target.value)}
          >
            {workspaces.map((ws) => (
              <option key={ws.id} value={ws.id}>
                {ws.name}
              </option>
            ))}
          </select>
        </div>
      )}

      <nav className="flex flex-col gap-1">
        {navItems.map((item) => (
          <Link
            key={item.href}
            href={item.href}
            className={`flex items-center gap-3 px-3 py-2.5 rounded-xl text-sm font-medium transition-all duration-200 relative overflow-hidden ${
              isActive(item.href)
                ? "bg-gradient-to-r from-primary/15 to-primary/5 text-primary border border-primary/20"
                : "text-muted-foreground hover:text-foreground hover:bg-muted/50"
            }`}
          >
            {isActive(item.href) && (
              <span className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-3/5 bg-primary rounded-r" />
            )}
            <item.icon className={`w-5 h-5 transition-transform duration-200 ${isActive(item.href) ? 'text-primary' : ''}`} />
            <span>{item.label}</span>
          </Link>
        ))}
      </nav>

      <div className="mt-auto pt-4 border-t border-border/50">
        <div className="flex items-start gap-3 px-3 py-2.5 rounded-lg bg-muted/20 hover:bg-muted/35 transition-colors">
          <DocsIcon className="w-4 h-4 text-muted-foreground mt-0.5 flex-shrink-0" />
          <div>
            <span className="text-[10px] uppercase tracking-wider text-muted-foreground block">Docs</span>
            <a href="/docs" target="_blank" rel="noopener noreferrer" className="text-sm hover:text-primary transition-colors">
              Read the docs
            </a>
          </div>
        </div>
        <div className="flex items-start gap-3 px-3 py-2.5 rounded-lg bg-muted/20 hover:bg-muted/35 transition-colors mt-2">
          <SupportIcon className="w-4 h-4 text-muted-foreground mt-0.5 flex-shrink-0" />
          <div>
            <span className="text-[10px] uppercase tracking-wider text-muted-foreground block">Support</span>
            <a href="/contact" target="_blank" rel="noopener noreferrer" className="text-sm hover:text-primary transition-colors">
              Get help
            </a>
          </div>
        </div>
      </div>
    </aside>
  );
}
