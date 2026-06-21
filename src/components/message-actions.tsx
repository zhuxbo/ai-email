import { useMailStore } from '../lib/store/mail';
import { useComposeStore } from '../lib/store/compose';
import { useUiStore } from '../lib/store/ui';

export function MessageActions() {
  const body = useMailStore((s) => s.body);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const messages = useMailStore((s) => s.messages);

  // 早 return 让闭包里 selectedMessageId 收窄为 string，无需 non-null assertion。
  if (selectedMessageId === null) return null;

  const hasBody = body !== null;
  const message = messages.find((m) => m.id === selectedMessageId);

  return (
    <div className="mt-4 border-t border-[var(--color-border)] pt-3">
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => {
            if (message) {
              useComposeStore.getState().openReply(message);
              useUiStore.getState().openDrawer('compose');
            }
          }}
          disabled={!hasBody}
          className="rounded bg-accent px-3 py-1 text-xs font-medium text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          title={hasBody ? '回复这封邮件' : '等待正文加载后才能回复'}
        >
          回复
        </button>
        <button
          type="button"
          disabled={!hasBody}
          onClick={() => {
            useUiStore.getState().openDrawer('summary');
          }}
          className="rounded border border-[var(--color-border)] bg-panel px-3 py-1 text-xs font-medium text-text-1 hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          title={hasBody ? '查看 AI 摘要' : '等待正文加载后可用'}
        >
          摘要
        </button>
        <button
          type="button"
          disabled={!hasBody}
          onClick={() => {
            useUiStore.getState().openDrawer('translate');
          }}
          className="rounded border border-[var(--color-border)] bg-panel px-3 py-1 text-xs font-medium text-text-1 hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          title={hasBody ? '翻译这封邮件' : '等待正文加载后可用'}
        >
          翻译
        </button>
        <button
          type="button"
          disabled
          title="P3b 接入"
          className="rounded border border-[var(--color-border)] px-3 py-1 text-xs font-medium text-text-3 disabled:cursor-not-allowed"
        >
          归档
        </button>
        <button
          type="button"
          disabled
          title="P3b 接入"
          className="rounded border border-[var(--color-border)] px-3 py-1 text-xs font-medium text-text-3 disabled:cursor-not-allowed"
        >
          删除
        </button>
      </div>
    </div>
  );
}
