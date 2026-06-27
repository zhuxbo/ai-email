import { useEffect, useRef } from 'react';

/** 左下角错误条：每条独立、互不掩盖，可手动关闭，也在 3 秒后自动消失。 */
export interface ErrorToastItem {
  key: string;
  text: string;
  clear: () => void;
}

const AUTO_DISMISS_MS = 3000;

function ErrorToast({ toast }: { toast: ErrorToastItem }) {
  // clear 闭包每次 render 标识可能变化；用 ref 持最新值，使计时 effect 只依赖 key+text，
  // 不因父组件重渲染而反复重置 3s 计时（否则计时永远走不满 3s）。
  const clearRef = useRef(toast.clear);
  clearRef.current = toast.clear;
  useEffect(() => {
    const id = setTimeout(() => {
      clearRef.current();
    }, AUTO_DISMISS_MS);
    return () => {
      clearTimeout(id);
    };
    // 同 key 文案变化（同一通道来了新错误）→ 重置 3s 计时，让新错误也完整展示。
  }, [toast.key, toast.text]);

  return (
    <div role="alert" className="rounded bg-danger px-3 py-2 text-xs text-white shadow-lg">
      <div className="flex items-start gap-2">
        <span className="flex-1 break-words">{toast.text}</span>
        <button
          type="button"
          onClick={toast.clear}
          aria-label="关闭错误提示"
          className="text-white/70 transition-colors hover:text-white"
        >
          ×
        </button>
      </div>
    </div>
  );
}

export function ErrorToasts({ toasts }: { toasts: ErrorToastItem[] }) {
  if (toasts.length === 0) return null;
  return (
    <div className="fixed bottom-4 left-4 z-50 flex max-w-md flex-col gap-2">
      {toasts.map((t) => (
        <ErrorToast key={t.key} toast={t} />
      ))}
    </div>
  );
}
