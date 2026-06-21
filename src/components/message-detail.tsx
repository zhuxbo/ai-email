// Right pane: header + body (text or html with sandbox) + actions bar.
//
// HTML rendering uses an iframe with `srcDoc` + a `sandbox` attribute that disables scripts
// and same-origin escape — so a malicious email can't pull cookies, exec JS, or beacon out.
// Plain-text fallback if the message has no HTML. Reply moved into the actions bar.
// AI 摘要/翻译/写信通过操作条触发右侧抽屉展示。

import { useMemo } from 'react';

import { useMailStore } from '../lib/store/mail';
import type { MessageHeader } from '../lib/types';
import { MessageActions } from './message-actions';

function selectedMessage(messages: MessageHeader[], id: string | null): MessageHeader | null {
  if (id === null) return null;
  return messages.find((m) => m.id === id) ?? null;
}

function formatDateTime(iso: string | null): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return d.toLocaleString(undefined, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function MessageDetail() {
  const messages = useMailStore((s) => s.messages);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const body = useMailStore((s) => s.body);
  const loadingBody = useMailStore((s) => s.loadingBody);

  const msg = useMemo(
    () => selectedMessage(messages, selectedMessageId),
    [messages, selectedMessageId],
  );

  if (!msg) {
    return (
      <section className="flex h-full flex-1 items-center justify-center text-sm text-slate-400 dark:text-slate-500">
        在左侧选择一封邮件查看内容
      </section>
    );
  }

  return (
    <section className="flex h-full flex-1 flex-col bg-slate-50 dark:bg-slate-950">
      <header className="border-b border-slate-200 bg-white px-6 py-4 dark:border-slate-700 dark:bg-slate-900">
        <h3 className="text-lg font-semibold text-slate-900 dark:text-slate-100">
          {msg.subject ?? '(无主题)'}
        </h3>
        <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs text-slate-600 dark:text-slate-400">
          <dt className="font-medium">发件人</dt>
          <dd className="break-all">{msg.fromAddr ?? '—'}</dd>
          <dt className="font-medium">收件人</dt>
          <dd className="break-all">{msg.toAddrs.join(', ') || '—'}</dd>
          {msg.ccAddrs.length > 0 && (
            <>
              <dt className="font-medium">抄送</dt>
              <dd className="break-all">{msg.ccAddrs.join(', ')}</dd>
            </>
          )}
          <dt className="font-medium">时间</dt>
          <dd>{formatDateTime(msg.sentAt)}</dd>
          {msg.hasAttachment && (
            <>
              <dt className="font-medium">附件</dt>
              <dd className="text-amber-600 dark:text-amber-400">📎 含附件</dd>
            </>
          )}
        </dl>
      </header>

      <div className="flex-1 overflow-auto p-6">
        {loadingBody && <div className="text-sm text-slate-500">正在加载正文…</div>}
        {!loadingBody && body && <BodyView body={body} />}
        {!loadingBody && !body && (
          <div className="text-sm text-slate-500">无法加载正文 — 检查左下角错误提示。</div>
        )}
        <MessageActions />
      </div>
    </section>
  );
}

function BodyView({
  body,
}: {
  body: NonNullable<ReturnType<typeof useMailStore.getState>['body']>;
}) {
  if (body.html) {
    return (
      <iframe
        // `allow-popups` lets clicks on links open the system browser (Tauri intercepts those);
        // we still disable scripts + same-origin so the page is rendered as inert HTML.
        sandbox="allow-popups"
        srcDoc={body.html}
        title="message body"
        className="h-full min-h-[300px] w-full rounded border border-slate-200 bg-white dark:border-slate-700 dark:bg-slate-900"
      />
    );
  }
  if (body.textPlain) {
    return (
      <pre className="whitespace-pre-wrap break-words rounded border border-slate-200 bg-white p-4 font-sans text-sm text-slate-800 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200">
        {body.textPlain}
      </pre>
    );
  }
  return <div className="text-sm text-slate-500">这封邮件没有可显示的正文。</div>;
}
