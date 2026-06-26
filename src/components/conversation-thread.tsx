import { useState } from 'react';

import { useMailStore } from '../lib/store/mail';
import type { ConversationMessage, ConversationView } from '../lib/types';
import { formatDateTimeCN } from '../lib/utils';
import { colorForSeed } from './ui/avatar';
import { BodyView } from './body-view';

function MessageBlock({
  msg,
  defaultOpen,
  ownColor,
}: {
  msg: ConversationMessage;
  defaultOpen: boolean;
  ownColor: string | null;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const who = msg.fromAddr ?? '(未知)';
  const when = formatDateTimeCN(msg.sentAt);
  // 自己发的：账号图标色的极浅底（8 位 hex 末尾 14 ≈ 8% alpha）。
  const ownStyle = ownColor ? { backgroundColor: `${ownColor}14` } : undefined;

  if (!open) {
    return (
      <button
        type="button"
        style={ownStyle}
        className="mb-2 flex w-full items-center gap-2 rounded-lg border border-[var(--color-border)] bg-panel px-3 py-2 text-left text-sm"
        onClick={() => {
          setOpen(true);
        }}
        aria-label={`展开 ${who} ${when}`}
      >
        <span className="shrink-0 font-medium text-text-1">{who}</span>
        <span className="min-w-0 flex-1 truncate text-text-3">
          {msg.snippet ?? msg.textPlain?.slice(0, 80) ?? ''}
        </span>
        <span className="shrink-0 whitespace-nowrap text-xs text-text-3">{when}</span>
      </button>
    );
  }

  return (
    <div
      style={ownStyle}
      className="mb-3 rounded-lg border border-[var(--color-border)] bg-panel p-3 shadow-sm"
    >
      <button
        type="button"
        className="mb-2 flex w-full items-baseline justify-between gap-2 text-left"
        onClick={() => {
          setOpen(false);
        }}
        aria-label={`收起 ${who} ${when}`}
      >
        <span className="min-w-0 truncate text-sm font-medium text-text-1">{who}</span>
        <span className="shrink-0 whitespace-nowrap text-xs text-text-3">{when}</span>
      </button>
      <BodyView html={msg.html} textPlain={msg.textPlain} />
    </div>
  );
}

export function ConversationThread({ view }: { view: ConversationView }) {
  const accounts = useMailStore((s) => s.accounts);
  // 后端 members 按时间升序（剥引用依赖「前序=更早」，顺序不能动）；此处仅反转展示，最新在最上。
  const ordered = [...view.messages].reverse();
  return (
    <div className="conversation-thread">
      {!view.sentSyncOk && (
        <div className="mb-2 rounded bg-amber-50 px-2 py-1 text-xs text-amber-700" role="status">
          已发件箱同步未完成，部分回复可能缺失
        </div>
      )}
      {ordered.map((m, i) => {
        const account = accounts.find((a) => a.id === m.accountId);
        const ownColor = m.isOwn ? colorForSeed(account?.email ?? m.accountId) : null;
        return <MessageBlock key={m.id} msg={m} defaultOpen={i === 0} ownColor={ownColor} />;
      })}
    </div>
  );
}
