import type { TranslateResult } from '../lib/types';

export function TranslationView({ translation }: { translation: TranslateResult }) {
  return (
    <div className="mt-3 space-y-2 rounded border border-slate-200 bg-slate-50 p-3 dark:border-slate-700 dark:bg-slate-950">
      <div className="text-[10px] font-medium uppercase tracking-wide text-slate-500 dark:text-slate-400">
        译文 · {translation.target}
      </div>
      <p className="text-sm font-medium text-slate-900 dark:text-slate-100">
        {translation.subject}
      </p>
      <pre className="max-h-[400px] overflow-auto whitespace-pre-wrap break-words font-sans text-sm text-slate-700 dark:text-slate-300">
        {translation.body}
      </pre>
      <footer className="text-[10px] text-slate-400">
        <UsageLine
          source={translation.source}
          model={translation.model}
          inputTokens={translation.inputTokens}
          outputTokens={translation.outputTokens}
          cacheReadTokens={translation.cacheReadTokens}
        />
      </footer>
    </div>
  );
}

interface UsageProps {
  source: 'fresh' | 'cached';
  model: string;
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
  extra?: string;
}

export function UsageLine(props: UsageProps) {
  if (props.source === 'cached') {
    return <span title="ai_results 命中，无 token 消耗">{props.model} · 缓存命中 · 0 token</span>;
  }
  const parts: string[] = [props.model];
  if (props.inputTokens !== null) {
    parts.push(`输入 ${String(props.inputTokens)}`);
  }
  if (props.outputTokens !== null) {
    parts.push(`输出 ${String(props.outputTokens)}`);
  }
  if (props.cacheReadTokens !== null && props.cacheReadTokens > 0) {
    parts.push(`prompt 缓存命中 ${String(props.cacheReadTokens)}`);
  }
  if (props.extra) {
    parts.push(props.extra);
  }
  return <span>{parts.join(' · ')}</span>;
}
