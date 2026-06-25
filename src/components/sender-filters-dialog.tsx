import { useEffect, useState } from 'react';

import { useSenderFilters } from '../lib/store/sender-filters';
import type { SenderFilter } from '../lib/types';

const TYPE_LABEL: Record<SenderFilter['matchType'], string> = {
  address: '地址',
  domain: '域名',
  domain_glob: '通配',
};

function ListSection({ listType }: { listType: 'black' | 'white' }) {
  const allFilters = useSenderFilters((s) => s.filters);
  const filters = allFilters.filter((f) => f.listType === listType);
  const add = useSenderFilters((s) => s.add);
  const remove = useSenderFilters((s) => s.remove);
  const [value, setValue] = useState('');
  const label = listType === 'black' ? '黑名单' : '白名单';

  const submit = async () => {
    const v = value.trim();
    if (!v) return;
    const before = useSenderFilters.getState().filters.length;
    await add(listType, v);
    // 成功（条目数增加）才清空；失败保留输入供修正
    if (useSenderFilters.getState().filters.length > before) setValue('');
  };

  return (
    <section className="mb-6">
      <h3 className="mb-2 text-sm font-semibold text-slate-700 dark:text-slate-200">{label}</h3>
      <div className="mb-2 flex gap-2">
        <input
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
          }}
          placeholder={`${label}：a@x.com / @x.com / *.x.com（域名须 ASCII）`}
          className="flex-1 rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
        />
        <button
          type="button"
          onClick={() => {
            void submit();
          }}
          className="rounded bg-blue-500 px-3 py-1 text-sm text-white hover:bg-blue-600"
        >
          加入{label}
        </button>
      </div>
      {filters.length === 0 ? (
        <p className="text-sm text-slate-400">暂无{label}条目</p>
      ) : (
        <ul className="space-y-1">
          {filters.map((f) => (
            <li
              key={f.id}
              className="flex items-center justify-between rounded bg-slate-50 px-2 py-1 text-sm dark:bg-slate-800"
            >
              <span>
                <span className="font-mono">{f.pattern}</span>
                <span className="ml-2 text-xs text-slate-400">{TYPE_LABEL[f.matchType]}</span>
                {f.note ? <span className="ml-2 text-xs text-slate-400">{f.note}</span> : null}
              </span>
              <button
                type="button"
                onClick={() => {
                  void remove(f.id);
                }}
                className="text-slate-400 hover:text-red-500"
                aria-label="删除"
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function SenderFiltersPanel() {
  const error = useSenderFilters((s) => s.error);
  const load = useSenderFilters((s) => s.load);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div>
      {error ? (
        <p className="mb-3 rounded bg-red-50 px-2 py-1 text-sm text-red-600 dark:bg-red-950/40">
          {error}
        </p>
      ) : null}
      <ListSection listType="black" />
      <ListSection listType="white" />
    </div>
  );
}
