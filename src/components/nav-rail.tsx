import { useState } from 'react';
import { Avatar } from './ui/avatar';
import { IconButton } from './ui/icon-button';
import { decodeModifiedUtf7 } from '../lib/utils';
import type { Account, Mailbox, SpecialUse } from '../lib/types';

/** specialUse → 图标 + 中文标签 */
const SPECIAL_USE_META: Record<SpecialUse, { icon: string; label: string }> = {
  inbox: { icon: '📥', label: '收件箱' },
  sent: { icon: '📤', label: '已发送' },
  drafts: { icon: '✏️', label: '草稿' },
  trash: { icon: '🗑️', label: '废纸篓' },
  junk: { icon: '🚫', label: '垃圾邮件' },
};

/** 是否被识别为标准信箱（有有效 specialUse）。对 null / undefined 均返回 false。 */
function hasSpecialUse(box: Mailbox): boolean {
  return box.specialUse !== null && box.specialUse in SPECIAL_USE_META;
}

function mailboxLabel(box: Mailbox): string {
  if (box.specialUse !== null && box.specialUse in SPECIAL_USE_META) {
    return SPECIAL_USE_META[box.specialUse].label;
  }
  // 自定义文件夹：解码 modified UTF-7（中文名）后取叶子名、截断过长
  const decoded = decodeModifiedUtf7(box.name);
  const leaf = decoded.split('/').pop() ?? decoded;
  return leaf.length > 6 ? `${leaf.slice(0, 6)}…` : leaf;
}

function mailboxIcon(box: Mailbox): string {
  if (box.specialUse !== null && box.specialUse in SPECIAL_USE_META) {
    return SPECIAL_USE_META[box.specialUse].icon;
  }
  return '📁';
}

function MailboxButton({
  box,
  selected,
  onSelect,
}: {
  box: Mailbox;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <button
      type="button"
      aria-label={mailboxLabel(box)}
      aria-pressed={selected}
      title={decodeModifiedUtf7(box.name)}
      onClick={() => {
        onSelect(box.id);
      }}
      className={`flex h-8 w-8 flex-col items-center justify-center rounded text-[10px] leading-none transition-colors hover:bg-slate-700 ${
        selected ? 'bg-slate-600 text-white' : 'text-slate-400'
      }`}
    >
      <span className="text-sm leading-none">{mailboxIcon(box)}</span>
    </button>
  );
}

interface Props {
  accounts: Account[];
  selectedAccountId: string | null;
  mailboxes: Mailbox[];
  selectedMailboxId: string | null;
  syncing: boolean;
  onSelectAccount: (id: string | null) => void;
  onSelectMailbox: (mailboxId: string) => void;
  onAddAccount: () => void;
  onSync: () => void;
  onRemoveAccount: (id: string) => void;
  onOpenSettings: () => void;
  onOpenAutoReply: () => void;
  autoReplyCount: number;
}

export function NavRail({
  accounts,
  selectedAccountId,
  mailboxes,
  selectedMailboxId,
  syncing,
  onSelectAccount,
  onSelectMailbox,
  onAddAccount,
  onSync,
  onRemoveAccount,
  onOpenSettings,
  onOpenAutoReply,
  autoReplyCount,
}: Props) {
  const [foldersExpanded, setFoldersExpanded] = useState(false);
  // 标准信箱（收件箱/已发送/草稿/废纸篓/垃圾）始终直显；其余自定义文件夹折叠进"更多"，
  // 避免 QQ 等账户的一堆订阅/广告/标签文件夹挤满 54px 窄栏。
  const specialBoxes = mailboxes.filter(hasSpecialUse);
  const customBoxes = mailboxes.filter((b) => !hasSpecialUse(b));

  return (
    <nav className="flex w-[54px] shrink-0 flex-col items-center gap-3 bg-ink py-3">
      <button
        type="button"
        aria-label="全部账户"
        aria-pressed={selectedAccountId === null}
        title="全部账户（聚合收件箱）"
        onClick={() => {
          onSelectAccount(null);
        }}
        className={`flex h-[30px] w-[30px] items-center justify-center rounded-full bg-slate-700 text-xs font-medium text-slate-200 hover:bg-slate-600 ${
          selectedAccountId === null
            ? 'ring-2 ring-accent ring-offset-2 ring-offset-[var(--color-ink)]'
            : ''
        }`}
      >
        全
      </button>
      {accounts.map((a) => (
        <button
          key={a.id}
          type="button"
          aria-label={a.email}
          aria-pressed={a.id === selectedAccountId}
          title={`${a.email}（右键删除）`}
          onClick={() => {
            onSelectAccount(a.id);
          }}
          onContextMenu={(e) => {
            e.preventDefault();
            onRemoveAccount(a.id);
          }}
          className={`rounded-full ${a.id === selectedAccountId ? 'ring-2 ring-accent ring-offset-2 ring-offset-[var(--color-ink)]' : ''}`}
        >
          <Avatar seed={a.email} size={30} />
        </button>
      ))}
      <IconButton
        label="新增账户"
        onClick={onAddAccount}
        className="h-[30px] w-[30px] rounded-full bg-slate-700 text-slate-300 hover:bg-slate-600"
      >
        ＋
      </IconButton>

      {/* 信箱列表：仅在选中单个账户时显示。标准信箱直显，自定义文件夹折叠进"更多"。 */}
      {selectedAccountId !== null && mailboxes.length > 0 && (
        <>
          <div className="h-px w-6 bg-slate-700" />
          {specialBoxes.map((box) => (
            <MailboxButton
              key={box.id}
              box={box}
              selected={box.id === selectedMailboxId}
              onSelect={onSelectMailbox}
            />
          ))}
          {customBoxes.length > 0 && (
            <>
              <button
                type="button"
                aria-label={
                  foldersExpanded ? '收起文件夹' : `更多文件夹（${String(customBoxes.length)}）`
                }
                aria-expanded={foldersExpanded}
                title={foldersExpanded ? '收起文件夹' : '更多文件夹'}
                onClick={() => {
                  setFoldersExpanded((v) => !v);
                }}
                className="flex h-8 w-8 items-center justify-center rounded text-sm leading-none text-slate-400 transition-colors hover:bg-slate-700"
              >
                {foldersExpanded ? '▴' : '⋯'}
              </button>
              {foldersExpanded &&
                customBoxes.map((box) => (
                  <MailboxButton
                    key={box.id}
                    box={box}
                    selected={box.id === selectedMailboxId}
                    onSelect={onSelectMailbox}
                  />
                ))}
            </>
          )}
        </>
      )}

      <div className="h-px w-6 bg-slate-700" />
      <IconButton
        label="同步收件箱"
        onClick={onSync}
        disabled={syncing || accounts.length === 0}
        className="h-8 w-8 text-slate-200 hover:bg-slate-700"
      >
        {syncing ? '⟳' : '🔄'}
      </IconButton>
      <div className="relative">
        <IconButton
          label="自动回复中心"
          onClick={onOpenAutoReply}
          className="h-8 w-8 text-amber-400 hover:bg-slate-700"
        >
          ⚡
        </IconButton>
        {autoReplyCount > 0 && (
          <span className="pointer-events-none absolute -right-0.5 -top-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-danger px-1 text-[9px] font-bold text-white">
            {autoReplyCount > 99 ? '99+' : autoReplyCount}
          </span>
        )}
      </div>
      <IconButton
        label="设置"
        onClick={onOpenSettings}
        className="mt-auto h-8 w-8 text-slate-400 hover:bg-slate-700"
      >
        ⚙
      </IconButton>
    </nav>
  );
}
