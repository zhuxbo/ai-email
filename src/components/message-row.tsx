import { useMailStore } from '../lib/store/mail';
import type { Category, MessageHeader } from '../lib/types';
import { colorForSeed } from './ui/avatar';

function relativeDate(iso: string | null): string {
  if (!iso) return '';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  const now = new Date();
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();
  if (sameDay) {
    return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }
  const sameYear = date.getFullYear() === now.getFullYear();
  return date.toLocaleDateString(undefined, {
    year: sameYear ? undefined : '2-digit',
    month: '2-digit',
    day: '2-digit',
  });
}

function isUnread(m: MessageHeader): boolean {
  return !m.flags.includes('\\Seen');
}

export const CATEGORY_OPTIONS: { value: Category; label: string; cls: string }[] = [
  {
    value: 'personal',
    label: '私人',
    cls: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300',
  },
  {
    value: 'work',
    label: '工作',
    cls: 'bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-300',
  },
  {
    value: 'notification',
    label: '通知',
    cls: 'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-300',
  },
  {
    value: 'promotion',
    label: '推广',
    cls: 'bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300',
  },
  {
    value: 'spam',
    label: '垃圾',
    cls: 'bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300',
  },
];

function categoryClass(category: Category | null): string {
  if (category === null) return 'bg-slate-100 text-slate-500 dark:bg-slate-800 dark:text-slate-400';
  return CATEGORY_OPTIONS.find((c) => c.value === category)?.cls ?? '';
}

function categoryLabel(category: Category | null): string {
  if (category === null) return '未分类';
  return CATEGORY_OPTIONS.find((c) => c.value === category)?.label ?? category;
}

function priorityBadge(p: number | null): { label: string; cls: string } | null {
  if (p === null) return null;
  if (p === 1) {
    return { label: '高', cls: 'bg-red-500 text-white' };
  }
  if (p === 3) {
    return {
      label: '低',
      cls: 'bg-slate-300 text-slate-700 dark:bg-slate-700 dark:text-slate-300',
    };
  }
  return null; // 2 = normal, no badge
}

export function MessageRow({
  m,
  active,
  onClick,
}: {
  m: MessageHeader;
  active: boolean;
  onClick: () => void;
}) {
  const accounts = useMailStore((s) => s.accounts);
  const account = accounts.find((a) => a.id === m.accountId);
  const dotColor = colorForSeed(account?.email ?? m.accountId);
  const unread = isUnread(m);
  const badge = priorityBadge(m.priority);
  return (
    <li>
      <button
        type="button"
        onClick={onClick}
        style={{ borderLeftColor: dotColor }}
        className={`block w-full border-b border-l-4 border-slate-100 px-3 py-2 text-left transition-colors dark:border-slate-800 ${
          active ? 'bg-blue-50 dark:bg-blue-950' : 'hover:bg-slate-50 dark:hover:bg-slate-800'
        }`}
      >
        <div className="flex items-baseline justify-between gap-2">
          <span className="flex min-w-0 items-center gap-1.5">
            {unread && (
              <span
                data-testid="unread-dot"
                aria-label="未读"
                className="h-2 w-2 shrink-0 rounded-full bg-blue-500"
              />
            )}
            <span
              className={`truncate text-xs ${
                unread
                  ? 'font-semibold text-slate-900 dark:text-slate-100'
                  : 'text-slate-600 dark:text-slate-400'
              }`}
            >
              {m.fromAddr ?? '(无发件人)'}
            </span>
          </span>
          <span className="flex shrink-0 items-center gap-1">
            {m.flags.includes('\\Flagged') && (
              <span aria-label="已加星" title="已加星" className="text-amber-500">
                ★
              </span>
            )}
            <span className="text-xs text-slate-400">{relativeDate(m.sentAt)}</span>
          </span>
        </div>
        <div className="flex items-center gap-2">
          {badge && (
            <span
              className={`shrink-0 rounded px-1 text-[9px] font-bold leading-3 ${badge.cls}`}
              title={`priority ${String(m.priority)}`}
            >
              {badge.label}
            </span>
          )}
          <div
            className={`min-w-0 flex-1 truncate text-sm ${
              unread
                ? 'font-semibold text-slate-900 dark:text-slate-100'
                : 'text-slate-700 dark:text-slate-300'
            }`}
          >
            {m.subject ?? '(无主题)'}
          </div>
        </div>
        {m.snippet && (
          <div className="truncate text-xs text-slate-500 dark:text-slate-400">{m.snippet}</div>
        )}
        {(m.category !== null || m.tags.length > 0) && (
          <div className="mt-1 flex flex-wrap items-center gap-1">
            {m.category !== null && (
              <span
                className={`rounded px-1 py-0.5 text-[9px] font-medium ${categoryClass(m.category)}`}
              >
                {categoryLabel(m.category)}
              </span>
            )}
            {m.tags.slice(0, 3).map((t) => (
              <span
                key={t}
                className="rounded bg-slate-100 px-1 py-0.5 text-[9px] text-slate-600 dark:bg-slate-800 dark:text-slate-300"
              >
                {t}
              </span>
            ))}
          </div>
        )}
      </button>
    </li>
  );
}
