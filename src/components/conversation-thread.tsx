import { useState } from 'react';
import type { ConversationMessage, ConversationView } from '../lib/types';
import { BodyView } from './body-view';
import { FilterPreview } from './filter-preview';

function MessageBlock({ msg, defaultOpen }: { msg: ConversationMessage; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const who = msg.isOwn ? '我' : (msg.fromAddr ?? '(未知)');
  const when = msg.sentAt ?? '';

  if (!open) {
    return (
      <button
        type="button"
        className="flex w-full gap-2 border-b border-slate-100 px-2 py-1 text-left text-sm dark:border-slate-800"
        onClick={() => {
          setOpen(true);
        }}
        aria-label={`展开 ${msg.fromAddr ?? '邮件'} ${when}`}
      >
        <span className="font-medium">{who}</span>
        <span className="truncate text-slate-500">
          {msg.snippet ?? msg.textPlain?.slice(0, 80) ?? ''}
        </span>
      </button>
    );
  }

  return (
    <div className="border-b border-slate-100 py-2 dark:border-slate-800">
      <div className="mb-1 flex justify-between text-xs text-slate-500">
        <span className="font-medium text-slate-700 dark:text-slate-300">{who}</span>
        <span>{when}</span>
      </div>
      <BodyView html={msg.html} textPlain={msg.textPlain} />
      <FilterPreview messageId={msg.id} />
    </div>
  );
}

export function ConversationThread({ view }: { view: ConversationView }) {
  const lastIndex = view.messages.length - 1;
  return (
    <div className="conversation-thread">
      {!view.sentSyncOk && (
        <div className="mb-2 rounded bg-amber-50 px-2 py-1 text-xs text-amber-700" role="status">
          已发件箱同步未完成，部分回复可能缺失
        </div>
      )}
      {view.messages.map((m, i) => (
        <MessageBlock key={m.id} msg={m} defaultOpen={i === lastIndex} />
      ))}
    </div>
  );
}
