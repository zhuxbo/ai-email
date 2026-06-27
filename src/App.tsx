import { useEffect, useState } from 'react';

import { AppShell } from './components/app-shell';
import { AddAccountDialog } from './components/add-account-dialog';
import { AiDrawer } from './components/ai-drawer';
import { SettingsDialog } from './components/settings-dialog';
import { AutoReplyDialog } from './components/auto-reply-dialog';
import { ErrorToasts, type ErrorToastItem } from './components/error-toasts';
import { MessageDetail } from './components/message-detail';
import { MessageList } from './components/message-list';
import { useAiStore } from './lib/store/ai';
import { useAutoReplyStore } from './lib/store/auto-reply';
import { useComposeStore } from './lib/store/compose';
import { useDbStore } from './lib/store/db';
import { useMailStore } from './lib/store/mail';
import { applyTheme, useUiStore } from './lib/store/ui';
import {
  getDbStatus,
  onAutoReplyUpdated,
  onDbError,
  onDbReady,
  onMailClassified,
} from './lib/tauri';
import './App.css';

function App() {
  const dbStatus = useDbStore((s) => s.status);
  const dbErrorMessage = useDbStore((s) => s.errorMessage);

  const loadAccounts = useMailStore((s) => s.loadAccounts);
  const accounts = useMailStore((s) => s.accounts);
  const selectedAccountId = useMailStore((s) => s.selectedAccountId);
  const mailboxes = useMailStore((s) => s.mailboxes);
  const selectedMailboxId = useMailStore((s) => s.selectedMailboxId);
  const selectMailbox = useMailStore((s) => s.selectMailbox);
  const messageOpenSeq = useMailStore((s) => s.messageOpenSeq);
  const syncing = useMailStore((s) => s.syncing);
  const setFilter = useMailStore((s) => s.setFilter);
  const setQuery = useMailStore((s) => s.setQuery);
  const syncInbox = useMailStore((s) => s.syncInbox);
  const error = useMailStore((s) => s.error);
  const clearError = useMailStore((s) => s.clearError);
  const aiError = useAiStore((s) => s.error);
  const composeError = useComposeStore((s) => s.error);
  const autoReplyError = useAutoReplyStore((s) => s.error);
  const autoReplyCount = useAutoReplyStore((s) => s.queue.length);

  const [addOpen, setAddOpen] = useState(false);
  const [autoReplyOpen, setAutoReplyOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  // 把 store 初始主题（可能跟随系统）同步到 <html>。
  useEffect(() => {
    applyTheme(useUiStore.getState().theme);
  }, []);

  // DB 就绪/失败事件订阅——在最外层 effect 里注册，与主逻辑解耦。
  // mounted 守卫防止 StrictMode 双 mount 下孤儿监听器泄漏。
  //
  // 竞态兜底：Tauri emit 无重放，若 db::connect 在 listener 注册前完成则事件丢失。
  // 解决方案：先 await 两个 listener 注册完成，再发一次 getDbStatus() 主动查询——
  // 若查询返回 ready/error 则经状态机（只前进）直接进入终态；若返回 initializing，
  // 则 listener 此刻必已就位、后续 emit 不会再丢。先注册后查询消除了"查询太早 + emit 丢失"
  // 的残留窗口；事件与查询哪条先到都正确。
  useEffect(() => {
    // 守卫用对象属性而非裸 let：对象属性跨 await 会被 CFA widen 回 boolean，
    // 既保留卸载守卫语义，又不会被 no-unnecessary-condition 误判为恒 falsy。
    const guard = { mounted: true };
    let unlistenReady: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;

    void (async () => {
      const [readyFn, errorFn] = await Promise.all([
        onDbReady(() => {
          useDbStore.getState().setReady();
        }),
        onDbError(({ message }) => {
          useDbStore.getState().setError(message);
        }),
      ]);

      // 注册期间组件已卸载：立即清理，避免孤儿监听器。
      if (!guard.mounted) {
        readyFn();
        errorFn();
        return;
      }
      unlistenReady = readyFn;
      unlistenError = errorFn;

      // listener 已就位，再兜底查询一次当前状态（emit 可能在注册前已发生而丢失）。
      // 此处无需 mounted 守卫：setReady/setError 操作的是全局 zustand store（非 React
      // setState），卸载后调用安全且幂等（状态机只前进）；listener 清理已由上面的 guard
      // 分支与 cleanup 覆盖。
      const payload = await getDbStatus();
      if (payload.status === 'ready') {
        useDbStore.getState().setReady();
      } else if (payload.status === 'error') {
        useDbStore.getState().setError(payload.message ?? '未知错误');
      }
      // initializing：listener 已就位，等 emit 即可，不会丢。
    })();

    return () => {
      guard.mounted = false;
      unlistenReady?.();
      unlistenError?.();
    };
  }, []);

  useEffect(() => {
    // 仅在 DB 就绪后初始化依赖数据库的 store。
    if (dbStatus !== 'ready') return;

    void loadAccounts();
    void useAiStore.getState().loadAiConfig();
    void useAutoReplyStore.getState().loadQueue();

    // 后台 classify 完成 → 刷新邮件列表（category/priority 已写回）。
    // 后台 evaluate_rules 完成 → 刷新建议回复队列。
    // mounted 守卫：若 Promise resolve 时组件已卸载，立即调用返回的 unlisten，
    // 避免孤儿监听器泄漏（在 StrictMode dev 双 mount 场景尤为必要）。
    let mounted = true;
    let unlistenClassified: (() => void) | null = null;
    let unlistenAutoReply: (() => void) | null = null;

    void onMailClassified(() => {
      const store = useMailStore.getState();
      if (store.classifiedAffectsCurrentView()) {
        void store.reloadMessages();
      }
    }).then((fn) => {
      if (!mounted) fn();
      else unlistenClassified = fn;
    });

    void onAutoReplyUpdated(() => {
      void useAutoReplyStore.getState().loadQueue();
    }).then((fn) => {
      if (!mounted) fn();
      else unlistenAutoReply = fn;
    });

    return () => {
      mounted = false;
      unlistenClassified?.();
      unlistenAutoReply?.();
    };
  }, [dbStatus, loadAccounts]);

  // mail / ai / compose / autoReply 四路错误各自成条，互不掩盖；各自独立关闭，不连带清掉对方未读的错误。
  const errorToasts: ErrorToastItem[] = [];
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

  if (dbStatus === 'loading') {
    return (
      <div className="flex h-screen items-center justify-center bg-surface text-content-muted text-sm">
        数据库初始化中…
      </div>
    );
  }

  if (dbStatus === 'error') {
    return (
      <div className="flex h-screen items-center justify-center bg-surface">
        <div
          role="alert"
          className="max-w-sm rounded bg-danger px-6 py-5 text-sm text-white shadow-lg"
        >
          <p className="font-semibold mb-1">数据库启动失败</p>
          <p className="break-words text-white/90">{dbErrorMessage ?? '未知错误'}</p>
        </div>
      </div>
    );
  }

  return (
    <>
      <AppShell
        nav={{
          accounts,
          selectedAccountId,
          mailboxes,
          selectedMailboxId,
          syncing,
          onSelectAccount: (id) => void setFilter(id),
          onSelectMailbox: (mailboxId) => void selectMailbox(mailboxId),
          onAddAccount: () => {
            setAddOpen(true);
          },
          onSync: () => {
            void syncInbox();
            // 队列/列表刷新由后台任务完成后 emit 的事件驱动（autoreply://updated、
            // mail://classified），不再需要固定延迟计时器。
          },
          onOpenSettings: () => {
            setSettingsOpen(true);
          },
          onOpenAutoReply: () => {
            setAutoReplyOpen(true);
          },
          autoReplyCount,
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
      <SettingsDialog
        open={settingsOpen}
        onClose={() => {
          setSettingsOpen(false);
        }}
      />
      <AutoReplyDialog
        open={autoReplyOpen}
        onClose={() => {
          setAutoReplyOpen(false);
        }}
      />
      <ErrorToasts toasts={errorToasts} />
    </>
  );
}

export default App;
