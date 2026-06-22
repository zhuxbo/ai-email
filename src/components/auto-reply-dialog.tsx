// 自动回复中心对话框：队列（建议回复）+ 规则两区。
//   • 打开即 loadQueue（聚合，跨账户）。
//   • 规则按账户编辑；多账户时顶部 select 切换编辑目标，AutoReplyRules 自行 loadRules。

import { useEffect, useState } from 'react';

import { useAutoReplyStore } from '../lib/store/auto-reply';
import { useMailStore } from '../lib/store/mail';
import { AutoReplyRules } from './auto-reply-rules';
import { SuggestedReplyList } from './suggested-reply-list';

interface Props {
  open: boolean;
  onClose: () => void;
}

export function AutoReplyDialog({ open, onClose }: Props) {
  const accounts = useMailStore((s) => s.accounts);
  const selectedAccountId = useMailStore((s) => s.selectedAccountId);
  const loadQueue = useAutoReplyStore((s) => s.loadQueue);
  const [ruleAccount, setRuleAccount] = useState<string | null>(null);

  useEffect(() => {
    if (open) void loadQueue();
  }, [open, loadQueue]);

  useEffect(() => {
    if (ruleAccount === null) setRuleAccount(selectedAccountId ?? accounts[0]?.id ?? null);
  }, [accounts, selectedAccountId, ruleAccount]);

  if (!open) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="自动回复中心"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4"
      onClick={onClose}
    >
      <div
        onClick={(e) => {
          e.stopPropagation();
        }}
        className="flex max-h-[90vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg bg-[var(--color-panel)] shadow-xl"
      >
        <header className="flex items-center justify-between border-b border-[var(--color-border)] px-6 py-3">
          <h2 className="text-lg font-semibold text-text-1">自动回复中心</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭"
            className="text-text-3 hover:text-text-1"
          >
            ×
          </button>
        </header>

        <div className="flex flex-1 flex-col gap-6 overflow-auto px-6 py-4">
          <section>
            <h3 className="mb-2 text-sm font-semibold text-text-2">建议回复队列</h3>
            <SuggestedReplyList />
          </section>
          <section>
            <h3 className="mb-2 text-sm font-semibold text-text-2">规则管理</h3>
            {accounts.length > 1 && (
              <select
                aria-label="规则所属账户"
                value={ruleAccount ?? ''}
                onChange={(e) => {
                  setRuleAccount(e.target.value);
                }}
                className="mb-2 rounded border border-[var(--color-border)] px-2 py-1 text-sm"
              >
                {accounts.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.email}
                  </option>
                ))}
              </select>
            )}
            {ruleAccount !== null ? (
              <AutoReplyRules accountId={ruleAccount} />
            ) : (
              <p className="text-sm text-text-3">先添加邮箱账户再配置规则。</p>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}
