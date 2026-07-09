import { useState } from 'react';

import * as tauri from '../lib/tauri';

export function MaintenancePanel() {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function clearCache() {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const report = await tauri.cacheClear();
      setMessage(
        `已清理 ${String(report.messageBodiesDeleted)} 条正文缓存、${String(
          report.aiResultsDeleted,
        )} 条 AI 缓存。`,
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : '清理本地缓存失败');
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-4">
      <section className="space-y-3">
        <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">本地缓存</h3>
        <button
          type="button"
          onClick={() => void clearCache()}
          disabled={busy}
          className="rounded border border-slate-300 px-3 py-2 text-sm text-slate-700 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-slate-700 dark:text-slate-100 dark:hover:bg-slate-800"
        >
          {busy ? '正在清理…' : '清理本地缓存'}
        </button>
        {message && <div className="text-sm text-emerald-700 dark:text-emerald-300">{message}</div>}
        {error && <div className="text-sm text-red-600 dark:text-red-300">{error}</div>}
      </section>
    </div>
  );
}
