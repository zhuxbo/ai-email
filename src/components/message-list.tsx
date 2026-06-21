// Middle pane: aggregated INBOX rows across accounts (front-end aggregation via the store's
// unifiedInbox). Filter / search / sort run client-side over the already-fetched window; we
// don't refetch on toggle to keep the UX snappy. Source-color dots + rows live in message-row.

import { useMemo } from 'react';

import { useMailStore } from '../lib/store/mail';
import { CATEGORY_OPTIONS, MessageRow } from './message-row';

export function MessageList() {
  const messages = useMailStore((s) => s.messages);
  const accounts = useMailStore((s) => s.accounts);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const selectMessage = useMailStore((s) => s.selectMessage);
  const categoryFilter = useMailStore((s) => s.categoryFilter);
  const sortByPriority = useMailStore((s) => s.sortByPriority);
  const query = useMailStore((s) => s.query);
  const accountErrors = useMailStore((s) => s.accountErrors);
  const toggleCategoryFilter = useMailStore((s) => s.toggleCategoryFilter);
  const setSortByPriority = useMailStore((s) => s.setSortByPriority);

  // 叠加顺序：query（发件人/主题/snippet 子串，大小写不敏感）→ categoryFilter → sortByPriority。
  // 作用于已聚合的局部窗口（各账户前 50），非全局；真全局优先排序留阶段2。
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const searched =
      q === ''
        ? messages
        : messages.filter(
            (m) =>
              (m.fromAddr?.toLowerCase().includes(q) ?? false) ||
              (m.subject?.toLowerCase().includes(q) ?? false) ||
              (m.snippet?.toLowerCase().includes(q) ?? false),
          );
    const base =
      categoryFilter.length === 0
        ? searched
        : searched.filter((m) => m.category !== null && categoryFilter.includes(m.category));
    if (!sortByPriority) return base;
    // Stable-sort by priority ascending (1=high first); null priority sorts last.
    return [...base].sort((a, b) => {
      const ap = a.priority ?? 99;
      const bp = b.priority ?? 99;
      return ap - bp;
    });
  }, [messages, categoryFilter, sortByPriority, query]);

  // 聚合层把部分账户的加载/同步失败汇到 accountErrors；这里映射成邮箱地址做提示。
  const failedAccounts = useMemo(
    () => Object.keys(accountErrors).map((id) => accounts.find((a) => a.id === id)?.email ?? id),
    [accountErrors, accounts],
  );

  const hasFilter = categoryFilter.length > 0 || query.trim() !== '';

  return (
    <section className="flex h-full w-full flex-col bg-white md:w-96 md:border-r border-slate-200 dark:border-slate-700 dark:bg-slate-900">
      <header className="border-b border-slate-200 px-3 py-3 dark:border-slate-700">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold text-slate-700 dark:text-slate-200">
            收件箱{' '}
            <span className="text-xs font-normal text-slate-400">
              ({filtered.length}
              {hasFilter && ` / ${String(messages.length)}`})
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

      {failedAccounts.length > 0 && (
        <div
          role="alert"
          className="border-b border-amber-200 bg-amber-50 px-3 py-1.5 text-[11px] text-amber-700 dark:border-amber-900/60 dark:bg-amber-950/40 dark:text-amber-300"
        >
          ⚠ {failedAccounts.length} 个账户加载失败：{failedAccounts.join('、')}
        </div>
      )}

      <ul className="flex-1 overflow-auto">
        {accounts.length === 0 ? (
          <li className="px-3 py-6 text-center text-xs text-slate-500 dark:text-slate-400">
            还没有账户，点左下角 ＋ 添加。
          </li>
        ) : messages.length === 0 ? (
          <li className="px-3 py-6 text-center text-xs text-slate-500 dark:text-slate-400">
            收件箱为空，点左侧 🔄 同步。
          </li>
        ) : filtered.length === 0 ? (
          <li className="px-3 py-6 text-center text-xs text-slate-500 dark:text-slate-400">
            当前条件无邮件。
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
