import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface PaginationProps {
  currentPage: number;
  totalPages: number;
  onPageChange: (page: number) => void;
  className?: string;
}

export function Pagination({
  currentPage,
  totalPages,
  onPageChange,
  className,
}: PaginationProps) {
  const getPageNumbers = () => {
    const pages: (number | "...")[] = [];
    const showPages = 5;

    if (totalPages <= showPages) {
      for (let i = 1; i <= totalPages; i++) {
        pages.push(i);
      }
    } else {
      if (currentPage <= 3) {
        for (let i = 1; i <= 4; i++) pages.push(i);
        pages.push("...");
        pages.push(totalPages);
      } else if (currentPage >= totalPages - 2) {
        pages.push(1);
        pages.push("...");
        for (let i = totalPages - 3; i <= totalPages; i++) pages.push(i);
      } else {
        pages.push(1);
        pages.push("...");
        for (let i = currentPage - 1; i <= currentPage + 1; i++) pages.push(i);
        pages.push("...");
        pages.push(totalPages);
      }
    }

    return pages;
  };

  return (
    <div className={twMerge("flex items-center justify-center gap-[0.25rem]", className)}>
      {/* Previous button */}
      <button
        type="button"
        onClick={() => onPageChange(currentPage - 1)}
        disabled={currentPage === 1}
        className={clsx(
          "p-2 rounded-[8px] border border-[var(--line)] bg-[rgba(9,27,39,0.86)] text-[var(--muted)] transition-all duration-150",
          "hover:border-[var(--line-strong)] hover:text-[var(--text)]",
          "disabled:opacity-[0.5] disabled:cursor-not-allowed disabled:hover:border-[var(--line)] disabled:hover:text-[var(--muted)]"
        )}
        aria-label="Previous page"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="15 18 9 12 15 6" />
        </svg>
      </button>

      {/* Page numbers */}
      <div className="flex items-center gap-[0.25rem]">
        {getPageNumbers().map((page, idx) =>
          page === "..." ? (
            <span key={`ellipsis-${idx}`} className="px-2 text-[var(--muted)]">
              ...
            </span>
          ) : (
            <button
              key={page}
              type="button"
              onClick={() => onPageChange(page)}
              className={clsx(
                "min-w-[36px] h-9 px-3 rounded-[8px] border text-[0.86rem] transition-all duration-150",
                currentPage === page
                  ? "border-[rgba(37,214,172,0.4)] bg-[rgba(37,214,172,0.18)] text-[#cbfff1]"
                  : "border-[var(--line)] bg-[rgba(9,27,39,0.86)] text-[var(--muted)] hover:border-[var(--line-strong)] hover:text-[var(--text)]"
              )}
            >
              {page}
            </button>
          )
        )}
      </div>

      {/* Next button */}
      <button
        type="button"
        onClick={() => onPageChange(currentPage + 1)}
        disabled={currentPage === totalPages}
        className={clsx(
          "p-2 rounded-[8px] border border-[var(--line)] bg-[rgba(9,27,39,0.86)] text-[var(--muted)] transition-all duration-150",
          "hover:border-[var(--line-strong)] hover:text-[var(--text)]",
          "disabled:opacity-[0.5] disabled:cursor-not-allowed disabled:hover:border-[var(--line)] disabled:hover:text-[var(--muted)]"
        )}
        aria-label="Next page"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="9 18 15 12 9 6" />
        </svg>
      </button>
    </div>
  );
}
