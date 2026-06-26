import { useState } from 'react';

import * as tauri from '../lib/tauri';
import type { MessageFilterPreview, FilterTarget } from '../lib/types';
import { errMsg } from '../lib/utils';
import { BodyView } from './body-view';

const TARGET_LABEL: Record<FilterTarget, string> = {
  signature: '签名',
  quote: '引用历史',
  repeat: '重复块',
};

export function FilterPreview({ messageId }: { messageId: string }) {
  const [open, setOpen] = useState(false);
  const [data, setData] = useState<MessageFilterPreview | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = async (): Promise<void> => {
    setError(null);
    try {
      const d = await tauri.messageFilterPreview(messageId);
      setData(d);
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const toggleOpen = (): void => {
    const next = !open;
    setOpen(next);
    if (next && data === null) void load();
  };

  const onToggleDisabled = async (disabled: boolean): Promise<void> => {
    setError(null);
    try {
      await tauri.messageSetFilterDisabled(messageId, disabled);
      await load();
    } catch (e) {
      setError(errMsg(e));
    }
  };

  return (
    <div className="mt-2 rounded border border-slate-200 dark:border-slate-700">
      <button
        type="button"
        onClick={toggleOpen}
        aria-expanded={open}
        className="w-full px-3 py-1.5 text-left text-xs font-medium text-slate-600 hover:bg-slate-50 dark:text-slate-300 dark:hover:bg-slate-800"
      >
        🔍 按当前规则会剥成…
      </button>

      {open && (
        <div className="flex flex-col gap-2 border-t border-slate-200 p-3 dark:border-slate-700">
          {error !== null && <p className="text-xs text-red-600">{error}</p>}

          {data === null && error === null && <p className="text-xs text-slate-400">加载中…</p>}

          {data !== null && (
            <>
              {data.disabled && (
                <p className="rounded bg-amber-50 px-2 py-1 text-xs text-amber-700">
                  本封已禁用过滤，AI 收到完整原文。下方为「若启用规则将剥成」的预览。
                </p>
              )}

              <div className="text-xs font-medium text-slate-500">按当前规则的净增量（预览）</div>
              <div className="rounded bg-emerald-50/40 p-2 dark:bg-emerald-900/10">
                {data.net ? (
                  <BodyView html={null} textPlain={data.net} />
                ) : (
                  <span className="text-xs text-slate-400">（内容全部已剥）</span>
                )}
              </div>

              {data.removed.length > 0 && (
                <div className="flex flex-col gap-1">
                  <div className="text-xs font-medium text-slate-500">被剥块</div>
                  {data.removed.map((b, i) => (
                    <details
                      key={i}
                      className="rounded border border-slate-200 dark:border-slate-700"
                    >
                      <summary className="cursor-pointer px-2 py-1 text-xs text-slate-500">
                        {TARGET_LABEL[b.kind]} — {b.reason}
                      </summary>
                      <pre className="whitespace-pre-wrap break-words px-2 py-1 text-xs text-slate-400">
                        {b.text}
                      </pre>
                    </details>
                  ))}
                </div>
              )}

              <label className="mt-1 flex items-center gap-1 text-xs text-slate-600 dark:text-slate-300">
                <input
                  type="checkbox"
                  checked={data.disabled}
                  onChange={(e) => {
                    void onToggleDisabled(e.target.checked);
                  }}
                />
                本封不过滤（AI 收完整原文）
              </label>
            </>
          )}
        </div>
      )}
    </div>
  );
}
