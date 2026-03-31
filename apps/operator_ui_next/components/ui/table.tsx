import { ReactNode } from "react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface Column<T> {
  key: string;
  header: string;
  render?: (item: T) => ReactNode;
  className?: string;
}

export interface TableProps<T> {
  columns: Column<T>[];
  data: T[];
  keyExtractor: (item: T) => string;
  onRowClick?: (item: T) => void;
  selectedKey?: string;
  isLoading?: boolean;
  emptyMessage?: string;
  className?: string;
}

export function Table<T>({
  columns,
  data,
  keyExtractor,
  onRowClick,
  selectedKey,
  isLoading,
  emptyMessage = "No data available",
  className,
}: TableProps<T>) {
  if (isLoading) {
    return (
      <div className="overflow-x-auto rounded-lg border border-border">
        <table className={twMerge("w-full", className)}>
          <thead>
            <tr className="bg-muted/50">
              {columns.map((col) => (
                <th key={col.key} className={twMerge("text-left px-4 py-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground", col.className)}>
                  {col.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            <tr>
              <td colSpan={columns.length} className="text-center py-8 text-muted-foreground">
                Loading...
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    );
  }

  if (data.length === 0) {
    return (
      <div className="overflow-x-auto rounded-lg border border-border">
        <table className={twMerge("w-full", className)}>
          <thead>
            <tr className="bg-muted/50">
              {columns.map((col) => (
                <th key={col.key} className={twMerge("text-left px-4 py-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground", col.className)}>
                  {col.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            <tr>
              <td colSpan={columns.length} className="text-center py-8 text-muted-foreground">
                {emptyMessage}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    );
  }

  return (
    <div className="overflow-x-auto rounded-lg border border-border">
      <table className={twMerge("w-full", className)}>
        <thead>
          <tr className="bg-muted/50">
            {columns.map((col) => (
              <th key={col.key} className={twMerge("text-left px-4 py-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground", col.className)}>
                {col.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.map((item) => {
            const key = keyExtractor(item);
            const isSelected = selectedKey === key;

            return (
              <tr
                key={key}
                className={clsx(
                  "transition-all duration-100",
                  onRowClick && "cursor-pointer",
                  isSelected && "bg-[rgba(37,214,172,0.14)]"
                )}
                onClick={() => onRowClick?.(item)}
              >
                {columns.map((col) => (
                  <td key={col.key} className={col.className}>
                    {col.render ? col.render(item) : (item as Record<string, unknown>)[col.key] as ReactNode}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

export interface TableHeadProps {
  children: ReactNode;
}

export function TableHead({ children }: TableHeadProps) {
  return <thead className="bg-muted/50">{children}</thead>;
}

export interface TableRowProps {
  children: ReactNode;
  isSelected?: boolean;
  onClick?: () => void;
}

export function TableRow({ children, isSelected, onClick }: TableRowProps) {
  return (
    <tr
      className={clsx(
        "transition-all duration-100 border-b border-border last:border-b-0",
        onClick && "cursor-pointer hover:bg-muted/30",
        isSelected && "bg-primary/10"
      )}
      onClick={onClick}
    >
      {children}
    </tr>
  );
}

export interface TableCellProps {
  children: ReactNode;
  className?: string;
}

export function TableCell({ children, className }: TableCellProps) {
  return (
    <td className={twMerge("px-4 py-3 border-b border-border whitespace-nowrap text-sm", className)}>
      {children}
    </td>
  );
}
