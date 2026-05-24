// Top-level shell. Loads accounts on mount, owns the dialog open-state, renders the
// 3-pane layout and an error toast for transient store errors.

import { useEffect, useState } from 'react';

import { AccountList } from './components/account-list';
import { AddAccountDialog } from './components/add-account-dialog';
import { AiSettingsDialog } from './components/ai-settings-dialog';
import { MessageDetail } from './components/message-detail';
import { MessageList } from './components/message-list';
import { useMailStore } from './lib/store/mail';
import './App.css';

function App() {
  const loadAccounts = useMailStore((s) => s.loadAccounts);
  const loadAiConfig = useMailStore((s) => s.loadAiConfig);
  const error = useMailStore((s) => s.error);
  const clearError = useMailStore((s) => s.clearError);
  const [addOpen, setAddOpen] = useState(false);
  const [aiSettingsOpen, setAiSettingsOpen] = useState(false);

  useEffect(() => {
    void loadAccounts();
    void loadAiConfig();
  }, [loadAccounts, loadAiConfig]);

  return (
    <div className="flex h-screen w-screen overflow-hidden">
      <AccountList
        onAddAccount={() => {
          setAddOpen(true);
        }}
        onOpenAiSettings={() => {
          setAiSettingsOpen(true);
        }}
      />
      <MessageList />
      <MessageDetail />

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

      {error && (
        <div
          role="alert"
          className="fixed bottom-4 left-4 z-40 max-w-md rounded bg-red-600 px-3 py-2 text-xs text-white shadow-lg"
        >
          <div className="flex items-start gap-2">
            <span className="flex-1 break-words">{error}</span>
            <button
              type="button"
              onClick={clearError}
              className="text-red-100 hover:text-white"
              aria-label="关闭错误提示"
            >
              ×
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
