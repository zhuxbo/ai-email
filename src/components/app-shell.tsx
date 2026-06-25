import { useEffect, useRef, type ReactNode } from 'react';
import { CommandBar } from './command-bar';
import { NavRail } from './nav-rail';
import { Drawer } from './ui/drawer';
import { useBreakpoint } from '../lib/hooks/use-breakpoint';
import { useUiStore } from '../lib/store/ui';
import { useMailStore } from '../lib/store/mail';
import { useComposeStore } from '../lib/store/compose';
import type { Account, Mailbox } from '../lib/types';

interface NavProps {
  accounts: Account[];
  selectedAccountId: string | null;
  mailboxes: Mailbox[];
  selectedMailboxId: string | null;
  syncing: boolean;
  onSelectAccount: (id: string | null) => void;
  onSelectMailbox: (mailboxId: string) => void;
  onAddAccount: () => void;
  onSync: () => void;
  onOpenSettings: () => void;
  onOpenAutoReply: () => void;
  autoReplyCount: number;
}

interface Props {
  nav: NavProps;
  onQueryChange: (q: string) => void;
  messageOpenSeq: number;
  list: ReactNode;
  detail: ReactNode;
  drawer: ReactNode;
}

export function AppShell({ nav, onQueryChange, messageOpenSeq, list, detail, drawer }: Props) {
  const bp = useBreakpoint();
  const isMobile = bp === 'mobile';
  const drawerOpen = useUiStore((s) => s.drawerOpen);
  const closeDrawer = useUiStore((s) => s.closeDrawer);
  const openDrawer = useUiStore((s) => s.openDrawer);
  const mobileView = useUiStore((s) => s.mobileView);
  const setMobileView = useUiStore((s) => s.setMobileView);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);

  // 读最新 isMobile 但不放进 effect deps —— 只有"打开邮件"（seq 变）才切详情，缩窗不切。
  const isMobileRef = useRef(isMobile);
  isMobileRef.current = isMobile;

  // 移动端：每次打开邮件（messageOpenSeq 递增）进详情视图。
  useEffect(() => {
    if (messageOpenSeq > 0 && isMobileRef.current) {
      setMobileView('detail');
    }
  }, [messageOpenSeq, setMobileView]);

  // #15 移动端：选中邮件失效（删除/切筛选）时自动回列表，避免困在空详情视图。
  useEffect(() => {
    if (isMobile && selectedMessageId === null) {
      setMobileView('list');
    }
  }, [isMobile, selectedMessageId, setMobileView]);

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-app">
      <CommandBar
        onQueryChange={onQueryChange}
        onAiCommand={() => {
          // AI 指令按钮作为开关：已打开则关闭，否则带当前邮件上下文打开「写信」tab。
          if (drawerOpen) {
            closeDrawer();
            return;
          }
          const { selectedMessageId, messages } = useMailStore.getState();
          const msg = selectedMessageId
            ? messages.find((m) => m.id === selectedMessageId)
            : undefined;
          if (msg) useComposeStore.getState().openReply(msg);
          else useComposeStore.getState().openBlank();
          openDrawer('compose');
        }}
      />
      <div className="relative flex min-h-0 flex-1">
        {!isMobile && <NavRail {...nav} />}
        {isMobile ? (
          <main className="min-w-0 flex-1 overflow-auto">
            {mobileView === 'detail' ? (
              <div className="flex h-full flex-col">
                <button
                  type="button"
                  aria-label="返回列表"
                  onClick={() => {
                    setMobileView('list');
                  }}
                  className="border-b border-[var(--color-border)] bg-panel px-3 py-2 text-left text-xs text-accent"
                >
                  ← 返回
                </button>
                <div className="min-h-0 flex-1 overflow-auto">{detail}</div>
              </div>
            ) : (
              list
            )}
          </main>
        ) : (
          <>
            <div className="shrink-0 overflow-auto">{list}</div>
            <main className="min-w-0 flex-1 overflow-auto">{detail}</main>
          </>
        )}
        <Drawer open={drawerOpen} onClose={closeDrawer}>
          {drawer}
        </Drawer>
      </div>
    </div>
  );
}
