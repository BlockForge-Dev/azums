export type PlanTier = "Developer" | "Team" | "Enterprise";

export type PlanSpec = {
  tier: PlanTier;
  monthly_price_usd: number;
  free_play_requests_per_month: number;
  paid_mode: "Unlimited";
  headline: string;
  features: string[];
};

export const PLAN_ORDER: PlanTier[] = ["Developer", "Team", "Enterprise"];

export const PLAN_SPECS: Record<PlanTier, PlanSpec> = {
  Developer: {
    tier: "Developer",
    monthly_price_usd: 20,
    free_play_requests_per_month: 500,
    paid_mode: "Unlimited",
    headline: "Individual builders and early integrations.",
    features: [
      "Single workspace",
      "Solana intent playground",
      "Durable receipts and callback history",
      "Role-aware read/write access",
    ],
  },
  Team: {
    tier: "Team",
    monthly_price_usd: 80,
    free_play_requests_per_month: 1000,
    paid_mode: "Unlimited",
    headline: "Collaborative engineering teams shipping to production.",
    features: [
      "Multi-user workspace",
      "Invite and role management",
      "Callback destination controls",
      "Usage and billing operations",
    ],
  },
  Enterprise: {
    tier: "Enterprise",
    monthly_price_usd: 500,
    free_play_requests_per_month: 10000,
    paid_mode: "Unlimited",
    headline: "High-volume execution with strong governance boundaries.",
    features: [
      "Operator-grade observability",
      "Detailed replay and audit workflows",
      "Higher baseline throughput",
      "Enterprise onboarding and support",
    ],
  },
};

export const PLAN_FREE_PLAY_LIMITS: Record<PlanTier, number> = {
  Developer: PLAN_SPECS.Developer.free_play_requests_per_month,
  Team: PLAN_SPECS.Team.free_play_requests_per_month,
  Enterprise: PLAN_SPECS.Enterprise.free_play_requests_per_month,
};

export function formatMonthlyPrice(plan: PlanTier): string {
  return `$${PLAN_SPECS[plan].monthly_price_usd}/month`;
}

export function formatFreePlayLimit(plan: PlanTier): string {
  const limit = PLAN_SPECS[plan].free_play_requests_per_month;
  return `${limit.toLocaleString()} requests/month`;
}
