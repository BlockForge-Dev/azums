import { ReactNode } from "react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface SummaryCardProps {
  label: string;
  value: ReactNode;
  className?: string;
}

export function SummaryCard({ label, value, className }: SummaryCardProps) {
  return (
    <div className={twMerge("bg-muted/30 rounded-lg p-4 border border-border/50", className)}>
      <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">{label}</span>
      <strong className="text-2xl font-bold text-foreground block">{value}</strong>
    </div>
  );
}

export interface KPICardProps {
  label: string;
  value: string | number;
  trend?: "up" | "down" | "neutral";
  className?: string;
}

export function KPICard({ label, value, trend, className }: KPICardProps) {
  return (
    <div className={twMerge("bg-muted/30 rounded-lg p-4 border border-border/50", className)}>
      <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">{label}</span>
      <strong className={clsx("text-2xl font-bold block", trend === "up" && "text-green-400", trend === "down" && "text-red-400", trend === "neutral" && "text-foreground")}>
        {value}
      </strong>
    </div>
  );
}

export interface StatGridProps {
  children: ReactNode;
  className?: string;
}

export function StatGrid({ children, className }: StatGridProps) {
  return <div className={twMerge("grid grid-cols-2 md:grid-cols-4 gap-4", className)}>{children}</div>;
}
