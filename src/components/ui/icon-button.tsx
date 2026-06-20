import type { ButtonHTMLAttributes, ReactNode } from 'react';

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  children: ReactNode;
}

export function IconButton({ label, className = '', children, ...rest }: Props) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={`grid place-items-center rounded-[var(--radius-sm)] transition-colors disabled:opacity-50 ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
