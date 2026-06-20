import type { ButtonHTMLAttributes, ReactNode } from 'react';

type Variant = 'primary' | 'secondary' | 'ghost';

const VARIANT: Record<Variant, string> = {
  primary: 'bg-accent text-white hover:opacity-90',
  secondary: 'bg-panel border border-[var(--color-border)] text-text-1 hover:opacity-90',
  ghost: 'text-text-2 hover:bg-black/5 dark:hover:bg-white/10',
};

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  children: ReactNode;
}

export function Button({ variant = 'primary', className = '', children, ...rest }: Props) {
  const tone = VARIANT[variant];
  return (
    <button
      type="button"
      className={`rounded-[var(--radius-md)] px-3.5 py-2 text-xs font-medium transition-colors disabled:opacity-50 disabled:pointer-events-none ${tone} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
