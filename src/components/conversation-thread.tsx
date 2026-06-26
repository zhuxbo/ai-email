import { useState } from 'react';

import type { ConversationMessage, ConversationView } from '../lib/types';
import { formatDateTimeCN } from '../lib/utils';
import { BodyView } from './body-view';

function MessageBlock({ msg, defaultOpen }: { msg: ConversationMessage; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const who = msg.isOwn ? '我' : (msg.fromAddr ?? '(未知)');
  const when = formatDateTimeCN(msg.sentAt);

  if (!open) {
    return (
      <button
        type="button"
        className="flex w-full items-center gap-2 border-b border-slate-100 px-2 py-1.5 text-left text-sm dark:border-slate-800"
        onClick={() => {
          setOpen(true);
        }}
        aria-label={`展开 ${msg.fromAddr ?? '邮件'} ${when}`}
      >
        <span className="shrink-0 font-medium">{who}</span>
        <span className="min-w-0 flex-1 truncate text-slate-500">
          {msg.snippet ?? msg.textPlain?.slice(0, 80) ?? ''}
        </span>
        <span className="shrink-0 whitespace-nowrap text-xs text-slate-400">{when}</span>
      </button>
    );
  }

  return (
    <div className="border-b border-slate-100 py-2 dark:border-slate-800">
      <button
        type="button"
        className="mb-1 flex w-full items-center justify-between gap-2 text-left text-xs text-slate-500"
        onClick={() => {
          setOpen(false);
        }}
        aria-label={`收起 ${msg.fromAddr ?? '邮件'} ${when}`}
      >
        <span className="font-medium text-slate-700 dark:text-slate-300">{who}</span>
        <span className="whitespace-nowrap">{when}</span>
      </button>
      <BodyView html={msg.html} textPlain={msg.textPlain} />
    </div>
  );
}

export function ConversationThread({ view }: { view: ConversationView }) {
  // 后端 members 按时间升序（剥引用依赖「前序=更早」，顺序不能动）；此处仅反转展示，最新在最上。
  const ordered = [...view.messages].reverse();
  return (
    <div className="conversation-thread">
      {!view.sentSyncOk && (
        <div className="mb-2 rounded bg-amber-50 px-2 py-1 text-xs text-amber-700" role="status">
          已发件箱同步未完成，部分回复可能缺失
        </div>
      )}
      {ordered.map((m, i) => (
        <MessageBlock key={m.id} msg={m} defaultOpen={i === 0} />
      ))}
    </div>
  );
}
