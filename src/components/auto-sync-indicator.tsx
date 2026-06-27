import { useEffect, useState } from 'react';

import { useMailStore } from '../lib/store/mail';

function fmtTime(ts: number): string {
  return new Date(ts).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
}

export function AutoSyncIndicator() {
  const syncing = useMailStore((s) => s.syncing);
  const lastSyncAt = useMailStore((s) => s.lastSyncAt);
  const intervalMin = useMailStore((s) => s.autoSyncIntervalMin);
  const accountErrors = useMailStore((s) => s.accountErrors);
  const error = useMailStore((s) => s.error);
  const accounts = useMailStore((s) => s.accounts);
  const syncAllInbox = useMailStore((s) => s.syncAllInbox);

  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => {
      clearInterval(id);
    };
  }, []);

  const errCount = Object.keys(accountErrors).length;
  const disabled = syncing || accounts.length === 0;

  let label: string;
  let tone: 'sync' | 'fail' | 'off' | 'idle';
  if (syncing) {
    label = '同步中…';
    tone = 'sync';
  } else if (errCount > 0 || error !== null) {
    label = errCount > 0 ? `${String(errCount)} 个账户失败` : '同步失败';
    tone = 'fail';
  } else if (intervalMin === 0) {
    label = '自动收信已关';
    tone = 'off';
  } else if (lastSyncAt === null) {
    label = '尚未同步';
    tone = 'idle';
  } else {
    const remainMs = lastSyncAt + intervalMin * 60_000 - now;
    const remainMin = Math.max(0, Math.ceil(remainMs / 60_000));
    label = `上次 ${fmtTime(lastSyncAt)} · ${remainMin <= 0 ? '即将刷新' : `${String(remainMin)} 分钟后`}`;
    tone = 'idle';
  }

  const toneCls: Record<typeof tone, string> = {
    sync: 'text-blue-600 dark:text-blue-400',
    fail: 'text-amber-600 dark:text-amber-400',
    off: 'text-slate-400',
    idle: 'text-slate-500 dark:text-slate-400',
  };

  const title =
    errCount > 0
      ? accounts
          .filter((a) => accountErrors[a.id])
          .map((a) => `${a.email}: ${accountErrors[a.id] ?? ''}`)
          .join('\n')
      : lastSyncAt !== null
        ? `上次同步 ${fmtTime(lastSyncAt)}${intervalMin > 0 ? ` · 自动每 ${String(intervalMin)} 分钟` : ''}`
        : '点击立即同步';

  return (
    <button
      type="button"
      onClick={() => void syncAllInbox()}
      disabled={disabled}
      title={title}
      aria-label="自动收信状态，点击立即同步"
      className={`flex items-center gap-1 rounded px-2 py-0.5 text-[10px] font-medium hover:bg-slate-100 disabled:opacity-60 dark:hover:bg-slate-800 ${toneCls[tone]}`}
    >
      <span aria-hidden="true" className={syncing ? 'animate-spin' : ''}>
        {tone === 'fail' ? '⚠' : syncing ? '⟳' : '🔄'}
      </span>
      <span>{label}</span>
    </button>
  );
}
