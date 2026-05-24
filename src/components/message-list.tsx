// Middle pane: INBOX rows. Sprint 3 added category filter chips + priority badges + tags.
//
// Filter / sort run client-side over the already-fetched 50 rows; we don't refetch on toggle
// to keep the UX snappy. When the next sync lands new rows + classifications, the store's
// delayed reload picks them up.

import { useMemo } from 'react';

import { useMailStore } from '../lib/store/mail';
import type { Category, MessageHeader } from '../lib/types';

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

const CATEGORY_OPTIONS: { value: Category; label: string; cls: string }[] = [
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

export function MessageList() {
  const messages = useMailStore((s) => s.messages);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const selectedMailboxId = useMailStore((s) => s.selectedMailboxId);
  const selectMessage = useMailStore((s) => s.selectMessage);
  const categoryFilter = useMailStore((s) => s.categoryFilter);
  const sortByPriority = useMailStore((s) => s.sortByPriority);
  const toggleCategoryFilter = useMailStore((s) => s.toggleCategoryFilter);
  const setSortByPriority = useMailStore((s) => s.setSortByPriority);

  const filtered = useMemo(() => {
    const base =
      categoryFilter.length === 0
        ? messages
        : messages.filter((m) => m.category !== null && categoryFilter.includes(m.category));
    if (!sortByPriority) return base;
    // Stable-sort by priority ascending (1=high first); null priority sorts last.
    return [...base].sort((a, b) => {
      const ap = a.priority ?? 99;
      const bp = b.priority ?? 99;
      return ap - bp;
    });
  }, [messages, categoryFilter, sortByPriority]);

  return (
    <section className="flex h-full w-96 flex-col border-r border-slate-200 bg-white dark:border-slate-700 dark:bg-slate-900">
      <header className="border-b border-slate-200 px-3 py-3 dark:border-slate-700">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold text-slate-700 dark:text-slate-200">
            收件箱{' '}
            <span className="text-xs font-normal text-slate-400">
              ({filtered.length}
              {categoryFilter.length > 0 && ` / ${String(messages.length)}`})
            </span>
          </h2>
          <button
            type="button"
            onClick={() => {
              setSortByPriority(!sortByPriority);
            }}
            className={`rounded px-2 py-0.5 text-[10px] font-medium ${
              sortByPriority
                ? 'bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-300'
                : 'text-slate-500 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800'
            }`}
            title="按 priority 排序"
          >
            ↑ 优先
          </button>
        </div>
        <div className="mt-2 flex flex-wrap gap-1">
          {CATEGORY_OPTIONS.map((c) => {
            const active = categoryFilter.includes(c.value);
            return (
              <button
                key={c.value}
                type="button"
                onClick={() => {
                  toggleCategoryFilter(c.value);
                }}
                className={`rounded px-1.5 py-0.5 text-[10px] font-medium transition-colors ${
                  active
                    ? c.cls
                    : 'text-slate-500 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800'
                }`}
              >
                {c.label}
              </button>
            );
          })}
          {categoryFilter.length > 0 && (
            <button
              type="button"
              onClick={() => {
                categoryFilter.forEach((c) => {
                  toggleCategoryFilter(c);
                });
              }}
              className="rounded px-1.5 py-0.5 text-[10px] text-slate-400 hover:text-slate-700 dark:hover:text-slate-200"
            >
              清空
            </button>
          )}
        </div>
      </header>

      <ul className="flex-1 overflow-auto">
        {selectedMailboxId === null ? (
          <li className="px-3 py-6 text-center text-xs text-slate-500 dark:text-slate-400">
            选择一个账户开始。
          </li>
        ) : filtered.length === 0 ? (
          <li className="px-3 py-6 text-center text-xs text-slate-500 dark:text-slate-400">
            {messages.length === 0
              ? '该邮箱为空 — 点击左侧「同步收件箱」拉取最新邮件。'
              : '当前过滤条件下没有邮件。'}
          </li>
        ) : (
          filtered.map((m) => (
            <MessageRow
              key={m.id}
              m={m}
              active={m.id === selectedMessageId}
              onClick={() => void selectMessage(m.id)}
            />
          ))
        )}
      </ul>
    </section>
  );
}

function MessageRow({
  m,
  active,
  onClick,
}: {
  m: MessageHeader;
  active: boolean;
  onClick: () => void;
}) {
  const unread = isUnread(m);
  const badge = priorityBadge(m.priority);
  return (
    <li>
      <button
        type="button"
        onClick={onClick}
        className={`block w-full border-b border-slate-100 px-3 py-2 text-left transition-colors dark:border-slate-800 ${
          active ? 'bg-blue-50 dark:bg-blue-950' : 'hover:bg-slate-50 dark:hover:bg-slate-800'
        }`}
      >
        <div className="flex items-baseline justify-between gap-2">
          <span
            className={`truncate text-xs ${
              unread
                ? 'font-semibold text-slate-900 dark:text-slate-100'
                : 'text-slate-600 dark:text-slate-400'
            }`}
          >
            {m.fromAddr ?? '(无发件人)'}
          </span>
          <span className="shrink-0 text-xs text-slate-400">{relativeDate(m.sentAt)}</span>
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
