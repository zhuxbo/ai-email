// 设置中心「收信」tab：配置自动收信间隔（关/1/5/15/30 分钟）+ 展示上次同步与失败账户。
// 间隔写入 mail store（持久化到 localStorage）；定时轮询由 useAutoSync hook 驱动。

import { useMailStore } from '../lib/store/mail';

const OPTIONS: { value: number; label: string }[] = [
  { value: 0, label: '关闭' },
  { value: 1, label: '每 1 分钟' },
  { value: 5, label: '每 5 分钟' },
  { value: 15, label: '每 15 分钟' },
  { value: 30, label: '每 30 分钟' },
];

export function AutoSyncPanel() {
  const intervalMin = useMailStore((s) => s.autoSyncIntervalMin);
  const setAutoSyncInterval = useMailStore((s) => s.setAutoSyncInterval);
  const lastSyncAt = useMailStore((s) => s.lastSyncAt);
  const accountErrors = useMailStore((s) => s.accountErrors);
  const accounts = useMailStore((s) => s.accounts);

  const failed = accounts.filter((a) => accountErrors[a.id]);

  return (
    <div className="space-y-4 text-sm">
      <div className="flex items-center gap-3">
        <label htmlFor="auto-sync-interval" className="text-slate-700 dark:text-slate-300">
          自动收信
        </label>
        <select
          id="auto-sync-interval"
          value={intervalMin}
          onChange={(e) => {
            setAutoSyncInterval(Number(e.target.value));
          }}
          className="rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
        >
          {OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </div>
      <p className="text-xs text-slate-500 dark:text-slate-400">
        窗口开启时每隔所选时间自动收取全部账户收件箱。关闭后仅手动同步。
      </p>
      <div className="text-xs text-slate-500 dark:text-slate-400">
        上次同步：
        {lastSyncAt === null
          ? '尚未同步'
          : new Date(lastSyncAt).toLocaleString('zh-CN', { hour12: false })}
      </div>
      {failed.length > 0 && (
        <ul className="text-xs text-amber-600 dark:text-amber-400">
          {failed.map((a) => (
            <li key={a.id}>
              {a.email}：{accountErrors[a.id] ?? ''}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
