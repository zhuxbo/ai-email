import type { ReactNode } from 'react';

interface Props {
  children: ReactNode;
  active?: boolean;
  className?: string;
  onClick?: () => void;
}

export function Chip({ children, active = false, className = '', onClick }: Props) {
  const base = 'rounded-[var(--radius-sm)] px-2 py-0.5 text-[10px] font-medium transition-colors';
  const tone = active
    ? 'bg-accent-soft text-accent border border-[var(--color-accent-border)]'
    : 'text-text-2 border border-[var(--color-border)] hover:opacity-80';
  if (onClick) {
    return (
      <button type="button" onClick={onClick} className={`${base} ${tone} ${className}`}>
        {children}
      </button>
    );
  }
  return <span className={`${base} ${tone} ${className}`}>{children}</span>;
}
