// Left pane: every configured account + add-account button + sync trigger for the selected
// one. Keep this minimal — multi-account UX (switcher, drag-reorder) lands in Sprint 6.

import { useMailStore } from '../lib/store/mail';

interface Props {
  onAddAccount: () => void;
  onOpenAiSettings: () => void;
}

export function AccountList({ onAddAccount, onOpenAiSettings }: Props) {
  const accounts = useMailStore((s) => s.accounts);
  const selectedAccountId = useMailStore((s) => s.selectedAccountId);
  const syncing = useMailStore((s) => s.syncing);
  const selectAccount = useMailStore((s) => s.selectAccount);
  const syncInbox = useMailStore((s) => s.syncInbox);
  const removeAccount = useMailStore((s) => s.removeAccount);

  return (
    <aside className="flex h-full w-60 flex-col border-r border-slate-200 bg-white dark:border-slate-700 dark:bg-slate-900">
      <header className="flex items-center justify-between border-b border-slate-200 px-3 py-3 dark:border-slate-700">
        <h1 className="text-sm font-semibold text-slate-700 dark:text-slate-200">账户</h1>
        <button
          type="button"
          onClick={onAddAccount}
          className="rounded bg-blue-600 px-2 py-1 text-xs font-medium text-white hover:bg-blue-700"
        >
          + 添加
        </button>
      </header>

      <ul className="flex-1 overflow-auto py-1">
        {accounts.length === 0 ? (
          <li className="px-3 py-6 text-center text-xs text-slate-500 dark:text-slate-400">
            尚未配置账户。点击右上角「+ 添加」开始。
          </li>
        ) : (
          accounts.map((a) => {
            const active = a.id === selectedAccountId;
            return (
              <li key={a.id}>
                <button
                  type="button"
                  onClick={() => {
                    void selectAccount(a.id);
                  }}
                  className={`block w-full truncate px-3 py-2 text-left text-sm transition-colors ${
                    active
                      ? 'bg-blue-50 text-blue-700 dark:bg-blue-950 dark:text-blue-200'
                      : 'text-slate-700 hover:bg-slate-50 dark:text-slate-200 dark:hover:bg-slate-800'
                  }`}
                  title={a.email}
                >
                  <div className="truncate font-medium">{a.displayName ?? a.email}</div>
                  {a.displayName && (
                    <div className="truncate text-xs text-slate-500 dark:text-slate-400">
                      {a.email}
                    </div>
                  )}
                </button>
              </li>
            );
          })
        )}
      </ul>

      {selectedAccountId && (
        <div className="flex gap-2 border-t border-slate-200 p-2 dark:border-slate-700">
          <button
            type="button"
            disabled={syncing}
            onClick={() => {
              void syncInbox(selectedAccountId);
            }}
            className="flex-1 rounded bg-slate-100 px-2 py-1 text-xs font-medium text-slate-700 hover:bg-slate-200 disabled:opacity-50 dark:bg-slate-800 dark:text-slate-200 dark:hover:bg-slate-700"
          >
            {syncing ? '同步中…' : '同步收件箱'}
          </button>
          <button
            type="button"
            onClick={() => {
              if (window.confirm('确认移除该账户？授权码会从 keychain 删除，本地邮件清空。')) {
                void removeAccount(selectedAccountId);
              }
            }}
            className="rounded px-2 py-1 text-xs font-medium text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
          >
            删除
          </button>
        </div>
      )}

      <footer className="border-t border-slate-200 p-2 dark:border-slate-700">
        <button
          type="button"
          onClick={onOpenAiSettings}
          className="block w-full rounded px-2 py-1.5 text-left text-xs font-medium text-slate-600 hover:bg-slate-50 dark:text-slate-300 dark:hover:bg-slate-800"
        >
          ⚙ AI 模型配置
        </button>
      </footer>
    </aside>
  );
}
