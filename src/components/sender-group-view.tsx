// Right pane: 同发件人组视图。点列表里 foldKind==='sender' 的折叠行时展示该发件人的邮件，
// 复用会话流的 MessageBlock（默认全部折叠，点击逐封展开）。与单封会话详情 (ConversationThread)
// 互斥，由 store.detailMode 判别选择渲染哪个。本组件不管附件（D2 给 MessageBlock 补逐封附件）。

import { useMailStore } from '../lib/store/mail';
import type { ConversationView } from '../lib/types';
import { colorForSeed } from './ui/avatar';
import { MessageBlock } from './conversation-thread';

export function SenderGroupView({ view }: { view: ConversationView }) {
  const accounts = useMailStore((s) => s.accounts);
  // 真实组大小（被点 FoldedItem.count）。后端至多物化 50 封，组更大时顶部提示截断。
  const senderGroupCount = useMailStore((s) => s.senderGroupCount);
  // 后端 messages 按时间升序；此处反转展示，最新在最上（同 ConversationThread）。
  const ordered = [...view.messages].reverse();
  const truncated = senderGroupCount > view.messages.length;

  return (
    <div className="sender-group-view">
      {truncated && (
        <div className="mb-2 rounded bg-slate-100 px-2 py-1 text-xs text-text-3" role="status">
          显示最近 {view.messages.length} 封，共 {senderGroupCount} 封
        </div>
      )}
      {ordered.map((m) => {
        const account = accounts.find((a) => a.id === m.accountId);
        const ownColor = m.isOwn ? colorForSeed(account?.email ?? m.accountId) : null;
        return <MessageBlock key={m.id} msg={m} defaultOpen={false} ownColor={ownColor} />;
      })}
    </div>
  );
}
