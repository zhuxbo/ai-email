import { useEffect, useState } from 'react';

import { useFilterRulesStore } from '../lib/store/filter-rules';
import type { FilterScope, FilterTarget, FilterAction } from '../lib/types';

const SCOPES: { value: FilterScope; label: string }[] = [
  { value: 'global', label: '全局' },
  { value: 'domain', label: '按域名' },
  { value: 'email', label: '按邮箱' },
];
const TARGETS: { value: FilterTarget; label: string }[] = [
  { value: 'signature', label: '签名' },
  { value: 'quote', label: '引用历史' },
  { value: 'repeat', label: '重复块' },
];
const ACTIONS: { value: FilterAction; label: string }[] = [
  { value: 'strip', label: '剥除' },
  { value: 'keep', label: '保留' },
];

export function FilterRulesPanel() {
  const rules = useFilterRulesStore((s) => s.rules);
  const error = useFilterRulesStore((s) => s.error);
  const loadRules = useFilterRulesStore((s) => s.loadRules);
  const addRule = useFilterRulesStore((s) => s.addRule);
  const removeRule = useFilterRulesStore((s) => s.removeRule);
  const toggleRule = useFilterRulesStore((s) => s.toggleRule);

  const [scope, setScope] = useState<FilterScope>('global');
  const [scopeValue, setScopeValue] = useState('');
  const [target, setTarget] = useState<FilterTarget>('signature');
  const [action, setAction] = useState<FilterAction>('strip');
  const [pattern, setPattern] = useState('');

  useEffect(() => {
    void loadRules();
  }, [loadRules]);

  // global signature strip 会波及 translate（翻译默认保留签名）。
  const affectsTranslate = scope === 'global' && target === 'signature' && action === 'strip';

  const submit = (): void => {
    if (scope !== 'global' && scopeValue.trim() === '') return;
    void addRule({
      scope,
      scopeValue: scope === 'global' ? '' : scopeValue.trim(),
      target,
      action,
      pattern: pattern.trim() === '' ? null : pattern.trim(),
      enabled: true,
      note: null,
    });
    setScopeValue('');
    setPattern('');
  };

  return (
    <div className="flex flex-col gap-4">
      <p className="text-xs text-slate-500">
        默认对所有邮件剥除签名、引用历史与重复块（翻译默认保留签名）。下面的规则是叠加的例外/定制。
      </p>

      <ul className="flex flex-col divide-y divide-slate-200 dark:divide-slate-700">
        {rules.map((r) => (
          <li key={r.id} className="flex items-center justify-between gap-2 py-2">
            <div className="min-w-0 text-sm">
              <span className="font-medium">
                {SCOPES.find((s) => s.value === r.scope)?.label}
                {r.scope !== 'global' ? ` ${r.scopeValue}` : ''}
              </span>
              <span className="text-slate-500">
                {' · '}
                {TARGETS.find((t) => t.value === r.target)?.label}
                {' → '}
                {ACTIONS.find((a) => a.value === r.action)?.label}
                {r.pattern ? ` (${r.pattern})` : ''}
              </span>
            </div>
            <div className="flex shrink-0 items-center gap-2 text-xs">
              <label className="flex items-center gap-1">
                <input
                  type="checkbox"
                  aria-label={`启用规则 ${r.id}`}
                  checked={r.enabled}
                  onChange={(e) => {
                    void toggleRule(r.id, e.target.checked);
                  }}
                />
                启用
              </label>
              <button
                type="button"
                onClick={() => {
                  void removeRule(r.id);
                }}
                className="text-red-600 hover:underline"
              >
                删除
              </button>
            </div>
          </li>
        ))}
      </ul>

      <div className="flex flex-col gap-2 rounded border border-slate-200 p-3 dark:border-slate-700">
        <label className="flex flex-col gap-1 text-xs">
          作用域
          <select
            aria-label="作用域"
            value={scope}
            onChange={(e) => {
              setScope(e.target.value as FilterScope);
            }}
            className="rounded border border-slate-200 px-2 py-1 text-sm"
          >
            {SCOPES.map((s) => (
              <option key={s.value} value={s.value}>
                {s.label}
              </option>
            ))}
          </select>
        </label>

        {scope !== 'global' && (
          <label className="flex flex-col gap-1 text-xs">
            作用域值
            <input
              aria-label="作用域值"
              value={scopeValue}
              onChange={(e) => {
                setScopeValue(e.target.value);
              }}
              placeholder={scope === 'domain' ? '如 cnssl.cn' : '如 boss@cnssl.cn'}
              className="rounded border border-slate-200 px-2 py-1 text-sm"
            />
          </label>
        )}

        <label className="flex flex-col gap-1 text-xs">
          目标
          <select
            aria-label="目标"
            value={target}
            onChange={(e) => {
              setTarget(e.target.value as FilterTarget);
            }}
            className="rounded border border-slate-200 px-2 py-1 text-sm"
          >
            {TARGETS.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </select>
        </label>

        <label className="flex flex-col gap-1 text-xs">
          动作
          <select
            aria-label="动作"
            value={action}
            onChange={(e) => {
              setAction(e.target.value as FilterAction);
            }}
            className="rounded border border-slate-200 px-2 py-1 text-sm"
          >
            {ACTIONS.map((a) => (
              <option key={a.value} value={a.value}>
                {a.label}
              </option>
            ))}
          </select>
        </label>

        <label className="flex flex-col gap-1 text-xs">
          正则（可空，仅签名 target 生效）
          <input
            aria-label="正则"
            value={pattern}
            onChange={(e) => {
              setPattern(e.target.value);
            }}
            placeholder="如 免责声明|Disclaimer"
            className="rounded border border-slate-200 px-2 py-1 text-sm"
          />
        </label>

        {affectsTranslate && (
          <p className="text-xs text-amber-600">
            该全局签名剥除规则将同时影响翻译（翻译默认保留签名）。
          </p>
        )}
        {error !== null && <p className="text-xs text-red-600">{error}</p>}

        <button
          type="button"
          onClick={submit}
          className="self-start rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:opacity-90"
        >
          新增规则
        </button>
      </div>
    </div>
  );
}
