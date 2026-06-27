import { useEffect } from 'react';
import { useMailStore } from '../store/mail';

/** 自动收信：窗口开启时按 autoSyncIntervalMin 周期同步全部账户。在 App 顶层调用一次。 */
export function useAutoSync(): void {
  const intervalMin = useMailStore((s) => s.autoSyncIntervalMin);
  useEffect(() => {
    if (intervalMin <= 0) return;
    const id = setInterval(() => {
      const s = useMailStore.getState();
      if (s.syncing || s.accounts.length === 0) return; // 跳过：进行中 / 无账户
      void s.syncAllInbox();
    }, intervalMin * 60_000);
    return () => {
      clearInterval(id);
    };
  }, [intervalMin]);
}
