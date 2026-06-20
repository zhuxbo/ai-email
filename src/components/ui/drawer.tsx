import type { ReactNode } from 'react';
import { useBreakpoint } from '../../lib/hooks/use-breakpoint';

interface Props {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
}

export function Drawer({ open, onClose, children }: Props) {
  const bp = useBreakpoint();
  if (!open) return null;

  if (bp === 'desktop') {
    return <aside className="w-[248px] shrink-0 bg-ink text-slate-200">{children}</aside>;
  }

  const panel =
    bp === 'mobile'
      ? 'absolute inset-x-0 bottom-0 max-h-[80%] rounded-t-2xl'
      : 'absolute inset-y-0 right-0 w-80';

  return (
    <div className="fixed inset-0 z-40">
      <div className="absolute inset-0 bg-black/30" onClick={onClose} role="presentation" />
      <div className={`bg-ink text-slate-200 shadow-xl overflow-y-auto ${panel}`}>{children}</div>
    </div>
  );
}
