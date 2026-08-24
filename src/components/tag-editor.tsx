// 详情区标签编辑器：展示当前邮件全部标签（AI + 用户），可删除任意标签、输入添加用户标签。
// 乐观更新走 store 的 addTagLocal/removeTagLocal（失败精准回滚该条 tags）。

import { useState, type SyntheticEvent } from 'react';

import { useMailStore } from '../lib/store/mail';
import type { MessageHeader } from '../lib/types';

/** 客户端与后端 normalize_tag 同口径的上限（字符数）。 */
const MAX_TAG_CHARS = 30;

export function TagEditor({ message }: { message: MessageHeader | undefined }) {
  const addTagLocal = useMailStore((s) => s.addTagLocal);
  const removeTagLocal = useMailStore((s) => s.removeTagLocal);
  const [input, setInput] = useState('');
  const [hint, setHint] = useState<string | null>(null);

  if (message === undefined) return null;

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const tag = input.trim();
    if (tag === '') return;
    // 按码点计数（与后端 chars().count() 同口径，中文友好；emoji ZWJ 序列可能被拆开计数，属可接受取舍）。
    // eslint-disable-next-line @typescript-eslint/no-misused-spread -- 标签为短文本，码点计数足矣
    if ([...tag].length > MAX_TAG_CHARS) {
      setHint(`标签过长（最多 ${String(MAX_TAG_CHARS)} 字符）`);
      return;
    }
    if (message.tags.includes(tag)) {
      setHint('该标签已存在');
      return;
    }
    setHint(null);
    setInput('');
    void addTagLocal(message.id, tag);
  };

  return (
    <div className="mt-2 flex flex-wrap items-center gap-1.5">
      <span className="text-[10px] text-text-3">标签</span>
      {message.tags.map((t) => (
        <span
          key={t}
          className="group inline-flex items-center gap-1 rounded bg-slate-200 px-1.5 py-0.5 text-[10px] text-slate-700 dark:bg-slate-700 dark:text-slate-200"
        >
          {t}
          <button
            type="button"
            aria-label={`删除标签 ${t}`}
            title={`删除标签 ${t}`}
            onClick={() => {
              void removeTagLocal(message.id, t);
            }}
            className="rounded-full px-0.5 leading-none text-slate-500 hover:bg-slate-300 hover:text-slate-900 dark:text-slate-400 dark:hover:bg-slate-600 dark:hover:text-white"
          >
            ×
          </button>
        </span>
      ))}
      <form onSubmit={submit} className="inline-flex items-center gap-1">
        <input
          type="text"
          value={input}
          onChange={(e) => {
            setInput(e.target.value);
            setHint(null);
          }}
          placeholder="加标签…"
          aria-label="添加标签"
          className="w-24 rounded border border-[var(--color-border)] bg-app px-1.5 py-0.5 text-[10px] text-text-1 outline-none placeholder:text-text-3 focus:border-accent"
        />
        <button
          type="submit"
          disabled={input.trim() === ''}
          className="rounded px-1.5 py-0.5 text-[10px] font-medium text-text-2 hover:bg-black/5 disabled:cursor-not-allowed disabled:opacity-40 dark:hover:bg-white/10"
        >
          添加
        </button>
      </form>
      {hint !== null && (
        <span role="alert" className="text-[10px] text-danger">
          {hint}
        </span>
      )}
    </div>
  );
}
