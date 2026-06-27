import { useState } from 'react';
import { useMailStore } from '../lib/store/mail';
import { useComposeStore } from '../lib/store/compose';
import { useUiStore } from '../lib/store/ui';
import { CategoryDialog } from './category-dialog';

export function MessageActions() {
  const body = useMailStore((s) => s.body);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const messages = useMailStore((s) => s.messages);
  const deleteMessage = useMailStore((s) => s.deleteMessage);
  const setSeen = useMailStore((s) => s.setSeen);
  const setFlagged = useMailStore((s) => s.setFlagged);
  const setCategoryLocal = useMailStore((s) => s.setCategoryLocal);

  const [categoryOpen, setCategoryOpen] = useState(false);

  // 早 return 让闭包里 selectedMessageId 收窄为 string，无需 non-null assertion。
  if (selectedMessageId === null) return null;

  const hasBody = body !== null;
  const message = messages.find((m) => m.id === selectedMessageId);

  const seen = message?.flags.includes('\\Seen') ?? false;
  const flagged = message?.flags.includes('\\Flagged') ?? false;

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
        <span aria-hidden className="mx-1 w-px self-stretch bg-[var(--color-border)]" />
        <button
          type="button"
          disabled={message === undefined}
          onClick={() => {
            setCategoryOpen(true);
          }}
          className="rounded border border-[var(--color-border)] bg-panel px-3 py-1 text-xs font-medium text-text-1 hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          title={message ? '修改此邮件的分类' : '选中邮件后可用'}
        >
          分类
        </button>
        <button
          type="button"
          disabled={message === undefined}
          onClick={() => {
            if (message && window.confirm('删除这封邮件？将移到废纸篓，可在邮箱中找回。')) {
              void deleteMessage(message.id);
            }
          }}
          className="rounded border border-[var(--color-border)] bg-panel px-3 py-1 text-xs font-medium text-text-1 hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          title={message ? '将此邮件移到废纸篓' : '选中邮件后可用'}
        >
          删除
        </button>
        <button
          type="button"
          disabled={message === undefined}
          onClick={() => {
            if (message) void setSeen(message.id, !seen);
          }}
          className="rounded border border-[var(--color-border)] bg-panel px-3 py-1 text-xs font-medium text-text-1 hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          title={message ? (seen ? '标记为未读' : '标记为已读') : '选中邮件后可用'}
        >
          {seen ? '标记未读' : '标记已读'}
        </button>
        <button
          type="button"
          disabled={message === undefined}
          onClick={() => {
            if (message) void setFlagged(message.id, !flagged);
          }}
          className="rounded border border-[var(--color-border)] bg-panel px-3 py-1 text-xs font-medium text-text-1 hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          title={message ? (flagged ? '取消星标' : '添加星标') : '选中邮件后可用'}
        >
          {flagged ? '取消加星' : '加星'}
        </button>
      </div>
      <CategoryDialog
        open={categoryOpen}
        messageId={selectedMessageId}
        current={message?.category ?? null}
        onClose={() => {
          setCategoryOpen(false);
        }}
        onConfirm={(msgId, category) => {
          void setCategoryLocal(msgId, category);
        }}
      />
    </div>
  );
}
