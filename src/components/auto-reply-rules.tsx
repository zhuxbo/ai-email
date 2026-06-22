import { useEffect, useState } from 'react';

import { useAutoReplyStore } from '../lib/store/auto-reply';
import type { Category } from '../lib/types';

const CATEGORIES: { value: Category; label: string }[] = [
  { value: 'personal', label: '私人' },
  { value: 'work', label: '工作' },
  { value: 'notification', label: '通知' },
  { value: 'promotion', label: '推广' },
  { value: 'spam', label: '垃圾' },
];

// 重要度档位 → priority ceiling（1=最重要）。null=不限。
const PRIORITY_OPTS: { label: string; value: number | null }[] = [
  { label: '不限', value: null },
  { label: '仅紧急', value: 1 },
  { label: '紧急或重要', value: 2 },
  { label: '全部', value: 3 },
];

export function AutoReplyRules({ accountId }: { accountId: string }) {
  const rules = useAutoReplyStore((s) => s.rules);
  const loadRules = useAutoReplyStore((s) => s.loadRules);
  const addRule = useAutoReplyStore((s) => s.addRule);
  const removeRule = useAutoReplyStore((s) => s.removeRule);
  const toggleRule = useAutoReplyStore((s) => s.toggleRule);

  const [name, setName] = useState('');
  const [domain, setDomain] = useState('');
  const [category, setCategory] = useState<Category | ''>('');
  const [ceiling, setCeiling] = useState<number | null>(null);
  const [intent, setIntent] = useState('');

  useEffect(() => {
    void loadRules(accountId);
  }, [accountId, loadRules]);

  const noCondition = domain.trim() === '' && category === '' && ceiling === null;

  const submit = (): void => {
    if (name.trim() === '' || intent.trim() === '') return;
    void addRule({
      accountId,
      name: name.trim(),
      enabled: true,
      matchDomain: domain.trim() === '' ? null : domain.trim(),
      matchCategory: category === '' ? null : category,
      matchPriorityCeiling: ceiling,
      draftIntent: intent.trim(),
    });
    setName('');
    setDomain('');
    setCategory('');
    setCeiling(null);
    setIntent('');
  };

  return (
    <div className="flex flex-col gap-4">
      <ul className="flex flex-col divide-y divide-[var(--color-border)]">
        {rules.map((r) => (
          <li key={r.id} className="flex items-center justify-between gap-2 py-2">
            <div className="min-w-0">
              <div className="truncate text-sm font-medium text-text-1">{r.name}</div>
              <div className="truncate text-xs text-text-3">{r.draftIntent}</div>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <label className="flex items-center gap-1 text-xs text-text-2">
                <input
                  type="checkbox"
                  aria-label={`启用 ${r.name}`}
                  checked={r.enabled}
                  onChange={(e) => void toggleRule(r.id, e.target.checked)}
                />
                启用
              </label>
              <button
                type="button"
                onClick={() => void removeRule(r.id)}
                className="text-xs text-danger hover:underline"
              >
                删除
              </button>
            </div>
          </li>
        ))}
      </ul>

      <div className="flex flex-col gap-2 rounded border border-[var(--color-border)] p-3">
        <label className="flex flex-col gap-1 text-xs text-text-2">
          规则名称
          <input
            aria-label="规则名称"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-text-2">
          发件地址包含（可空）
          <input
            aria-label="发件地址包含"
            value={domain}
            onChange={(e) => {
              setDomain(e.target.value);
            }}
            placeholder="如 client.com"
            className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-text-2">
          类别（可空）
          <select
            aria-label="类别"
            value={category}
            onChange={(e) => {
              setCategory(e.target.value as Category | '');
            }}
            className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
          >
            <option value="">不限</option>
            {CATEGORIES.map((c) => (
              <option key={c.value} value={c.value}>
                {c.label}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs text-text-2">
          重要度
          <select
            aria-label="重要度"
            value={ceiling === null ? '' : String(ceiling)}
            onChange={(e) => {
              setCeiling(e.target.value === '' ? null : Number(e.target.value));
            }}
            className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
          >
            {PRIORITY_OPTS.map((p) => (
              <option key={p.label} value={p.value === null ? '' : String(p.value)}>
                {p.label}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs text-text-2">
          回复意图
          <textarea
            aria-label="回复意图"
            value={intent}
            onChange={(e) => {
              setIntent(e.target.value);
            }}
            rows={2}
            className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
          />
        </label>
        {noCondition && (
          <p className="text-xs text-amber-600">未设任何条件：将对全部新邮件生效。</p>
        )}
        <button
          type="button"
          onClick={submit}
          className="self-start rounded bg-accent px-3 py-1 text-xs font-medium text-white hover:opacity-90"
        >
          新增规则
        </button>
      </div>
    </div>
  );
}
