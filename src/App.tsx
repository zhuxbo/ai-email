import { useEffect, useState } from 'react';

import { AppShell } from './components/app-shell';
import { AddAccountDialog } from './components/add-account-dialog';
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
  const loadAiConfig = useMailStore((s) => s.loadAiConfig);
  const accounts = useMailStore((s) => s.accounts);
  const selectedAccountId = useMailStore((s) => s.selectedAccountId);
  const messageOpenSeq = useMailStore((s) => s.messageOpenSeq);
  const syncing = useMailStore((s) => s.syncing);
  const selectAccount = useMailStore((s) => s.selectAccount);
  const syncInbox = useMailStore((s) => s.syncInbox);
  const removeAccount = useMailStore((s) => s.removeAccount);
  const error = useMailStore((s) => s.error);
  const clearError = useMailStore((s) => s.clearError);

  const [addOpen, setAddOpen] = useState(false);
  const [aiSettingsOpen, setAiSettingsOpen] = useState(false);

  // 把 store 初始主题（可能跟随系统）同步到 <html>。
  useEffect(() => {
    applyTheme(useUiStore.getState().theme);
  }, []);

  useEffect(() => {
    void loadAccounts();
    void loadAiConfig();
    void useAiStore.getState().loadAiConfig();
  }, [loadAccounts, loadAiConfig]);

  return (
    <>
      <AppShell
        nav={{
          accounts,
          selectedAccountId,
          syncing,
          onSelectAccount: (id) => void selectAccount(id),
          onAddAccount: () => {
            setAddOpen(true);
          },
          onSync: () => {
            if (selectedAccountId !== null) void syncInbox(selectedAccountId);
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
            /* Plan 3: auto-reply center */
          },
        }}
        onQueryChange={() => {
          /* Plan 2: list filtering by query */
        }}
        messageOpenSeq={messageOpenSeq}
        list={<MessageList />}
        detail={<MessageDetail />}
        drawer={
          <div className="p-4 text-xs text-text-3">
            AI 指令面板将在后续版本上线；当前摘要/翻译请在右侧邮件详情中使用。
          </div>
        }
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

      {error && (
        <div
          role="alert"
          className="fixed bottom-4 left-4 z-50 max-w-md rounded bg-danger px-3 py-2 text-xs text-white shadow-lg"
        >
          <div className="flex items-start gap-2">
            <span className="flex-1 break-words">{error}</span>
            <button
              type="button"
              onClick={clearError}
              aria-label="关闭错误提示"
              className="text-white/70 transition-colors hover:text-white"
            >
              ×
            </button>
          </div>
        </div>
      )}
    </>
  );
}

export default App;
