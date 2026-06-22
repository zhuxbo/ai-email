import { useAutoReplyStore } from '../lib/store/auto-reply';
import { useComposeStore } from '../lib/store/compose';
import { useUiStore } from '../lib/store/ui';
import type { SuggestedReply } from '../lib/types';

/** 打开队列项 → 复用 P3a 双语回复台：锁邮件 account、预填规则意图、即时起草、开抽屉。 */
function goReply(s: SuggestedReply): void {
  // 队列 DTO 携带 snippet（JOIN messages 派生），双语检测可凭 subject + snippet，
  // 不再降级；runDraft 内对 draft.body 另有二次 detectForeign 兜底。
  useComposeStore.getState().openReply({
    id: s.messageId,
    accountId: s.accountId,
    fromAddr: s.fromAddr,
    subject: s.subject,
    snippet: s.snippet,
  });
  useComposeStore.getState().setField({ intentZh: s.intentSnapshot });
  void useComposeStore.getState().runDraft();
  useUiStore.getState().openDrawer('compose');
}

export function SuggestedReplyList() {
  const queue = useAutoReplyStore((s) => s.queue);
  const dismiss = useAutoReplyStore((s) => s.dismiss);

  if (queue.length === 0) {
    return (
      <p className="px-4 py-6 text-sm text-text-3">
        暂无建议回复。同步新邮件后，命中规则的会出现在这里。
      </p>
    );
  }

  return (
    <ul className="flex flex-col divide-y divide-[var(--color-border)]">
      {queue.map((s) => (
        <li key={s.id} className="flex flex-col gap-1 px-4 py-3">
          <div className="flex items-baseline justify-between gap-2">
            <span className="truncate text-sm font-medium text-text-1">
              {s.subject ?? '(无主题)'}
            </span>
            <span className="shrink-0 rounded bg-amber-100 px-1 text-[10px] text-amber-700 dark:bg-amber-950 dark:text-amber-300">
              {s.ruleNameSnapshot}
            </span>
          </div>
          <span className="truncate text-xs text-text-3">{s.fromAddr ?? '(无发件人)'}</span>
          <div className="mt-1 flex gap-2">
            <button
              type="button"
              onClick={() => {
                goReply(s);
              }}
              className="rounded bg-accent px-3 py-1 text-xs font-medium text-white hover:opacity-90"
            >
              去回复
            </button>
            <button
              type="button"
              onClick={() => void dismiss(s.id)}
              className="rounded border border-[var(--color-border)] px-3 py-1 text-xs text-text-2 hover:bg-[var(--color-panel)]"
            >
              忽略
            </button>
          </div>
        </li>
      ))}
    </ul>
  );
}
