import { ReactNode } from "react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface BadgeProps {
  children: ReactNode;
  variant?: "default" | "success" | "warn" | "error";
  className?: string;
}

export function Badge({
  children,
  variant = "default",
  className,
}: BadgeProps) {
  const variantStyles = {
    default: "border border-[var(--line)] bg-[rgba(9,30,43,0.84)] text-[var(--muted)]",
    success:
      "border border-[rgba(37,214,172,0.4)] bg-[rgba(37,214,172,0.17)] text-[#b7ffe9]",
    warn: "border border-[rgba(244,189,98,0.44)] bg-[rgba(244,189,98,0.16)] text-[#ffe3b0]",
    error:
      "border border-[rgba(255,122,114,0.48)] bg-[rgba(255,122,114,0.14)] text-[#ffd0cb]",
  };

  return (
    <span
      className={twMerge(
        clsx(
          "inline-flex items-center rounded-full px-[0.54rem] py-[0.18rem] text-[0.74rem] whitespace-nowrap",
          variantStyles[variant],
          className
        )
      )}
    >
      {children}
    </span>
  );
}

export interface HealthPillProps {
  status: "ok" | "down" | "unknown";
  children: ReactNode;
  className?: string;
}

export function HealthPill({
  status,
  children,
  className,
}: HealthPillProps) {
  const statusStyles = {
    ok: "border border-[rgba(37,214,172,0.4)] text-[#b7ffe9]",
    down: "border border-[rgba(255,122,114,0.42)] text-[#ffd0cb]",
    unknown: "border border-[var(--line)] text-[var(--muted)]",
  };

  return (
    <span
      className={twMerge(
        clsx(
          "inline-flex items-center rounded-full px-[0.48rem] py-[0.2rem] text-[0.74rem]",
          statusStyles[status],
          className
        )
      )}
    >
      {children}
    </span>
  );
}
