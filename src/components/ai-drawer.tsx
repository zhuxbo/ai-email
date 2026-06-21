import { useUiStore } from '../lib/store/ui';
import { SummaryTab } from './summary-tab';
import { TranslateTab } from './translate-tab';
import { ComposeTab } from './compose-tab';

const TABS = [
  { key: 'summary', label: '摘要' },
  { key: 'translate', label: '翻译' },
  { key: 'compose', label: '写信' },
] as const;

export function AiDrawer() {
  const drawerTab = useUiStore((s) => s.drawerTab);
  const openDrawer = useUiStore((s) => s.openDrawer);
  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 border-b border-[var(--color-border)]">
        {TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            onClick={() => {
              openDrawer(t.key);
            }}
            aria-pressed={drawerTab === t.key}
            className={`flex-1 px-3 py-2 text-xs font-medium ${
              drawerTab === t.key ? 'border-b-2 border-accent text-accent' : 'text-text-3'
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-3">
        {drawerTab === 'summary' && <SummaryTab />}
        {drawerTab === 'translate' && <TranslateTab />}
        {drawerTab === 'compose' && <ComposeTab />}
      </div>
    </div>
  );
}
