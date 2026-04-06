import Link from "next/link";
import { 
  CheckCircle2, 
  Receipt, 
  ShieldCheck, 
  Layers, 
  ArrowRight, 
  Zap,
  Target,
  FileCheck,
  Search,
  Wallet,
  Building2,
  Globe,
  Mail,
  MessageSquare,
  Webhook,
  Hash,
  Box,
  Cpu,
  Database,
  Workflow,
  BarChart3,
  Clock,
  AlertTriangle,
  CheckCircle,
  ChevronRight,
  ExternalLink,
  XCircle,
  FileText,
  Eye
} from "lucide-react";

// ============================================================================
// ICONS
// ============================================================================

const BoltIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
  </svg>
);

const CheckIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="20,6 9,17 4,12" />
  </svg>
);

const ArrowRightIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M5 12h14" />
    <path d="m12 5 7 7-7 7" />
  </svg>
);

const SolanaIcon = ({ className }: { className?: string }) => (
  <svg className={className} viewBox="0 0 48 48" fill="currentColor">
    <path d="M38.5 11.7L24 4.7 9.5 11.7l1.7 29.6 12.8 6.9 12.8-6.9 1.7-29.6zM24 25.5l-9.4-5.1 1-17.9 8.4 4.7v14.3l-8.4 4.8 8.4 4.8v-14.3l8.4-4.7 1 17.9-9.4 5.1z"/>
  </svg>
);

const SuiIcon = ({ className }: { className?: string }) => (
  <svg className={className} viewBox="0 0 48 48" fill="currentColor">
    <circle cx="24" cy="24" r="20" fill="none" stroke="currentColor" strokeWidth="2"/>
    <circle cx="24" cy="24" r="8"/>
  </svg>
);

const EvmIcon = ({ className }: { className?: string }) => (
  <svg className={className} viewBox="0 0 48 48" fill="currentColor">
    <rect x="8" y="8" width="32" height="32" rx="4"/>
    <rect x="16" y="16" width="16" height="16" fill="#fff"/>
  </svg>
);

const HttpIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z" />
    <path d="m22 7-8.97 5.7a1.5 1.5 0 0 1-2.06 0L2 7" />
  </svg>
);

const WebhookIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M18 16.98h-5.99c-1.1 0-1.95.68-2.95 1.76" />
    <path d="M18 21h-6.01c-.55 0-1.05-.2-1.41-.59" />
    <path d="M12 21c-2.28 0-4.22-1.66-5-4h10c-.78 2.34-2.72 4-5 4" />
    <path d="M18 16.98c.68 0 1.25-.35 1.59-.9" />
    <path d="M21.41 14.59c-.36-.55-.95-.9-1.59-.9h-6.01c-.55 0-1.05.2-1.41.59" />
    <circle cx="12" cy="8" r="2" />
  </svg>
);

const SlackIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="24" height="24" viewBox="0 0 24 24" fill="currentColor">
    <path d="M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zM6.313 15.165a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313zM8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zM8.834 6.313a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312zM18.956 8.834a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zM17.688 8.834a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.165 0a2.528 2.528 0 0 1 2.523 2.522v6.312zM15.165 18.956a2.528 2.528 0 0 1 2.523 2.522A2.528 2.528 0 0 1 15.165 24a2.527 2.527 0 0 1-2.52-2.522v-2.522h2.52zM15.165 17.688a2.527 2.527 0 0 1-2.52-2.523 2.526 2.526 0 0 1 2.52-2.52h6.313A2.527 2.527 0 0 1 24 15.165a2.528 2.528 0 0 1-2.522 2.523h-6.313z"/>
  </svg>
);

const EmailIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect width="20" height="16" x="2" y="4" rx="2" />
    <path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7" />
  </svg>
);

// ============================================================================
// NAV COMPONENT - CLEAN & PROFESSIONAL
// ============================================================================

function Navbar() {
  return (
    <nav className="fixed top-0 left-0 right-0 z-50 border-b border-white/[0.06] bg-[#0c0c14]/90 backdrop-blur-xl">
      <div className="max-w-7xl mx-auto px-6 h-16 flex items-center justify-between">
        {/* Logo */}
        <Link href="/" className="flex items-center gap-2.5">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-teal-400 to-emerald-500 flex items-center justify-center shadow-lg shadow-teal-500/25">
            <Zap className="w-4 h-4 text-black" />
          </div>
          <span className="text-lg font-bold tracking-tight text-white">Azums</span>
        </Link>

        {/* Center Nav Links */}
        <div className="hidden md:flex items-center gap-1">
          <Link href="/how-it-works" className="px-3 py-2 text-sm text-white/50 hover:text-white hover:bg-white/5 rounded-lg transition-all">
            How It Works
          </Link>
          <Link href="/docs" className="px-3 py-2 text-sm text-white/50 hover:text-white hover:bg-white/5 rounded-lg transition-all">
            Docs
          </Link>
          <a href="#adapters" className="px-3 py-2 text-sm text-white/50 hover:text-white hover:bg-white/5 rounded-lg transition-all">
            Adapters
          </a>
          <a href="#reconciliation" className="px-3 py-2 text-sm text-white/50 hover:text-white hover:bg-white/5 rounded-lg transition-all">
            Reconciliation
          </a>
        </div>

        {/* Right side - Auth buttons */}
        <div className="flex items-center gap-3">
          <Link 
            href="/login"
            className="px-4 py-2 text-sm font-medium text-white/70 hover:text-white transition-colors"
          >
            Sign In
          </Link>
          <Link 
            href="/signup"
            className="px-4 py-2 rounded-lg bg-gradient-to-r from-teal-400 to-emerald-500 text-black text-sm font-semibold hover:shadow-lg hover:shadow-teal-500/25 transition-all"
          >
            Get Started
          </Link>
        </div>
      </div>
    </nav>
  );
}

// ============================================================================
// HERO SECTION - UNIFIED & POWERFUL
// ============================================================================

function HeroSection() {
  return (
    <section className="relative pt-32 pb-24 overflow-hidden">
      {/* Background Effects */}
      <div className="absolute inset-0 bg-[#0c0c14]" />
      <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[800px] h-[500px] bg-gradient-to-b from-teal-500/10 via-emerald-500/5 to-transparent rounded-full blur-[100px] pointer-events-none" />
      <div className="absolute top-20 left-10 w-[300px] h-[300px] bg-teal-500/5 rounded-full blur-[60px] pointer-events-none" />
      
      <div className="relative max-w-7xl mx-auto px-6">
        {/* Main Hero Content - Centered */}
        <div className="max-w-4xl mx-auto text-center mb-16">
          {/* Eyebrow */}
          <div className="inline-flex items-center gap-2.5 px-4 py-1.5 rounded-full bg-gradient-to-r from-teal-500/10 to-emerald-500/10 border border-teal-500/20 text-teal-300 text-xs font-medium mb-8">
            <span className="w-2 h-2 rounded-full bg-teal-400 animate-pulse" />
            Production-Ready on Solana
          </div>
          
          {/* Headline */}
          <h1 className="text-5xl md:text-6xl lg:text-7xl font-bold text-white leading-[1.1] tracking-tight mb-7">
            Execute transactions with{" "}
            <span className="text-transparent bg-clip-text bg-gradient-to-r from-teal-400 via-emerald-400 to-teal-400">
              receipts, not silence.
            </span>
          </h1>
          
          {/* Subheadline */}
          <p className="text-lg md:text-xl text-white/45 leading-relaxed mb-10 max-w-2xl mx-auto font-light">
            Azums is a receipt-oriented execution layer for direct API or webhook traffic and
            agent gateway traffic, recording what happened every time an intent runs with one
            shared execution truth model underneath.
          </p>
          
          {/* CTAs */}
          <div className="flex flex-col sm:flex-row gap-4 justify-center mb-12">
            <Link 
              href="/signup"
              className="inline-flex items-center justify-center gap-2.5 px-8 py-4 rounded-lg bg-gradient-to-r from-teal-400 to-emerald-500 text-black font-semibold text-base transition-all hover:shadow-xl hover:shadow-teal-500/20 hover:scale-[1.02]"
            >
              Start Building
              <ArrowRightIcon className="w-4 h-4" />
            </Link>
            <Link 
              href="/how-it-works"
              className="inline-flex items-center justify-center gap-2.5 px-8 py-4 rounded-lg border border-white/10 hover:border-white/20 hover:bg-white/5 text-white font-medium text-base transition-all"
            >
              See How It Works
            </Link>
          </div>
          
          {/* Micro Points - With Icons */}
          <div className="flex flex-wrap justify-center gap-x-8 gap-y-3 text-sm text-white/40">
            <div className="flex items-center gap-2.5">
              <XCircle className="w-4 h-4 text-teal-400" />
              <span>No silent failures</span>
            </div>
            <div className="flex items-center gap-2.5">
              <Receipt className="w-4 h-4 text-teal-400" />
              <span>Queryable receipts</span>
            </div>
            <div className="flex items-center gap-2.5">
              <ShieldCheck className="w-4 h-4 text-teal-400" />
              <span>Reconciliation-backed</span>
            </div>
          </div>
        </div>
        
        {/* Product Visual - Below hero content */}
        <div className="relative max-w-5xl mx-auto">
          <div className="relative bg-[#101018] rounded-2xl border border-white/[0.08] p-5 shadow-2xl shadow-black/50">
            {/* Header */}
            <div className="flex items-center justify-between mb-5 pb-4 border-b border-white/[0.06]">
              <div className="flex items-center gap-2">
                <div className="w-3 h-3 rounded-full bg-red-500/40" />
                <div className="w-3 h-3 rounded-full bg-yellow-500/40" />
                <div className="w-3 h-3 rounded-full bg-green-500/40" />
              </div>
              <div className="text-xs text-white/30 font-mono">Azums Dashboard</div>
            </div>
            
            {/* Receipt Card */}
            <div className="bg-[#0a0a10] rounded-xl border border-teal-500/15 p-4 mb-4">
              <div className="flex items-center justify-between mb-3">
                <span className="text-xs text-white/35">Receipt ID</span>
                <span className="px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-400 text-xs font-medium">executed</span>
              </div>
              <div className="font-mono text-sm text-white/80 mb-3">rcpt_3x7K9mP2qR4tY8vW1nZ</div>
              <div className="grid grid-cols-2 gap-3 text-xs">
                <div>
                  <div className="text-white/30 mb-1">Intent ID</div>
                  <div className="text-white/70 font-mono">intent_A2b5C9dE</div>
                </div>
                <div>
                  <div className="text-white/30 mb-1">Adapter</div>
                  <div className="text-white/70 flex items-center gap-1.5">
                    <SolanaIcon className="w-3.5 h-3.5" />
                    Solana
                  </div>
                </div>
                <div>
                  <div className="text-white/30 mb-1">Timestamp</div>
                  <div className="text-white/70">2024-03-15 14:32:01</div>
                </div>
                <div>
                  <div className="text-white/30 mb-1">Reconciliation</div>
                  <div className="text-emerald-400 flex items-center gap-1.5">
                    <CheckCircle className="w-3.5 h-3.5" />
                    matched
                  </div>
                </div>
              </div>
            </div>
            
            {/* Flow Visualization */}
            <div className="flex items-center justify-center gap-1.5 py-3">
              <div className="px-2.5 py-1.5 rounded-lg bg-white/[0.03] border border-white/[0.05] text-xs text-white/35">
                Intent
              </div>
              <ChevronRight className="w-3.5 h-3.5 text-white/20" />
              <div className="px-2.5 py-1.5 rounded-lg bg-white/[0.03] border border-white/[0.05] text-xs text-white/35">
                Execute
              </div>
              <ChevronRight className="w-3.5 h-3.5 text-white/20" />
              <div className="px-2.5 py-1.5 rounded-lg bg-teal-500/10 border border-teal-500/20 text-xs text-teal-300">
                Receipt
              </div>
              <ChevronRight className="w-3.5 h-3.5 text-white/20" />
              <div className="px-2.5 py-1.5 rounded-lg bg-white/[0.03] border border-white/[0.05] text-xs text-white/35">
                Verify
              </div>
            </div>
          </div>
          
          {/* Floating Elements */}
          <div className="absolute -top-5 -right-5 w-14 h-14 rounded-xl bg-gradient-to-br from-teal-500/20 to-emerald-500/10 border border-teal-500/20 flex items-center justify-center">
            <Receipt className="w-6 h-6 text-teal-400" />
          </div>
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// VALUE STRIP
// ============================================================================

function ValueStrip() {
  const values = [
    {
      icon: XCircle,
      title: "No silent failures",
      description: "Every execution path produces a recorded outcome."
    },
    {
      icon: Receipt,
      title: "Receipt-oriented by design",
      description: "Success or failure, Azums records what happened."
    },
    {
      icon: ShieldCheck,
      title: "Reconciliation-backed",
      description: "Verify, match, and investigate outcomes clearly."
    },
    {
      icon: Layers,
      title: "Built to expand",
      description: "Start with Solana, grow across modern adapters."
    }
  ];

  return (
    <section className="py-20 bg-[#0c0c14] border-y border-white/[0.04]">
      <div className="max-w-7xl mx-auto px-6">
        <div className="grid md:grid-cols-2 lg:grid-cols-4 gap-4">
          {values.map((item, idx) => (
            <div 
              key={idx}
              className="group p-6 rounded-xl bg-[#101018] border border-white/[0.06] hover:border-teal-500/20 transition-all duration-300 hover:-translate-y-1"
            >
              <div className="w-11 h-11 rounded-lg bg-gradient-to-br from-teal-500/10 to-emerald-500/5 border border-teal-500/15 flex items-center justify-center mb-4 group-hover:from-teal-500/15 group-hover:to-emerald-500/10 transition-colors">
                <item.icon className="w-5 h-5 text-teal-400" />
              </div>
              <h3 className="text-white font-semibold mb-2 tracking-tight">{item.title}</h3>
              <p className="text-sm text-white/40 leading-relaxed">{item.description}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// WHAT AZUMS IS TODAY
// ============================================================================

function WhatAzumsIsToday() {
  return (
    <section id="product" className="py-28 relative">
      <div className="max-w-7xl mx-auto px-6">
        <div className="grid lg:grid-cols-2 gap-20 items-center">
          <div>
            <h2 className="text-3xl md:text-4xl font-bold text-white mb-8 tracking-tight">
              What Azums is today
            </h2>
            <div className="space-y-5 text-white/50 leading-relaxed font-light">
              <p>
                Azums is the execution and receipt layer for modern transaction flows.
              </p>
              <p>
                Customers can integrate Azums directly through API or webhook traffic, or
                through the agent gateway. Once accepted, both paths converge into the same
                explicit execution path.
              </p>
              <p>
                That gives teams a clear, queryable record of what happened and reduces 
                ambiguity around missing states, unclear outcomes, and silent operational failures.
              </p>
              <p className="text-teal-300 font-normal">
                Currently available with Solana-first execution and reconciliation-backed visibility.
              </p>
            </div>
          </div>
          
          {/* Receipt Detail Mockup */}
          <div className="relative hidden lg:block">
            <div className="bg-[#101018] rounded-2xl border border-white/[0.08] p-6">
              <div className="flex items-center gap-2.5 mb-6 pb-5 border-b border-white/[0.06]">
                <Receipt className="w-5 h-5 text-teal-400" />
                <span className="text-white font-medium">Receipt Detail</span>
              </div>
              
              <div className="space-y-0">
                <div className="flex justify-between items-center py-3 border-b border-white/[0.04]">
                  <span className="text-white/35 text-sm">Receipt ID</span>
                  <span className="text-white/70 font-mono text-sm">rcpt_3x7K9mP2qR4tY</span>
                </div>
                <div className="flex justify-between items-center py-3 border-b border-white/[0.04]">
                  <span className="text-white/35 text-sm">Intent ID</span>
                  <span className="text-white/70 font-mono text-sm">intent_A2b5C9dE</span>
                </div>
                <div className="flex justify-between items-center py-3 border-b border-white/[0.04]">
                  <span className="text-white/35 text-sm">Adapter</span>
                  <span className="text-white/70 flex items-center gap-1.5">
                    <SolanaIcon className="w-4 h-4" />
                    Solana
                  </span>
                </div>
                <div className="flex justify-between items-center py-3 border-b border-white/[0.04]">
                  <span className="text-white/35 text-sm">Execution Time</span>
                  <span className="text-white/70 text-sm">2024-03-15 14:32:01</span>
                </div>
                <div className="flex justify-between items-center py-3 border-b border-white/[0.04]">
                  <span className="text-white/35 text-sm">Outcome</span>
                  <span className="px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 text-xs font-medium">success</span>
                </div>
                <div className="flex justify-between items-center py-3 border-b border-white/[0.04]">
                  <span className="text-white/35 text-sm">Reconciliation</span>
                  <span className="text-emerald-400 text-sm flex items-center gap-1.5">
                    <CheckCircle className="w-4 h-4" />
                    Verified
                  </span>
                </div>
                <div className="pt-5">
                  <span className="text-white/35 text-sm block mb-2.5">Error Details</span>
                  <div className="p-3 rounded-lg bg-emerald-500/5 border border-emerald-500/10 text-emerald-400/80 text-sm font-mono">
                    Transaction confirmed on-chain. Signature: 5x7K9mP2qR4tY8vW1nZ...
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// HOW IT WORKS
// ============================================================================

function HowItWorks() {
  const steps = [
    {
      step: "01",
      title: "Submit an intent",
      description: "A transaction or execution intent enters Azums."
    },
    {
      step: "02",
      title: "Execute with explicit outcomes",
      description: "Azums processes the action and records the resulting outcome."
    },
    {
      step: "03",
      title: "Reconcile and verify",
      description: "The reconciliation layer matches outcomes and surfaces mismatches."
    },
    {
      step: "04",
      title: "Query the receipt",
      description: "Users can inspect detailed execution and reconciliation records."
    }
  ];

  return (
    <section id="how-it-works" className="py-28 bg-[#0c0c14]">
      <div className="max-w-7xl mx-auto px-6">
        <div className="text-center mb-20">
          <h2 className="text-3xl md:text-4xl font-bold text-white mb-5 tracking-tight">
            How Azums works
          </h2>
          <p className="text-white/40 max-w-xl mx-auto font-light">
            Azums is designed so that important execution paths remain visible, auditable, and explainable.
          </p>
        </div>
        
        <div className="grid md:grid-cols-2 lg:grid-cols-4 gap-4">
          {steps.map((item, idx) => (
            <div key={idx} className="relative group">
              <div className="p-7 rounded-xl bg-[#101018] border border-white/[0.06] h-full hover:border-teal-500/15 transition-all duration-300">
                <div className="text-5xl font-bold text-teal-500/10 mb-5 tracking-[-0.05em]">{item.step}</div>
                <h3 className="text-lg font-semibold text-white mb-3 tracking-tight">{item.title}</h3>
                <p className="text-white/40 leading-relaxed text-sm font-light">{item.description}</p>
              </div>
              {idx < steps.length - 1 && (
                <div className="hidden lg:block absolute top-1/2 -right-2 transform -translate-y-1/2 z-10">
                  <ArrowRight className="w-5 h-5 text-white/15" />
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// CORE VALUE PILLARS
// ============================================================================

function CoreValuePillars() {
  const pillars = [
    {
      icon: Search,
      title: "Operational clarity",
      description: "Azums gives teams explicit visibility into what happened, instead of forcing them to infer outcomes from fragmented systems."
    },
    {
      icon: FileCheck,
      title: "Deterministic receipts",
      description: "Every execution path produces a receipt, making outcomes queryable and reducing ambiguity across critical transaction flows."
    },
    {
      icon: ShieldCheck,
      title: "Correctness through reconciliation",
      description: "Azums does not stop at execution. Its reconciliation layer helps confirm, match, and investigate what actually happened."
    },
    {
      icon: Layers,
      title: "Adapter-ready architecture",
      description: "Azums starts with Solana and is built to expand across Web3 and Web2 adapters without changing the core execution model."
    }
  ];

  return (
    <section className="py-28">
      <div className="max-w-7xl mx-auto px-6">
        <div className="text-center mb-20">
          <h2 className="text-3xl md:text-4xl font-bold text-white mb-5 tracking-tight">
            Why Azums matters
          </h2>
        </div>
        
        <div className="grid md:grid-cols-2 gap-4">
          {pillars.map((pillar, idx) => (
            <div 
              key={idx}
              className="group p-8 rounded-xl bg-[#101018] border border-white/[0.06] hover:border-teal-500/15 transition-all duration-300"
            >
              <div className="w-12 h-12 rounded-lg bg-gradient-to-br from-teal-500/15 to-emerald-500/10 border border-teal-500/15 flex items-center justify-center mb-6 group-hover:from-teal-500/20 group-hover:to-emerald-500/15 transition-colors">
                <pillar.icon className="w-6 h-6 text-teal-400" />
              </div>
              <h3 className="text-lg font-semibold text-white mb-3 tracking-tight">{pillar.title}</h3>
              <p className="text-white/40 leading-relaxed font-light">{pillar.description}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// RECONCILIATION SECTION
// ============================================================================

function ReconciliationSection() {
  return (
    <section id="reconciliation" className="py-28 bg-[#0c0c14]">
      <div className="max-w-7xl mx-auto px-6">
        <div className="grid lg:grid-cols-2 gap-20 items-center">
          <div>
            <h2 className="text-3xl md:text-4xl font-bold text-white mb-8 tracking-tight">
              Reconciliation is built into the trust story
            </h2>
            <div className="space-y-5 text-white/50 leading-relaxed font-light">
              <p>
                Azums is not only about triggering execution. It is about proving what happened 
                after execution.
              </p>
              <p>
                Its reconciliation layer helps match outcomes, verify state, surface exceptions, 
                and return detailed feedback as receipts. That gives users a clearer operational 
                picture and reduces the risk of silent drift between intent, execution, and final state.
              </p>
            </div>
            
            <div className="mt-10 grid grid-cols-2 gap-3">
              <div className="p-4 rounded-xl bg-[#101018] border border-white/[0.06]">
                <CheckCircle className="w-5 h-5 text-emerald-400 mb-2.5" />
                <div className="text-white text-sm font-medium">Match execution to outcome</div>
              </div>
              <div className="p-4 rounded-xl bg-[#101018] border border-white/[0.06]">
                <AlertTriangle className="w-5 h-5 text-amber-400 mb-2.5" />
                <div className="text-white text-sm font-medium">Surface exceptions early</div>
              </div>
              <div className="p-4 rounded-xl bg-[#101018] border border-white/[0.06]">
                <FileCheck className="w-5 h-5 text-teal-400 mb-2.5" />
                <div className="text-white text-sm font-medium">Return verifiable feedback</div>
              </div>
              <div className="p-4 rounded-xl bg-[#101018] border border-white/[0.06]">
                <Workflow className="w-5 h-5 text-purple-400 mb-2.5" />
                <div className="text-white text-sm font-medium">Support hybrid workflows</div>
              </div>
            </div>
            
            {/* <p className="mt-8 text-sm text-white/30 italic">
              The reconciliation layer can serve both Azums-powered workflows and standalone operational use cases.
            </p> */}
          </div>
          
          {/* Architecture Diagram */}
          <div className="relative hidden lg:block">
            <div className="bg-[#101018] rounded-2xl border border-white/[0.08] p-8 pt-6">
              <div className="text-center mb-8">
                <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-gradient-to-r from-teal-500/10 to-emerald-500/10 border border-teal-500/20 text-teal-300 text-sm font-medium">
                  Reconciliation Flow
                </div>
              </div>
              
              <div className="space-y-3">
                <div className="flex items-center gap-4">
                  <div className="w-10 h-10 rounded-lg bg-blue-500/10 flex items-center justify-center">
                    <Target className="w-5 h-5 text-blue-400" />
                  </div>
                  <div className="flex-1 p-3 rounded-lg bg-[#0a0a10] border border-white/[0.05]">
                    <div className="text-white text-sm font-medium">Azums Intent</div>
                    <div className="text-white/30 text-xs">intent_A2b5C9dE</div>
                  </div>
                </div>
                
                <div className="flex justify-center">
                  <ArrowRight className="w-4 h-4 text-white/20 rotate-90 lg:rotate-0" />
                </div>
                
                <div className="flex items-center gap-4">
                  <div className="w-10 h-10 rounded-lg bg-emerald-500/10 flex items-center justify-center">
                    <Receipt className="w-5 h-5 text-emerald-400" />
                  </div>
                  <div className="flex-1 p-3 rounded-lg bg-[#0a0a10] border border-white/[0.05]">
                    <div className="text-white text-sm font-medium">Execution Receipt</div>
                    <div className="text-white/30 text-xs">rcpt_3x7K9mP2qR</div>
                  </div>
                </div>
                
                <div className="flex justify-center">
                  <ArrowRight className="w-4 h-4 text-white/20 rotate-90 lg:rotate-0" />
                </div>
                
                <div className="flex items-center gap-4">
                  <div className="w-10 h-10 rounded-lg bg-teal-500/10 flex items-center justify-center">
                    <ShieldCheck className="w-5 h-5 text-teal-400" />
                  </div>
                  <div className="flex-1 p-3 rounded-lg bg-[#0a0a10] border border-teal-500/15">
                    <div className="text-white text-sm font-medium">Reconciliation Match</div>
                    <div className="text-teal-400 text-xs">✓ verified</div>
                  </div>
                </div>
                
                <div className="flex justify-center">
                  <ArrowRight className="w-4 h-4 text-white/20 rotate-90 lg:rotate-0" />
                </div>
                
                <div className="flex items-center gap-4">
                  <div className="w-10 h-10 rounded-lg bg-purple-500/10 flex items-center justify-center">
                    <CheckCircle className="w-5 h-5 text-purple-400" />
                  </div>
                  <div className="flex-1 p-3 rounded-lg bg-[#0a0a10] border border-white/[0.05]">
                    <div className="text-white text-sm font-medium">Verified Outcome</div>
                    <div className="text-white/30 text-xs">Detailed receipt returned</div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// ADAPTER EXPANSION SECTION
// ============================================================================

function AdapterExpansion() {
  const availableAdapters = [
    { name: "Solana", icon: SolanaIcon, status: "now" }
  ];
  
  const comingSoonAdapters = [
    { name: "Sui", icon: SuiIcon },
    { name: "EVM", icon: EvmIcon },
    { name: "HTTP", icon: HttpIcon },
    { name: "Webhooks", icon: WebhookIcon },
    { name: "Slack", icon: SlackIcon },
    { name: "Email", icon: EmailIcon }
  ];

  return (
    <section id="adapters" className="py-28">
      <div className="max-w-7xl mx-auto px-6">
        <div className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-bold text-white mb-5 tracking-tight">
            Built to expand across adapters
          </h2>
          <p className="text-white/40 max-w-2xl mx-auto font-light">
            Azums launches with Solana-first execution and is designed to grow into a broader 
            adapter-driven execution layer across both Web3 and Web2 systems.
          </p>
        </div>
        
        {/* Available Now */}
        <div className="mb-10">
          <div className="flex items-center gap-2 mb-5">
            <span className="text-xs font-medium text-white/40 uppercase tracking-wider">Available Now</span>
          </div>
          <div className="flex flex-wrap gap-3">
            {availableAdapters.map((adapter, idx) => (
              <div 
                key={idx}
                className="flex items-center gap-3 px-5 py-3 rounded-xl bg-[#101018] border border-teal-500/20"
              >
                <adapter.icon className="w-6 h-6 text-teal-400" />
                <span className="text-white font-medium">{adapter.name}</span>
                <span className="px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 text-xs font-medium">Available</span>
              </div>
            ))}
          </div>
        </div>
        
        {/* Coming Next */}
        <div>
          <div className="flex items-center gap-2 mb-5">
            <span className="text-xs font-medium text-white/40 uppercase tracking-wider">Coming Soon</span>
          </div>
          <div className="flex flex-wrap gap-3">
            {comingSoonAdapters.map((adapter, idx) => (
              <div 
                key={idx}
                className="flex items-center gap-3 px-5 py-3 rounded-xl bg-[#101018] border border-dashed border-white/[0.08] opacity-50"
              >
                <adapter.icon className="w-6 h-6 text-white/30" />
                <span className="text-white/40 font-medium">{adapter.name}</span>
              </div>
            ))}
          </div>
        </div>
        
        <div className="mt-12 text-center">
          <p className="text-lg text-white/60">
            One execution model. Multiple environments. Clear receipts across all of them.
          </p>
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// PRESENT VS FUTURE
// ============================================================================

function PresentVsFuture() {
  return (
    <section id="roadmap" className="py-28 bg-[#0c0c14]">
      <div className="max-w-7xl mx-auto px-6">
        <div className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-bold text-white mb-5 tracking-tight">
            What Azums is now — and where it is going
          </h2>
        </div>
        
        <div className="grid lg:grid-cols-2 gap-6">
          {/* Today */}
          <div className="p-8 rounded-xl bg-[#101018] border border-white/[0.06]">
            <div className="flex items-center gap-3 mb-6">
              <div className="w-10 h-10 rounded-lg bg-teal-500/10 flex items-center justify-center">
                <Clock className="w-5 h-5 text-teal-400" />
              </div>
              <h3 className="text-xl font-semibold text-white">Available Now</h3>
            </div>
            <p className="text-white/50 mb-7 font-light">
              Azums is a Solana-first execution and receipt layer with reconciliation-backed correctness.
            </p>
            <ul className="space-y-3">
              {["Submit intents", "Execute with explicit outcomes", "Record receipts for success or failure", "Reconcile outcomes for correctness", "Query what happened clearly"].map((item, idx) => (
                <li key={idx} className="flex items-center gap-3 text-white/60">
                  <div className="w-1 h-1 rounded-full bg-teal-400" />
                  {item}
                </li>
              ))}
            </ul>
          </div>
          
          {/* Over Time */}
          <div className="p-8 rounded-xl bg-[#101018] border border-white/[0.06]">
            <div className="flex items-center gap-3 mb-6">
              <div className="w-10 h-10 rounded-lg bg-purple-500/10 flex items-center justify-center">
                <BarChart3 className="w-5 h-5 text-purple-400" />
              </div>
              <h3 className="text-xl font-semibold text-white">Coming Soon</h3>
            </div>
            <p className="text-white/50 mb-7 font-light">
              Azums expands into a broader multi-adapter execution fabric across Web2 and Web3 systems.
            </p>
            <ul className="space-y-3">
              {["More blockchain adapters", "More communication and system adapters", "Expanded reconciliation coverage", "Exception intelligence", "Higher-order operational products"].map((item, idx) => (
                <li key={idx} className="flex items-center gap-3 text-white/60">
                  <ChevronRight className="w-3.5 h-3.5 text-purple-400" />
                  {item}
                </li>
              ))}
            </ul>
          </div>
        </div>
        
        <div className="mt-10 p-6 rounded-xl bg-gradient-to-r from-teal-500/5 to-emerald-500/5 border border-teal-500/10 text-center">
          <p className="text-white/60 leading-relaxed font-light">
            Over time, Azums becomes a stronger foundation for broader operational layers, including 
            reconciliation-heavy workflows, controlled automation, and multi-rail transaction systems.
          </p>
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// USE CASES
// ============================================================================

function UseCases() {
  const useCases = [
    {
      icon: Wallet,
      title: "Payment operations teams",
      description: "Track transaction execution clearly and reduce ambiguity across critical money flows."
    },
    {
      icon: Box,
      title: "Web3 infrastructure teams",
      description: "Execute and verify transaction paths with explicit receipts and reconciliation visibility."
    },
    {
      icon: Cpu,
      title: "Platforms handling sensitive workflows",
      description: "Create a clearer record of what happened, even when execution fails."
    },
    {
      icon: Layers,
      title: "Teams building on adapters",
      description: "Start with one adapter today and grow across multiple environments over time."
    }
  ];

  return (
    <section className="py-28">
      <div className="max-w-7xl mx-auto px-6">
        <div className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-bold text-white mb-5 tracking-tight">
            Who Azums is for
          </h2>
        </div>
        
        <div className="grid md:grid-cols-2 gap-4">
          {useCases.map((useCase, idx) => (
            <div 
              key={idx}
              className="p-6 rounded-xl bg-[#101018] border border-white/[0.06] hover:border-teal-500/15 transition-all duration-300"
            >
              <div className="w-11 h-11 rounded-lg bg-gradient-to-br from-teal-500/10 to-emerald-500/5 border border-teal-500/15 flex items-center justify-center mb-4">
                <useCase.icon className="w-5 h-5 text-teal-400" />
              </div>
              <h3 className="text-base font-semibold text-white mb-2 tracking-tight">{useCase.title}</h3>
              <p className="text-white/40 leading-relaxed text-sm font-light">{useCase.description}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// ARCHITECTURE PREVIEW
// ============================================================================

function ArchitecturePreview() {
  return (
    <section className="py-28 bg-[#0c0c14]">
      <div className="max-w-7xl mx-auto px-6">
        <div className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-bold text-white mb-5 tracking-tight">
            A cleaner architecture for execution visibility
          </h2>
        </div>
        
        <div className="bg-[#101018] rounded-2xl border border-white/[0.08] p-8">
          {/* Top Layer - Entry Paths */}
          <div className="mb-8">
            <div className="text-center mb-4">
              <span className="text-xs font-medium text-white/30 uppercase tracking-wider">Entry Paths</span>
            </div>
            <div className="flex justify-center gap-3">
              <div className="px-4 py-2 rounded-lg bg-[#0c0c14] border border-white/[0.05] text-white/40 text-sm">Direct API / Webhooks</div>
              <div className="px-4 py-2 rounded-lg bg-[#0c0c14] border border-white/[0.05] text-white/40 text-sm">Agent Gateway</div>
            </div>
          </div>
          
          {/* Arrow */}
          <div className="flex justify-center mb-8">
            <ArrowRight className="w-4 h-4 text-white/15 rotate-90" />
          </div>
          
          {/* Middle Layer - Azums Core */}
          <div className="mb-8">
            <div className="text-center mb-4">
              <span className="text-xs font-medium text-teal-400 uppercase tracking-wider">Azums Core</span>
            </div>
            <div className="max-w-3xl mx-auto">
              <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
                <div className="p-4 rounded-xl bg-gradient-to-br from-teal-500/10 to-emerald-500/5 border border-teal-500/15 text-center">
                  <Target className="w-5 h-5 text-teal-400 mx-auto mb-2" />
                  <div className="text-white text-sm font-medium">Intent Intake</div>
                </div>
                <div className="p-4 rounded-xl bg-[#0c0c14] border border-white/[0.05] text-center">
                  <Zap className="w-5 h-5 text-white/50 mx-auto mb-2" />
                  <div className="text-white/60 text-sm font-medium">Execution Engine</div>
                </div>
                <div className="p-4 rounded-xl bg-[#0c0c14] border border-white/[0.05] text-center">
                  <Receipt className="w-5 h-5 text-white/50 mx-auto mb-2" />
                  <div className="text-white/60 text-sm font-medium">Receipt Model</div>
                </div>
                <div className="p-4 rounded-xl bg-[#0c0c14] border border-white/[0.05] text-center">
                  <Search className="w-5 h-5 text-white/50 mx-auto mb-2" />
                  <div className="text-white/60 text-sm font-medium">Query Layer</div>
                </div>
              </div>
            </div>
          </div>
          
          {/* Side - Reconciliation */}
          <div className="flex justify-center mb-8">
            <div className="flex items-center gap-3">
              <ArrowRight className="w-4 h-4 text-white/15" />
              <div className="px-5 py-2.5 rounded-xl bg-purple-500/10 border border-purple-500/15">
                <div className="flex items-center gap-2">
                  <ShieldCheck className="w-5 h-5 text-purple-400" />
                  <span className="text-purple-300 text-sm font-medium">Reconciliation Engine</span>
                </div>
              </div>
              <ArrowRight className="w-4 h-4 text-white/15" />
            </div>
          </div>
          
          {/* Arrow */}
          <div className="flex justify-center mb-8">
            <ArrowRight className="w-4 h-4 text-white/15 rotate-90" />
          </div>
          
          {/* Bottom Layer - Adapters */}
          <div>
            <div className="text-center mb-4">
              <span className="text-xs font-medium text-white/30 uppercase tracking-wider">Adapters</span>
            </div>
            <div className="flex justify-center gap-3 flex-wrap">
              <div className="px-4 py-2 rounded-lg bg-teal-500/10 border border-teal-500/20 text-teal-300 text-sm flex items-center gap-2">
                <SolanaIcon className="w-4 h-4" /> Solana
              </div>
              <div className="px-4 py-2 rounded-lg bg-[#0c0c14] border border-dashed border-white/[0.08] text-white/30 text-sm flex items-center gap-2">
                <SuiIcon className="w-4 h-4" /> Sui
              </div>
              <div className="px-4 py-2 rounded-lg bg-[#0c0c14] border border-dashed border-white/[0.08] text-white/30 text-sm flex items-center gap-2">
                <EvmIcon className="w-4 h-4" /> EVM
              </div>
              <div className="px-4 py-2 rounded-lg bg-[#0c0c14] border border-dashed border-white/[0.08] text-white/30 text-sm">+ More</div>
            </div>
          </div>
        </div>
        
        <p className="mt-8 text-center text-white/35 max-w-2xl mx-auto font-light">
          Azums keeps direct API and agent runtime entry paths additive on top of one shared
          execution core so outcomes remain visible and explainable.
        </p>
      </div>
    </section>
  );
}

// ============================================================================
// FINAL CTA
// ============================================================================

function FinalCTA() {
  return (
    <section className="py-28 relative overflow-hidden">
      <div className="absolute inset-0 bg-gradient-to-b from-[#0c0c14] to-[#08080c]" />
      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[600px] h-[300px] bg-gradient-to-r from-teal-500/8 via-emerald-500/5 to-transparent rounded-full blur-[80px] pointer-events-none" />
      
      <div className="relative max-w-4xl mx-auto px-6 text-center">
        <h2 className="text-4xl md:text-5xl font-bold text-white mb-7 tracking-tight">
          Build with explicit outcomes, not silent errors.
        </h2>
        <p className="text-xl text-white/40 mb-12 max-w-2xl mx-auto font-light leading-relaxed">
          Azums helps teams execute intents with receipts, verification, and clearer operational 
          visibility — starting with Solana and expanding across modern adapters.
        </p>
        <div className="flex flex-col sm:flex-row gap-4 justify-center">
          <Link 
            href="/signup"
            className="inline-flex items-center justify-center gap-2.5 px-8 py-4 rounded-lg bg-gradient-to-r from-teal-400 to-emerald-500 text-black font-semibold text-base transition-all hover:shadow-xl hover:shadow-teal-500/20 hover:scale-[1.02]"
          >
            Start Building
            <ArrowRightIcon className="w-4 h-4" />
          </Link>
          <Link 
            href="/contact"
            className="inline-flex items-center justify-center gap-2.5 px-8 py-4 rounded-lg border border-white/10 hover:border-white/20 hover:bg-white/5 text-white font-medium text-base transition-all"
          >
            Talk to Us
          </Link>
        </div>
      </div>
    </section>
  );
}

// ============================================================================
// FOOTER
// ============================================================================

function Footer() {
  return (
    <footer className="py-12 border-t border-white/[0.04]">
      <div className="max-w-7xl mx-auto px-6">
        <div className="grid md:grid-cols-4 gap-10 mb-12">
          {/* Brand */}
          <div>
            <Link href="/" className="flex items-center gap-2.5 mb-4">
              <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-teal-400 to-emerald-500 flex items-center justify-center shadow-lg shadow-teal-500/25">
                <Zap className="w-4 h-4 text-black" />
              </div>
              <span className="text-xl font-bold tracking-tight text-white">Azums</span>
            </Link>
            <p className="text-white/30 text-sm leading-relaxed">
              Receipt-oriented execution for modern transaction systems.
            </p>
          </div>
          
          {/* Product */}
          <div>
            <h4 className="text-white font-medium mb-4">Product</h4>
            <ul className="space-y-2.5">
              <li><a href="#product" className="text-white/35 hover:text-white text-sm transition-colors">Features</a></li>
              <li><a href="/how-it-works" className="text-white/35 hover:text-white text-sm transition-colors">How It Works</a></li>
              <li><a href="#adapters" className="text-white/35 hover:text-white text-sm transition-colors">Adapters</a></li>
              <li><a href="/docs" className="text-white/35 hover:text-white text-sm transition-colors">Documentation</a></li>
            </ul>
          </div>
          
          {/* Company */}
          <div>
            <h4 className="text-white font-medium mb-4">Company</h4>
            <ul className="space-y-2.5">
              <li><a href="/about" className="text-white/35 hover:text-white text-sm transition-colors">About</a></li>
              <li><a href="/contact" className="text-white/35 hover:text-white text-sm transition-colors">Contact</a></li>
              <li><a href="/pricing" className="text-white/35 hover:text-white text-sm transition-colors">Pricing</a></li>
            </ul>
          </div>
          
          {/* Legal */}
          <div>
            <h4 className="text-white font-medium mb-4">Legal</h4>
            <ul className="space-y-2.5">
              <li><a href="/privacy" className="text-white/35 hover:text-white text-sm transition-colors">Privacy</a></li>
              <li><a href="/terms" className="text-white/35 hover:text-white text-sm transition-colors">Terms</a></li>
              <li><a href="/security" className="text-white/35 hover:text-white text-sm transition-colors">Security</a></li>
            </ul>
          </div>
        </div>
        
        <div className="pt-8 border-t border-white/[0.04] flex flex-col md:flex-row justify-between items-center gap-4">
          <p className="text-white/25 text-sm">
            © 2024 Azums. All rights reserved.
          </p>
          <div className="flex items-center gap-4">
            <a href="#" className="text-white/25 hover:text-white transition-colors">
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg>
            </a>
            <a href="#" className="text-white/25 hover:text-white transition-colors">
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/></svg>
            </a>
            <a href="#" className="text-white/25 hover:text-white transition-colors">
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24"><path d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.267 2.37 4.267 5.455v6.286zM5.337 7.433c-1.144 0-2.063-.926-2.063-2.065 0-1.138.92-2.063 2.063-2.063 1.14 0 2.064.925 2.064 2.063 0 1.139-.925 2.065-2.064 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z"/></svg>
            </a>
          </div>
        </div>
      </div>
    </footer>
  );
}

// ============================================================================
// DEFAULT EXPORT
// ============================================================================

export default function HomePage() {
  return (
    <div className="min-h-screen bg-[#0c0c14]">
      <Navbar />
      <HeroSection />
      <ValueStrip />
      <WhatAzumsIsToday />
      <HowItWorks />
      <CoreValuePillars />
      <ReconciliationSection />
      <AdapterExpansion />
      <PresentVsFuture />
      <UseCases />
      <ArchitecturePreview />
      <FinalCTA />
      <Footer />
    </div>
  );
}
