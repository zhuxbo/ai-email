import { IconButton } from './ui/icon-button';
import { useUiStore } from '../lib/store/ui';

interface Props {
  onQueryChange: (q: string) => void;
  onAiCommand: () => void;
}

export function CommandBar({ onQueryChange, onAiCommand }: Props) {
  const theme = useUiStore((s) => s.theme);
  const toggleTheme = useUiStore((s) => s.toggleTheme);

  return (
    <header className="flex items-center gap-2.5 border-b border-[var(--color-border)] bg-panel px-3.5 py-2">
      <span className="text-[13px] font-bold text-text-1">✉ 统一收件箱</span>
      <div className="flex max-w-md flex-1 items-center gap-2 rounded-[var(--radius-md)] border border-[var(--color-border)] bg-app px-2.5 py-1.5">
        <span className="text-text-3">🔍</span>
        <input
          type="text"
          aria-label="搜索邮件"
          placeholder="搜索全部账户的邮件、联系人、附件…"
          onChange={(e) => {
            onQueryChange(e.target.value);
          }}
          className="min-w-0 flex-1 bg-transparent text-xs text-text-1 outline-none placeholder:text-text-3"
        />
        <kbd className="rounded border border-[var(--color-border)] bg-panel px-1.5 text-[10px] text-text-2">
          ⌘K
        </kbd>
      </div>
      <button
        type="button"
        onClick={onAiCommand}
        className="ml-auto rounded-[var(--radius-md)] bg-ink px-2.5 py-1.5 text-xs font-medium text-white"
      >
        ✦ AI 指令
      </button>
      <IconButton
        label="切换主题"
        onClick={toggleTheme}
        className="h-7 w-7 text-text-2 hover:bg-black/5 dark:hover:bg-white/10"
      >
        {theme === 'light' ? '🌙' : '☀️'}
      </IconButton>
    </header>
  );
}
