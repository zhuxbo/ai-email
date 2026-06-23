import { useEffect, useState } from 'react';

import { AppShell } from './components/app-shell';
import { AddAccountDialog } from './components/add-account-dialog';
import { AiDrawer } from './components/ai-drawer';
import { AiSettingsDialog } from './components/ai-settings-dialog';
import { AutoReplyDialog } from './components/auto-reply-dialog';
import { MessageDetail } from './components/message-detail';
import { MessageList } from './components/message-list';
import { useAiStore } from './lib/store/ai';
import { useAutoReplyStore } from './lib/store/auto-reply';
import { useComposeStore } from './lib/store/compose';
import { useMailStore } from './lib/store/mail';
import { applyTheme, useUiStore } from './lib/store/ui';
import { onAutoReplyUpdated, onMailClassified } from './lib/tauri';
import './App.css';

function App() {
  const loadAccounts = useMailStore((s) => s.loadAccounts);
  const accounts = useMailStore((s) => s.accounts);
  const selectedAccountId = useMailStore((s) => s.selectedAccountId);
  const messageOpenSeq = useMailStore((s) => s.messageOpenSeq);
  const syncing = useMailStore((s) => s.syncing);
  const setFilter = useMailStore((s) => s.setFilter);
  const setQuery = useMailStore((s) => s.setQuery);
  const syncInbox = useMailStore((s) => s.syncInbox);
  const removeAccount = useMailStore((s) => s.removeAccount);
  const error = useMailStore((s) => s.error);
  const clearError = useMailStore((s) => s.clearError);
  const aiError = useAiStore((s) => s.error);
  const composeError = useComposeStore((s) => s.error);
  const autoReplyError = useAutoReplyStore((s) => s.error);

  const [addOpen, setAddOpen] = useState(false);
  const [aiSettingsOpen, setAiSettingsOpen] = useState(false);
  const [autoReplyOpen, setAutoReplyOpen] = useState(false);

  // 把 store 初始主题（可能跟随系统）同步到 <html>。
  useEffect(() => {
    applyTheme(useUiStore.getState().theme);
  }, []);

  useEffect(() => {
    void loadAccounts();
    void useAiStore.getState().loadAiConfig();
    void useAutoReplyStore.getState().loadQueue();

    // 后台 classify 完成 → 刷新邮件列表（category/priority 已写回）。
    // 后台 evaluate_rules 完成 → 刷新建议回复队列。
    // 两个 listen 均返回 Promise<unlisten>，在 useEffect 清理时调用。
    let unlistenClassified: (() => void) | null = null;
    let unlistenAutoReply: (() => void) | null = null;

    void onMailClassified(() => {
      void useMailStore.getState().reloadMessages();
    }).then((fn) => {
      unlistenClassified = fn;
    });

    void onAutoReplyUpdated(() => {
      void useAutoReplyStore.getState().loadQueue();
    }).then((fn) => {
      unlistenAutoReply = fn;
    });

    return () => {
      unlistenClassified?.();
      unlistenAutoReply?.();
    };
  }, [loadAccounts]);

  // mail / ai / compose / autoReply 四路错误各自成条，互不掩盖；各自独立关闭，不连带清掉对方未读的错误。
  const errorToasts: { key: string; text: string; clear: () => void }[] = [];
  if (error !== null) errorToasts.push({ key: 'mail', text: error, clear: clearError });
  if (aiError !== null)
    errorToasts.push({
      key: 'ai',
      text: aiError,
      clear: () => {
        useAiStore.getState().clearError();
      },
    });
  if (composeError !== null)
    errorToasts.push({
      key: 'compose',
      text: composeError,
      clear: () => {
        useComposeStore.setState({ error: null });
      },
    });
  if (autoReplyError !== null)
    errorToasts.push({
      key: 'autoReply',
      text: autoReplyError,
      clear: () => {
        useAutoReplyStore.getState().clearError();
      },
    });

  return (
    <>
      <AppShell
        nav={{
          accounts,
          selectedAccountId,
          syncing,
          onSelectAccount: (id) => void setFilter(id),
          onAddAccount: () => {
            setAddOpen(true);
          },
          onSync: () => {
            void syncInbox();
            // 队列/列表刷新由后台任务完成后 emit 的事件驱动（autoreply://updated、
            // mail://classified），不再需要固定延迟计时器。
          },
          onRemoveAccount: (id) => {
            if (window.confirm('确认移除该账户？授权码会从 keychain 删除，本地邮件清空。')) {
              void removeAccount(id);
            }
          },
          onOpenSettings: () => {
            setAiSettingsOpen(true);
          },
          onOpenAutoReply: () => {
            setAutoReplyOpen(true);
          },
          autoReplyCount: useAutoReplyStore((s) => s.queue.length),
        }}
        onQueryChange={(q) => {
          setQuery(q);
        }}
        messageOpenSeq={messageOpenSeq}
        list={<MessageList />}
        detail={<MessageDetail />}
        drawer={<AiDrawer />}
      />

      <AddAccountDialog
        open={addOpen}
        onClose={() => {
          setAddOpen(false);
        }}
      />
      <AiSettingsDialog
        open={aiSettingsOpen}
        onClose={() => {
          setAiSettingsOpen(false);
        }}
      />
      <AutoReplyDialog
        open={autoReplyOpen}
        onClose={() => {
          setAutoReplyOpen(false);
        }}
      />
      {errorToasts.length > 0 && (
        <div className="fixed bottom-4 left-4 z-50 flex max-w-md flex-col gap-2">
          {errorToasts.map((t) => (
            <div
              key={t.key}
              role="alert"
              className="rounded bg-danger px-3 py-2 text-xs text-white shadow-lg"
            >
              <div className="flex items-start gap-2">
                <span className="flex-1 break-words">{t.text}</span>
                <button
                  type="button"
                  onClick={t.clear}
                  aria-label="关闭错误提示"
                  className="text-white/70 transition-colors hover:text-white"
                >
                  ×
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

export default App;
