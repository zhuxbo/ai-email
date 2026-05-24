// Middle pane: last 50 INBOX rows by sent_at DESC. Truncated subject + from + snippet.
// Sprint 3 adds priority chips + filter pills; for now it's a flat list.

import { useMailStore } from '../lib/store/mail';
import type { MessageHeader } from '../lib/types';

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

export function MessageList() {
  const messages = useMailStore((s) => s.messages);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const selectedMailboxId = useMailStore((s) => s.selectedMailboxId);
  const selectMessage = useMailStore((s) => s.selectMessage);

  return (
    <section className="flex h-full w-96 flex-col border-r border-slate-200 bg-white dark:border-slate-700 dark:bg-slate-900">
      <header className="flex items-center justify-between border-b border-slate-200 px-3 py-3 dark:border-slate-700">
        <h2 className="text-sm font-semibold text-slate-700 dark:text-slate-200">
          收件箱 <span className="text-xs font-normal text-slate-400">({messages.length})</span>
        </h2>
      </header>

      <ul className="flex-1 overflow-auto">
        {selectedMailboxId === null ? (
          <li className="px-3 py-6 text-center text-xs text-slate-500 dark:text-slate-400">
            选择一个账户开始。
          </li>
        ) : messages.length === 0 ? (
          <li className="px-3 py-6 text-center text-xs text-slate-500 dark:text-slate-400">
            该邮箱为空 — 点击左侧「同步收件箱」拉取最新邮件。
          </li>
        ) : (
          messages.map((m) => {
            const active = m.id === selectedMessageId;
            const unread = isUnread(m);
            return (
              <li key={m.id}>
                <button
                  type="button"
                  onClick={() => {
                    void selectMessage(m.id);
                  }}
                  className={`block w-full border-b border-slate-100 px-3 py-2 text-left transition-colors dark:border-slate-800 ${
                    active
                      ? 'bg-blue-50 dark:bg-blue-950'
                      : 'hover:bg-slate-50 dark:hover:bg-slate-800'
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
                    <span className="shrink-0 text-xs text-slate-400">
                      {relativeDate(m.sentAt)}
                    </span>
                  </div>
                  <div
                    className={`truncate text-sm ${
                      unread
                        ? 'font-semibold text-slate-900 dark:text-slate-100'
                        : 'text-slate-700 dark:text-slate-300'
                    }`}
                  >
                    {m.subject ?? '(无主题)'}
                  </div>
                  {m.snippet && (
                    <div className="truncate text-xs text-slate-500 dark:text-slate-400">
                      {m.snippet}
                    </div>
                  )}
                </button>
              </li>
            );
          })
        )}
      </ul>
    </section>
  );
}
