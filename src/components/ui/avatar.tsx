const PALETTE = ['#3b82f6', '#10b981', '#f59e0b', '#8b5cf6', '#ef4444', '#06b6d4'] as const;

interface Props {
  seed: string;
  size?: number;
  className?: string;
}

export function Avatar({ seed, size = 28, className = '' }: Props) {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  const color = PALETTE[h % PALETTE.length] ?? PALETTE[0];
  const initial = seed.trim().charAt(0).toUpperCase() || '?';
  return (
    <span
      className={`grid place-items-center rounded-full font-semibold text-white ${className}`}
      style={{ width: size, height: size, background: color, fontSize: size * 0.42 }}
    >
      {initial}
    </span>
  );
}
