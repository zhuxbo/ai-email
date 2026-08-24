// 全局搜索结果行：扁平单封（无折叠），在主题/发件人/时间之外额外展示
// 信箱 + 账户归属徽标与「正文」命中标记，让用户知道为什么命中、邮件在哪里。

import type { SearchHit } from '../lib/types';
import { formatDateTimeCN } from '../lib/utils';
import { colorForSeed } from './ui/avatar';

export function SearchResultRow({
  hit,
  active,
  onClick,
}: {
  hit: SearchHit;
  active: boolean;
  onClick: () => void;
}) {
  const unread = !hit.flags.includes('\\Seen');
  const dotColor = colorForSeed(hit.accountEmail);
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
              {hit.fromAddr ?? '(无发件人)'}
            </span>
          </span>
          <span className="flex shrink-0 items-center gap-1">
            {hit.flags.includes('\\Flagged') && (
              <span aria-label="已加星" title="已加星" className="text-amber-500">
                ★
              </span>
            )}
            <span className="whitespace-nowrap text-xs text-slate-400">
              {formatDateTimeCN(hit.sentAt)}
            </span>
          </span>
        </div>
        <div
          className={`truncate text-sm ${
            unread
              ? 'font-semibold text-slate-900 dark:text-slate-100'
              : 'text-slate-700 dark:text-slate-300'
          }`}
        >
          {hit.subject ?? '(无主题)'}
        </div>
        {hit.snippet && (
          <div className="truncate text-xs text-slate-500 dark:text-slate-400">{hit.snippet}</div>
        )}
        <div className="mt-1 flex flex-wrap items-center gap-1">
          <span className="rounded bg-slate-100 px-1 py-0.5 text-[9px] text-slate-600 dark:bg-slate-800 dark:text-slate-300">
            {hit.mailboxName}
          </span>
          <span
            className="max-w-[14rem] truncate rounded bg-slate-100 px-1 py-0.5 text-[9px] text-slate-500 dark:bg-slate-800 dark:text-slate-400"
            title={hit.accountEmail}
          >
            {hit.accountEmail}
          </span>
          {hit.bodyMatched && (
            <span className="rounded bg-blue-100 px-1 py-0.5 text-[9px] font-medium text-blue-700 dark:bg-blue-950 dark:text-blue-300">
              正文
            </span>
          )}
        </div>
      </button>
    </li>
  );
}
