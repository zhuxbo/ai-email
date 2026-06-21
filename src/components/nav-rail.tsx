import { Avatar } from './ui/avatar';
import { IconButton } from './ui/icon-button';
import type { Account } from '../lib/types';

interface Props {
  accounts: Account[];
  selectedAccountId: string | null;
  syncing: boolean;
  onSelectAccount: (id: string | null) => void;
  onAddAccount: () => void;
  onSync: () => void;
  onRemoveAccount: (id: string) => void;
  onOpenSettings: () => void;
  onOpenAutoReply: () => void;
}

export function NavRail({
  accounts,
  selectedAccountId,
  syncing,
  onSelectAccount,
  onAddAccount,
  onSync,
  onRemoveAccount,
  onOpenSettings,
  onOpenAutoReply,
}: Props) {
  return (
    <nav className="flex w-[54px] shrink-0 flex-col items-center gap-3 bg-ink py-3">
      <button
        type="button"
        aria-label="全部账户"
        aria-pressed={selectedAccountId === null}
        title="全部账户（聚合收件箱）"
        onClick={() => {
          onSelectAccount(null);
        }}
        className={`flex h-[30px] w-[30px] items-center justify-center rounded-full bg-slate-700 text-xs font-medium text-slate-200 hover:bg-slate-600 ${
          selectedAccountId === null
            ? 'ring-2 ring-accent ring-offset-2 ring-offset-[var(--color-ink)]'
            : ''
        }`}
      >
        全
      </button>
      {accounts.map((a) => (
        <button
          key={a.id}
          type="button"
          aria-label={a.email}
          aria-pressed={a.id === selectedAccountId}
          title={`${a.email}（右键删除）`}
          onClick={() => {
            onSelectAccount(a.id);
          }}
          onContextMenu={(e) => {
            e.preventDefault();
            onRemoveAccount(a.id);
          }}
          className={`rounded-full ${a.id === selectedAccountId ? 'ring-2 ring-accent ring-offset-2 ring-offset-[var(--color-ink)]' : ''}`}
        >
          <Avatar seed={a.email} size={30} />
        </button>
      ))}
      <IconButton
        label="新增账户"
        onClick={onAddAccount}
        className="h-[30px] w-[30px] rounded-full bg-slate-700 text-slate-300 hover:bg-slate-600"
      >
        ＋
      </IconButton>
      <div className="h-px w-6 bg-slate-700" />
      <IconButton
        label="同步收件箱"
        onClick={onSync}
        disabled={syncing || accounts.length === 0}
        className="h-8 w-8 text-slate-200 hover:bg-slate-700"
      >
        {syncing ? '⟳' : '🔄'}
      </IconButton>
      <IconButton
        label="自动回复中心"
        onClick={onOpenAutoReply}
        className="h-8 w-8 text-amber-400 hover:bg-slate-700"
      >
        ⚡
      </IconButton>
      <IconButton
        label="设置"
        onClick={onOpenSettings}
        className="mt-auto h-8 w-8 text-slate-400 hover:bg-slate-700"
      >
        ⚙
      </IconButton>
    </nav>
  );
}
