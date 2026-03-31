import { forwardRef, ButtonHTMLAttributes, ReactNode } from "react";
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "ghost" | "danger";
  size?: "default" | "tight" | "small";
  isLoading?: boolean;
  leftIcon?: ReactNode;
  rightIcon?: ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      variant = "primary",
      size = "default",
      isLoading = false,
      leftIcon,
      rightIcon,
      className,
      disabled,
      children,
      ...props
    },
    ref
  ) => {
    const baseStyles =
      "inline-flex items-center justify-center border border-transparent rounded-[12px] font-semibold no-underline transition-all duration-150 cursor-pointer disabled:opacity-[0.62] disabled:cursor-not-allowed disabled:translate-y-0 disabled:brightness-100";

    const sizeStyles = {
      default: "min-h-[42px] p-[0.6rem_0.92rem]",
      tight: "p-[0.28rem_0.55rem] text-[0.76rem]",
      small: "min-h-[32px] p-[0.4rem_0.7rem] text-[0.82rem]",
    };

    const variantStyles = {
      primary:
        "bg-gradient-to-r from-[rgba(37,214,172,0.97)] to-[rgba(98,232,246,0.9)] text-[#07211b] hover:-translate-y-[1px] hover:brightness-[1.04] active:translate-y-0",
      ghost:
        "bg-[rgba(8,27,39,0.78)] border-[var(--line)] text-[var(--muted)] hover:-translate-y-[1px] hover:border-[var(--line-strong)] active:translate-y-0",
      danger:
        "bg-gradient-to-r from-[rgba(255,122,114,0.95)] to-[rgba(255,173,106,0.9)] text-[#2f140d] hover:-translate-y-[1px] hover:brightness-[1.04] active:translate-y-0",
    };

    const focusStyles =
      "focus-visible:outline-none focus-visible:shadow-[0_0_0_2px_rgba(37,214,172,0.28)]";

    return (
      <button
        ref={ref}
        className={twMerge(
          clsx(
            baseStyles,
            sizeStyles[size],
            variantStyles[variant],
            focusStyles,
            className
          )
        )}
        disabled={disabled || isLoading}
        {...props}
      >
        {isLoading ? (
          <>
            <svg
              className="animate-spin -ml-1 mr-2 h-4 w-4"
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
            >
              <circle
                className="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                strokeWidth="4"
              />
              <path
                className="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
              />
            </svg>
            Loading...
          </>
        ) : (
          <>
            {leftIcon && <span className="mr-2">{leftIcon}</span>}
            {children}
            {rightIcon && <span className="ml-2">{rightIcon}</span>}
          </>
        )}
      </button>
    );
  }
);

Button.displayName = "Button";
