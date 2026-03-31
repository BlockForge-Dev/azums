import { ReactNode } from "react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface CardProps {
  children: ReactNode;
  className?: string;
  variant?: "default" | "surface" | "panel";
}

export function Card({
  children,
  className,
  variant = "default",
}: CardProps) {
  const variantStyles = {
    default: "bg-card border border-border rounded-xl p-4",
    surface:
      "bg-card/90 border border-border rounded-xl p-5 shadow-lg",
    panel:
      "bg-card/95 border border-border rounded-xl p-5 shadow-xl backdrop-blur-sm",
  };

  return (
    <div className={twMerge(clsx(variantStyles[variant], className))}>
      {children}
    </div>
  );
}

export interface CardHeaderProps {
  title: string;
  subtitle?: string;
  action?: ReactNode;
  className?: string;
}

export function CardHeader({
  title,
  subtitle,
  action,
  className,
}: CardHeaderProps) {
  return (
    <div className={twMerge("flex justify-between items-start gap-4 mb-4", className)}>
      <div>
        <h3 className="text-base font-semibold text-foreground">{title}</h3>
        {subtitle && (
          <p className="mt-1 text-sm text-muted-foreground">
            {subtitle}
          </p>
        )}
      </div>
      {action && <div className="flex items-center gap-2">{action}</div>}
    </div>
  );
}

export interface CardContentProps {
  children: ReactNode;
  className?: string;
}

export function CardContent({ children, className }: CardContentProps) {
  return <div className={twMerge("", className)}>{children}</div>;
}

export interface CardFooterProps {
  children: ReactNode;
  className?: string;
}

export function CardFooter({ children, className }: CardFooterProps) {
  return (
    <div
      className={twMerge(
        "mt-4 pt-4 border-t border-border flex justify-end gap-2",
        className
      )}
    >
      {children}
    </div>
  );
}
