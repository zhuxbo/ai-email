// 设置中心：左栏单一 ⚙ 入口打开此对话框，用 tab 区分各类设置。
// 目前五类：账户（增删改邮箱）、AI 模型（配置 + 角色指派）、黑白名单、AI 过滤规则、收信（自动收信间隔）。各 tab 内容由独立 Panel 提供。

import { useState } from 'react';

import { AccountsPanel } from './account-settings-dialog';
import { AiModelsPanel } from './ai-settings-dialog';
import { AutoSyncPanel } from './auto-sync-panel';
import { FilterRulesPanel } from './filter-rules-dialog';
import { MaintenancePanel } from './maintenance-panel';
import { SenderFiltersPanel } from './sender-filters-dialog';

interface Props {
  open: boolean;
  onClose: () => void;
}

const TABS = [
  { key: 'accounts', label: '账户' },
  { key: 'ai', label: 'AI 模型' },
  { key: 'filters', label: '黑白名单' },
  { key: 'ai-filters', label: 'AI 过滤规则' },
  { key: 'auto-sync', label: '收信' },
  { key: 'maintenance', label: '维护' },
] as const;
type TabKey = (typeof TABS)[number]['key'];

export function SettingsDialog({ open, onClose }: Props) {
  const [tab, setTab] = useState<TabKey>('accounts');

  if (!open) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4"
      onClick={onClose}
    >
      <div
        onClick={(e) => {
          e.stopPropagation();
        }}
        className="flex h-[600px] max-h-[90vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg bg-white shadow-xl dark:bg-slate-900"
      >
        <header className="flex items-center justify-between border-b border-slate-200 px-6 py-3 dark:border-slate-700">
          <h2 className="text-lg font-semibold text-slate-900 dark:text-slate-100">设置中心</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭"
            className="text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200"
          >
            ×
          </button>
        </header>

        <div className="flex shrink-0 gap-1 border-b border-slate-200 px-4 dark:border-slate-700">
          {TABS.map((t) => (
            <button
              key={t.key}
              type="button"
              onClick={() => {
                setTab(t.key);
              }}
              aria-pressed={tab === t.key}
              className={`px-4 py-2 text-sm font-medium transition-colors ${
                tab === t.key
                  ? 'border-b-2 border-blue-500 text-blue-600 dark:text-blue-400'
                  : 'text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200'
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>

        <div className="flex-1 overflow-auto px-6 py-4">
          {tab === 'accounts' && <AccountsPanel />}
          {tab === 'ai' && <AiModelsPanel />}
          {tab === 'filters' && <SenderFiltersPanel />}
          {tab === 'ai-filters' && <FilterRulesPanel />}
          {tab === 'auto-sync' && <AutoSyncPanel />}
          {tab === 'maintenance' && <MaintenancePanel />}
        </div>
      </div>
    </div>
  );
}
