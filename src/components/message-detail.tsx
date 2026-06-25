// Right pane: header + body (text or html with sandbox) + actions bar.
//
// HTML rendering uses an iframe with `srcDoc` + a `sandbox` attribute (no allow-same-origin,
// no allow-scripts) plus an injected Content-Security-Policy meta tag. Remote images load by
// default (img-src allows http/https) so email images show without a click; scripts/fonts/
// frames stay blocked by default-src 'none' + the no-scripts sandbox. Trade-off: remote images
// include tracking pixels, which will load when a message is opened.
// Plain-text fallback if the message has no HTML. Reply moved into the actions bar.
// AI 摘要/翻译/写信通过操作条触发右侧抽屉展示。

import { useEffect, useMemo, useRef, useState } from 'react';
import { save } from '@tauri-apps/plugin-dialog';

import * as tauri from '../lib/tauri';
import { useMailStore } from '../lib/store/mail';
import type { AttachmentMeta, MessageHeader } from '../lib/types';
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

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${String(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
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

  const [attachments, setAttachments] = useState<AttachmentMeta[]>([]);
  const [attachmentsLoading, setAttachmentsLoading] = useState(false);
  const attachmentsRef = useRef<HTMLDivElement>(null);
  // 打开有附件的邮件时按需取附件列表（不入库）。active 守卫防切换邮件时迟到结果覆盖。
  useEffect(() => {
    if (!msg?.hasAttachment) {
      setAttachments([]);
      setAttachmentsLoading(false);
      return;
    }
    let active = true;
    const id = msg.id;
    setAttachmentsLoading(true);
    void tauri
      .messageAttachments(id)
      .then((atts) => {
        if (active) {
          setAttachments(atts);
          setAttachmentsLoading(false);
        }
      })
      .catch(() => {
        if (active) {
          setAttachments([]);
          setAttachmentsLoading(false);
        }
      });
    return () => {
      active = false;
    };
  }, [msg?.id, msg?.hasAttachment]);

  function scrollToAttachments() {
    attachmentsRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  if (!msg) {
    return (
      <section className="flex h-full flex-1 items-center justify-center text-sm text-slate-400 dark:text-slate-500">
        在左侧选择一封邮件查看内容
      </section>
    );
  }

  const selected = msg;
  async function downloadAttachment(index: number, att: AttachmentMeta) {
    // 用户「另存为」选定路径后由后端写盘（dialog 已授权写出 app 数据目录外）。
    // 写盘失败 surface 到全局错误条，避免静默 + unhandled rejection。
    try {
      const path = await save({ defaultPath: att.filename });
      if (typeof path === 'string') {
        await tauri.messageAttachmentSave(selected.id, index, path);
      }
    } catch (e) {
      useMailStore.setState({ error: e instanceof Error ? e.message : '下载附件失败' });
    }
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
              <dd>
                <button
                  type="button"
                  onClick={scrollToAttachments}
                  className="text-amber-600 underline-offset-2 hover:underline dark:text-amber-400"
                  title="跳到下方附件区"
                >
                  📎 含附件
                </button>
              </dd>
            </>
          )}
        </dl>
      </header>

      <div className="flex-1 overflow-auto p-6">
        {msg.hasAttachment && (
          <div
            ref={attachmentsRef}
            className="mb-4 rounded border border-slate-200 bg-white p-3 dark:border-slate-700 dark:bg-slate-900"
          >
            <div className="mb-2 text-xs font-medium text-slate-600 dark:text-slate-300">
              附件{attachments.length > 0 ? ` (${String(attachments.length)})` : ''}
            </div>
            {attachmentsLoading ? (
              <div className="text-xs text-slate-500 dark:text-slate-400">正在读取附件…</div>
            ) : attachments.length === 0 ? (
              <div className="text-xs text-slate-500 dark:text-slate-400">没有可下载的附件。</div>
            ) : (
              <ul className="flex flex-wrap gap-2">
                {attachments.map((a, i) => (
                  <li key={`${a.filename}-${String(i)}`}>
                    <button
                      type="button"
                      onClick={() => void downloadAttachment(i, a)}
                      className="flex items-center gap-1 rounded border border-slate-200 px-2 py-1 text-xs text-slate-700 hover:bg-slate-100 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
                      title="下载附件"
                    >
                      📎 {a.filename} · {formatSize(a.size)}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
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

// CSP injected into every HTML email srcdoc:
// - default-src 'none' 兜底屏蔽脚本 / 字体 / frame / XHR 等远程资源
// - style-src 'unsafe-inline' allows inline CSS (needed for most HTML email layouts)
// - img-src data: https: http: 默认下载远程图片（含 data 内嵌图）。取舍：放开图片即允许
//   tracking pixel 加载（暴露已读 + IP）；脚本仍由 sandbox(无 allow-scripts) + default-src
//   'none' 双重屏蔽，放开图片不影响 XSS 防护。
// Links to external sites are handled by allow-popups on the sandbox (Tauri intercepts them).
const EMAIL_CSP =
  `<meta http-equiv="Content-Security-Policy" ` +
  `content="default-src 'none'; style-src 'unsafe-inline'; img-src data: https: http:;">`;

function buildSrcdoc(html: string): string {
  return `${EMAIL_CSP}${html}`;
}

function BodyView({
  body,
}: {
  body: NonNullable<ReturnType<typeof useMailStore.getState>['body']>;
}) {
  if (body.html) {
    return (
      <iframe
        // sandbox without allow-same-origin: scripts are blocked and the iframe cannot
        // access parent-frame cookies or storage. allow-popups lets link clicks open in
        // the system browser (Tauri intercepts navigation requests).
        sandbox="allow-popups"
        srcDoc={buildSrcdoc(body.html)}
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
