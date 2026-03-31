import Link from "next/link";

export function EmptyState({
  title,
  description,
  actionHref,
  actionLabel,
  compact = false,
}: {
  title: string;
  description: string;
  actionHref?: string;
  actionLabel?: string;
  compact?: boolean;
}) {
  return (
    <div className={`empty-state ${compact ? "compact" : ""}`}>
      <span className="empty-state-icon" aria-hidden="true">
        ◌
      </span>
      <div>
        <h4>{title}</h4>
        <p>{description}</p>
        {actionHref && actionLabel ? (
          <Link className="empty-state-link" href={actionHref}>
            {actionLabel}
          </Link>
        ) : null}
      </div>
    </div>
  );
}
