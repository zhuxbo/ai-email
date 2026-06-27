import { useState } from 'react';
import type { Category } from '../lib/types';
import { CATEGORY_OPTIONS } from './message-row';

interface CategoryDialogProps {
  open: boolean;
  messageId: string;
  current: Category | null;
  onClose: () => void;
  onConfirm: (messageId: string, category: Category) => void;
}

export function CategoryDialog({
  open,
  messageId,
  current,
  onClose,
  onConfirm,
}: CategoryDialogProps) {
  const [chosen, setChosen] = useState<Category>(current ?? 'personal');

  if (!open) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="修改分类"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="w-72 rounded-lg border border-[var(--color-border)] bg-panel p-4 shadow-xl">
        <h2 className="mb-3 text-sm font-semibold text-text-1">修改分类</h2>
        <div className="flex flex-col gap-2">
          {CATEGORY_OPTIONS.map((opt) => (
            <label key={opt.value} className="flex cursor-pointer items-center gap-2">
              <input
                type="radio"
                name="category"
                value={opt.value}
                checked={chosen === opt.value}
                onChange={() => {
                  setChosen(opt.value);
                }}
                className="accent-accent"
              />
              <span className={`rounded px-1.5 py-0.5 text-xs font-medium ${opt.cls}`}>
                {opt.label}
              </span>
            </label>
          ))}
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded border border-[var(--color-border)] bg-panel px-3 py-1 text-xs text-text-1 hover:opacity-80"
          >
            取消
          </button>
          <button
            type="button"
            onClick={() => {
              onConfirm(messageId, chosen);
              onClose();
            }}
            className="rounded bg-accent px-3 py-1 text-xs font-medium text-white hover:opacity-90"
          >
            确认
          </button>
        </div>
      </div>
    </div>
  );
}
