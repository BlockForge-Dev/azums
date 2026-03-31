import { forwardRef, SelectHTMLAttributes, ReactNode } from "react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "children"> {
  label?: string;
  error?: string;
  hint?: string;
  options: SelectOption[];
  placeholder?: string;
}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(
  ({ label, error, hint, options, placeholder, className, id, ...props }, ref) => {
    const selectId = id || label?.toLowerCase().replace(/\s+/g, "-");

    return (
      <div className="w-full">
        {label && (
          <label
            htmlFor={selectId}
            className="block text-[var(--muted)] text-[0.74rem] tracking-[0.07em] uppercase mb-[0.25rem]"
          >
            {label}
          </label>
        )}
        <select
          ref={ref}
          id={selectId}
          className={twMerge(
            clsx(
              "w-full border border-[var(--line)] rounded-[12px] bg-[rgba(8,25,38,0.9)] text-[var(--text)] min-h-[42px] p-[0.56rem_0.72rem] transition-all duration-150 cursor-pointer appearance-none",
              "focus:outline-none focus:border-[var(--line-strong)] focus:shadow-[0_0_0_2px_rgba(37,214,172,0.14)]",
              "bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%2224%22%20height%3D%2224%22%20viewBox%3D%220%200%2024%2024%22%20fill%3D%22none%22%20stroke%3D%22%23a6c3d4%22%20stroke-width%3D%222%22%20stroke-linecap%3D%22round%22%20stroke-linejoin%3D%22round%22%3E%3Cpolyline%20points%3D%226%209%2012%2015%2018%209%22%3E%3C%2Fpolyline%3E%3C%2Fsvg%3E')] bg-[length:20px] bg-[right_0.5rem_center] bg-no-repeat pr-10",
              error && "border-[rgba(255,122,114,0.48)]",
              className
            )
          )}
          {...props}
        >
          {placeholder && (
            <option value="" disabled>
              {placeholder}
            </option>
          )}
          {options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        {error && <p className="mt-[0.4rem] text-[#ffd0cb] text-[0.78rem]">{error}</p>}
        {hint && !error && (
          <p className="mt-[0.4rem] text-[var(--muted)] text-[0.78rem]">{hint}</p>
        )}
      </div>
    );
  }
);

Select.displayName = "Select";
