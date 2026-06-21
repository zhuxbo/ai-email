import { useEffect, useState } from 'react';

import { AppShell } from './components/app-shell';
import { AddAccountDialog } from './components/add-account-dialog';
import { AiDrawer } from './components/ai-drawer';
import { AiSettingsDialog } from './components/ai-settings-dialog';
import { MessageDetail } from './components/message-detail';
import { MessageList } from './components/message-list';
import { ReplyComposer } from './components/reply-composer';
import { useAiStore } from './lib/store/ai';
import { useMailStore } from './lib/store/mail';
import { useUiStore, applyTheme } from './lib/store/ui';
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

  const [addOpen, setAddOpen] = useState(false);
  const [aiSettingsOpen, setAiSettingsOpen] = useState(false);

  // 把 store 初始主题（可能跟随系统）同步到 <html>。
  useEffect(() => {
    applyTheme(useUiStore.getState().theme);
  }, []);

  useEffect(() => {
    void loadAccounts();
    void useAiStore.getState().loadAiConfig();
  }, [loadAccounts]);

  // mail 与 ai 两路错误各自成条，互不掩盖；各自独立关闭，不连带清掉对方未读的错误。
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
          onSync: () => void syncInbox(),
          onRemoveAccount: (id) => {
            if (window.confirm('确认移除该账户？授权码会从 keychain 删除，本地邮件清空。')) {
              void removeAccount(id);
            }
          },
          onOpenSettings: () => {
            setAiSettingsOpen(true);
          },
          onOpenAutoReply: () => {
            /* Plan 3: auto-reply center */
          },
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
      <ReplyComposer />

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
