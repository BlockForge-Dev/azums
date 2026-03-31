import { forwardRef, TextareaHTMLAttributes } from "react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
  error?: string;
  hint?: string;
}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ label, error, hint, className, id, ...props }, ref) => {
    const textareaId = id || label?.toLowerCase().replace(/\s+/g, "-");

    return (
      <div className="w-full">
        {label && (
          <label
            htmlFor={textareaId}
            className="block text-[var(--muted)] text-[0.74rem] tracking-[0.07em] uppercase mb-[0.25rem]"
          >
            {label}
          </label>
        )}
        <textarea
          ref={ref}
          id={textareaId}
          className={twMerge(
            clsx(
              "w-full border border-[var(--line)] rounded-[12px] bg-[rgba(8,25,38,0.9)] text-[var(--text)] min-h-[100px] p-[0.56rem_0.72rem] transition-all duration-150 resize-y",
              "focus:outline-none focus:border-[var(--line-strong)] focus:shadow-[0_0_0_2px_rgba(37,214,172,0.14)]",
              error && "border-[rgba(255,122,114,0.48)]",
              className
            )
          )}
          {...props}
        />
        {error && <p className="mt-[0.4rem] text-[#ffd0cb] text-[0.78rem]">{error}</p>}
        {hint && !error && (
          <p className="mt-[0.4rem] text-[var(--muted)] text-[0.78rem]">{hint}</p>
        )}
      </div>
    );
  }
);

Textarea.displayName = "Textarea";
