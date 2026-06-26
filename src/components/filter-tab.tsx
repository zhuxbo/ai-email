import { useEffect, useState } from 'react';

import { useMailStore } from '../lib/store/mail';
import * as tauri from '../lib/tauri';
import type { MessageFilterPreview, FilterTarget } from '../lib/types';
import { errMsg } from '../lib/utils';
import { BodyView } from './body-view';

const TARGET_LABEL: Record<FilterTarget, string> = {
  signature: '签名',
  quote: '引用历史',
  repeat: '重复块',
};

export function FilterTab() {
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const [data, setData] = useState<MessageFilterPreview | null>(null);
  const [error, setError] = useState<string | null>(null);

  // 切换邮件即重新预览；active 守卫防迟到结果覆盖新选中。
  useEffect(() => {
    if (selectedMessageId === null) {
      setData(null);
      setError(null);
      return;
    }
    let active = true;
    const id = selectedMessageId;
    setData(null);
    setError(null);
    // 卸载守卫同 message-detail：迟到结果不覆盖新选中。
    void tauri
      .messageFilterPreview(id)
      .then((d) => {
        if (active) {
          setData(d);
        }
      })
      .catch((e: unknown) => {
        if (active) {
          setError(errMsg(e));
        }
      });
    return () => {
      active = false;
    };
  }, [selectedMessageId]);

  const onToggleDisabled = async (disabled: boolean): Promise<void> => {
    if (selectedMessageId === null) return;
    setError(null);
    try {
      await tauri.messageSetFilterDisabled(selectedMessageId, disabled);
      const d = await tauri.messageFilterPreview(selectedMessageId);
      setData(d);
    } catch (e) {
      setError(errMsg(e));
    }
  };

  if (selectedMessageId === null) {
    return <p className="text-sm text-text-3">在左侧选一封邮件再看过滤效果。</p>;
  }

  return (
    <div className="flex flex-col gap-2">
      <h4 className="text-sm font-semibold text-text-1">过滤核对</h4>
      <p className="text-xs text-text-3">
        AI 收到的是剥掉签名/引用/重复后的「净增量」。下面是按当前规则的预览。
      </p>

      {error !== null && <p className="text-xs text-red-600">{error}</p>}
      {data === null && error === null && <p className="text-xs text-text-3">加载中…</p>}

      {data !== null && (
        <>
          {data.disabled && (
            <p className="rounded bg-amber-50 px-2 py-1 text-xs text-amber-700">
              本封已禁用过滤，AI 收到完整原文。下方为「若启用规则将剥成」的预览。
            </p>
          )}

          <div className="text-xs font-medium text-text-3">按当前规则的净增量（预览）</div>
          <div className="rounded bg-emerald-50/40 p-2 dark:bg-emerald-900/10">
            {data.net ? (
              <BodyView html={null} textPlain={data.net} />
            ) : (
              <span className="text-xs text-text-3">（内容全部已剥）</span>
            )}
          </div>

          {data.removed.length > 0 && (
            <div className="flex flex-col gap-1">
              <div className="text-xs font-medium text-text-3">被剥块</div>
              {data.removed.map((b, i) => (
                <details key={i} className="rounded border border-[var(--color-border)]">
                  <summary className="cursor-pointer px-2 py-1 text-xs text-text-3">
                    {TARGET_LABEL[b.kind]} — {b.reason}
                  </summary>
                  <pre className="whitespace-pre-wrap break-words px-2 py-1 text-xs text-text-3">
                    {b.text}
                  </pre>
                </details>
              ))}
            </div>
          )}

          <label className="mt-1 flex items-center gap-1 text-xs text-text-2">
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
  );
}
