// Right pane: header + conversation thread + actions bar.
//
// Body area renders ConversationThread. Each ConversationMessage uses BodyView internally,
// which sanitizes HTML with DOMPurify and renders it inside a Shadow DOM (isolates the email's
// CSS; height auto-fits the content). Scripts / on* / javascript: are stripped; remote images
// load by default only for personal/work mail, otherwise blocked until the user opts in.
// selectMessage still fetches store.body so the AI drawer (summary/translate/actions) keeps working.
// AI 摘要/翻译/写信通过操作条触发右侧抽屉展示。

import { useEffect, useMemo } from 'react';

import { useMailStore } from '../lib/store/mail';
import type { MessageHeader } from '../lib/types';
import { MessageActions } from './message-actions';
import { ConversationThread } from './conversation-thread';
import { SenderGroupView } from './sender-group-view';

function selectedMessage(messages: MessageHeader[], id: string | null): MessageHeader | null {
  if (id === null) return null;
  return messages.find((m) => m.id === id) ?? null;
}

export function MessageDetail() {
  const messages = useMailStore((s) => s.messages);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const conversation = useMailStore((s) => s.conversation);
  const loadingConversation = useMailStore((s) => s.loadingConversation);
  const loadConversation = useMailStore((s) => s.loadConversation);
  const detailMode = useMailStore((s) => s.detailMode);
  const senderGroup = useMailStore((s) => s.senderGroup);

  const msg = useMemo(
    () => selectedMessage(messages, selectedMessageId),
    [messages, selectedMessageId],
  );

  // 打开邮件时加载对话流。selectMessage 仍会单独拉 body 供 AI 抽屉使用（store.body）。
  // 两者各自连接 IMAP 一次：message_body 拉当前邮件正文，conversation_thread 拉对话其他成员。
  // 当前邮件的正文在 materialize_thread_bodies 里已被缓存，不会重复请求。
  useEffect(() => {
    if (msg?.id) void loadConversation(msg.id);
  }, [msg?.id, loadConversation]);

  // 同发件人组视图：与单封会话详情互斥，按 detailMode 单独渲染（不走下方按 msg 的单封分支）。
  if (detailMode === 'senderGroup') {
    return (
      <section className="flex h-full flex-1 flex-col bg-slate-50 dark:bg-slate-950">
        <header className="border-b border-slate-200 bg-white px-6 py-4 dark:border-slate-700 dark:bg-slate-900">
          <h3 className="min-w-0 truncate text-lg font-semibold text-slate-900 dark:text-slate-100">
            {senderGroup?.messages[0]?.fromAddr ?? '同发件人'}
          </h3>
        </header>
        <div className="flex-1 overflow-auto p-6">
          {senderGroup ? (
            <SenderGroupView view={senderGroup} />
          ) : (
            <div className="text-sm text-slate-500">加载邮件…</div>
          )}
        </div>
      </section>
    );
  }

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
        </div>
        <MessageActions />
      </header>

      <div className="flex-1 overflow-auto p-6">
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
