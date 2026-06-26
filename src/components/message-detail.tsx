// Right pane: header + conversation thread + actions bar.
//
// Body area renders ConversationThread (Task 13). Each ConversationMessage uses BodyView
// internally, which renders HTML via an iframe with srcDoc + sandbox (no allow-same-origin,
// no allow-scripts) plus an injected CSP meta tag. Remote images load by default; scripts blocked.
// selectMessage still fetches store.body so the AI drawer (summary/translate/actions) keeps working.
// AI 摘要/翻译/写信通过操作条触发右侧抽屉展示。

import { useEffect, useMemo, useRef, useState } from 'react';
import { save } from '@tauri-apps/plugin-dialog';

import * as tauri from '../lib/tauri';
import { useMailStore } from '../lib/store/mail';
import type { AttachmentMeta, MessageHeader } from '../lib/types';
import { MessageActions } from './message-actions';
import { ConversationThread } from './conversation-thread';

function selectedMessage(messages: MessageHeader[], id: string | null): MessageHeader | null {
  if (id === null) return null;
  return messages.find((m) => m.id === id) ?? null;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${String(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function MessageDetail() {
  const messages = useMailStore((s) => s.messages);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const conversation = useMailStore((s) => s.conversation);
  const loadingConversation = useMailStore((s) => s.loadingConversation);
  const loadConversation = useMailStore((s) => s.loadConversation);

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

  // 打开邮件时加载对话流。selectMessage 仍会单独拉 body 供 AI 抽屉使用（store.body）。
  // 两者各自连接 IMAP 一次：message_body 拉当前邮件正文，conversation_thread 拉对话其他成员。
  // 当前邮件的正文在 materialize_thread_bodies 里已被缓存，不会重复请求。
  useEffect(() => {
    if (msg?.id) void loadConversation(msg.id);
  }, [msg?.id, loadConversation]);

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
        <div className="flex items-center gap-2">
          <h3 className="min-w-0 truncate text-lg font-semibold text-slate-900 dark:text-slate-100">
            {msg.subject ?? '(无主题)'}
          </h3>
          {conversation && conversation.messages.length > 1 && (
            <span
              className="shrink-0 rounded-full bg-slate-200 px-2 py-0.5 text-xs font-medium text-slate-600 dark:bg-slate-700 dark:text-slate-300"
              title={`此会话共 ${String(conversation.messages.length)} 封`}
            >
              {conversation.messages.length}
            </span>
          )}
          {msg.hasAttachment && (
            <button
              type="button"
              onClick={scrollToAttachments}
              aria-label="跳到下方附件区"
              className="shrink-0 text-amber-600 underline-offset-2 hover:underline dark:text-amber-400"
              title="跳到下方附件区"
            >
              📎
            </button>
          )}
        </div>
        <MessageActions />
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
        {conversation ? (
          <ConversationThread view={conversation} />
        ) : loadingConversation ? (
          <div className="text-sm text-slate-500">加载会话…</div>
        ) : (
          <div className="text-sm text-slate-500">选择一封邮件查看会话。</div>
        )}
      </div>
    </section>
  );
}
