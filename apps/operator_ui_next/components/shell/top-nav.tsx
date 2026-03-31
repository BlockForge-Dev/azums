"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { clearSession } from "@/lib/app-state";

export interface TopNavProps {
  user?: {
    email: string;
    name?: string;
  };
  onSearch?: (query: string) => void;
}

// SVG Icons as components
const SearchIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="8" />
    <path d="m21 21-4.3-4.3" />
  </svg>
);

const SettingsIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    <circle cx="12" cy="12" r="3" />
  </svg>
);

const UsersIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
    <circle cx="9" cy="7" r="4" />
    <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
    <path d="M16 3.13a4 4 0 0 1 0 7.75" />
  </svg>
);

const CreditCardIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect width="20" height="14" x="2" y="5" rx="2" />
    <line x1="2" x2="22" y1="10" y2="10" />
  </svg>
);

const BarChartIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <line x1="12" x2="12" y1="20" y2="10" />
    <line x1="18" x2="18" y1="20" y2="4" />
    <line x1="6" x2="6" y1="20" y2="16" />
  </svg>
);

const UserIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
    <circle cx="12" cy="7" r="4" />
  </svg>
);

const LogOutIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
    <polyline points="16,17 21,12 16,7" />
    <line x1="21" x2="9" y1="12" y2="12" />
  </svg>
);

const KeyIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="m21 2-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0 3 3L22 7l-3-3m-3.5 3.5L19 4" />
  </svg>
);

const LayersIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polygon points="12,2 2,7 12,12 22,7 12,2" />
    <polyline points="2,17 12,22 22,17" />
    <polyline points="2,12 12,17 22,12" />
  </svg>
);

const ChevronDownIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="m6 9 6 6 6-6" />
  </svg>
);

export function TopNav({ user, onSearch }: TopNavProps) {
  const router = useRouter();
  const [searchQuery, setSearchQuery] = useState("");
  const [showUserMenu, setShowUserMenu] = useState(false);
  const [signingOut, setSigningOut] = useState(false);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    onSearch?.(searchQuery);
  };

  async function handleSignOut() {
    setSigningOut(true);
    try {
      await clearSession();
    } finally {
      setShowUserMenu(false);
      setSigningOut(false);
      router.replace("/login");
      router.refresh();
    }
  }

  const navLinks = [
    { href: "/app/settings", label: "Settings", icon: SettingsIcon },
    { href: "/app/team", label: "Team", icon: UsersIcon },
    { href: "/app/billing", label: "Billing", icon: CreditCardIcon },
    { href: "/app/usage", label: "Usage", icon: BarChartIcon },
  ];

  return (
    <header className="sticky top-0 z-50 bg-card/85 backdrop-blur-xl border-b border-border px-6 py-3 flex justify-between items-center gap-6">
      <nav className="flex items-center gap-1">
        {navLinks.map((link) => (
          <Link 
            key={link.href} 
            href={link.href} 
            className="flex items-center gap-2 px-3 py-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/50 text-sm font-medium transition-all duration-150"
          >
            <link.icon className="w-4 h-4" />
            <span>{link.label}</span>
          </Link>
        ))}
      </nav>

      <form onSubmit={handleSearch} className="flex items-center gap-3 bg-input border border-border rounded-xl px-4 py-2 flex-1 max-w-md transition-all duration-200 focus-within:border-primary focus-within:shadow-[0_0_0_3px_rgba(120,200,180,0.15)]">
        <SearchIcon className="w-4 h-4 text-muted-foreground" />
        <input
          type="search"
          placeholder="Search requests, callbacks..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="bg-transparent border-none text-foreground w-full py-1 text-sm focus:outline-none focus:shadow-none placeholder:text-muted-foreground"
        />
      </form>

      <div className="relative">
        <button
          type="button"
          className="flex items-center gap-2 p-1 rounded-xl hover:bg-muted/30 border border-transparent hover:border-border transition-all duration-150"
          onClick={() => setShowUserMenu(!showUserMenu)}
        >
          <div className="w-9 h-9 rounded-xl bg-gradient-to-br from-primary to-emerald-400 flex items-center justify-center text-primary-foreground font-bold text-sm">
            {user?.name?.[0] || user?.email?.[0] || "U"}
          </div>
          <ChevronDownIcon className={`w-3.5 h-3.5 text-muted-foreground transition-transform duration-200 ${showUserMenu ? 'rotate-180' : ''}`} />
        </button>

        {showUserMenu && (
          <div className="absolute top-full right-0 mt-2 w-64 bg-card border border-border rounded-2xl shadow-[0_10px_40px_-10px_rgba(0,0,0,0.5)] p-2 z-50 animate-fade-in-up">
            <div className="flex items-center gap-3 px-3 py-3 border-b border-border/50 mb-2">
              <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-primary to-emerald-400 flex items-center justify-center text-primary-foreground font-bold">
                {user?.name?.[0] || user?.email?.[0] || "U"}
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-semibold truncate">{user?.name || "User"}</p>
                <p className="text-xs text-muted-foreground truncate">{user?.email}</p>
              </div>
            </div>
            <Link 
              href="/app/profile" 
              className="flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-muted/50 text-sm transition-colors" 
              onClick={() => setShowUserMenu(false)}
            >
              <UserIcon className="w-4 h-4 text-muted-foreground" />
              Profile
            </Link>
            <Link 
              href="/app/api-keys" 
              className="flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-muted/50 text-sm transition-colors" 
              onClick={() => setShowUserMenu(false)}
            >
              <KeyIcon className="w-4 h-4 text-muted-foreground" />
              API Keys
            </Link>
            <Link 
              href="/app/workspaces" 
              className="flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-muted/50 text-sm transition-colors" 
              onClick={() => setShowUserMenu(false)}
            >
              <LayersIcon className="w-4 h-4 text-muted-foreground" />
              Workspaces
            </Link>
            <div className="h-px bg-border/50 my-2" />
            <button
              type="button"
              className="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-muted/50 text-sm text-destructive transition-colors"
              onClick={() => void handleSignOut()}
            >
              <LogOutIcon className="w-4 h-4" />
              {signingOut ? "Signing out..." : "Sign Out"}
            </button>
          </div>
        )}
      </div>
    </header>
  );
}
