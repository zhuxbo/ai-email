const PALETTE = ['#3b82f6', '#10b981', '#f59e0b', '#8b5cf6', '#ef4444', '#06b6d4'] as const;

export function colorForSeed(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return PALETTE[h % PALETTE.length] ?? PALETTE[0];
}

interface Props {
  seed: string;
  size?: number;
  className?: string;
}

export function Avatar({ seed, size = 28, className = '' }: Props) {
  const color = colorForSeed(seed);
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
