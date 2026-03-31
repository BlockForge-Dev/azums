import { forwardRef, InputHTMLAttributes, ReactNode } from "react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  error?: string;
  hint?: string;
  leftElement?: ReactNode;
  rightElement?: ReactNode;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  (
    {
      label,
      error,
      hint,
      leftElement,
      rightElement,
      className,
      id,
      ...props
    },
    ref
  ) => {
    const inputId = id || label?.toLowerCase().replace(/\s+/g, "-");

    return (
      <div className="w-full">
        {label && (
          <label
            htmlFor={inputId}
            className="block text-[var(--muted)] text-[0.74rem] tracking-[0.07em] uppercase mb-[0.25rem]"
          >
            {label}
          </label>
        )}
        <div className="relative">
          {leftElement && (
            <div className="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--muted)]">
              {leftElement}
            </div>
          )}
          <input
            ref={ref}
            id={inputId}
            className={twMerge(
              clsx(
                "w-full border border-[var(--line)] rounded-[12px] bg-[rgba(8,25,38,0.9)] text-[var(--text)] min-h-[42px] p-[0.56rem_0.72rem] transition-all duration-150",
                "focus:outline-none focus:border-[var(--line-strong)] focus:shadow-[0_0_0_2px_rgba(37,214,172,0.14)]",
                error && "border-[rgba(255,122,114,0.48)] focus:border-[rgba(255,122,114,0.48)]",
                leftElement && "pl-10",
                rightElement && "pr-10",
                className
              )
            )}
            {...props}
          />
          {rightElement && (
            <div className="absolute right-3 top-1/2 -translate-y-1/2 text-[var(--muted)]">
              {rightElement}
            </div>
          )}
        </div>
        {error && <p className="mt-[0.4rem] text-[#ffd0cb] text-[0.78rem]">{error}</p>}
        {hint && !error && (
          <p className="mt-[0.4rem] text-[var(--muted)] text-[0.78rem]">{hint}</p>
        )}
      </div>
    );
  }
);

Input.displayName = "Input";
