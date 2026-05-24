// AI assist panel embedded in the message detail view.
//
// Sprint 2 surface: summarize only. Sprint 3 will add classify chips, Sprint 4 a translate
// toggle, Sprint 5 a draft-reply trigger. Each operation hangs off useMailStore so the
// panel stays a presentation-only component.

import { useMailStore } from '../lib/store/mail';
import type { SummaryResult } from '../lib/types';

export function AiPanel() {
  const body = useMailStore((s) => s.body);
  const summary = useMailStore((s) => s.summary);
  const summarizing = useMailStore((s) => s.summarizing);
  const summarize = useMailStore((s) => s.summarizeSelectedMessage);

  // Body is loaded lazily on message click; without it the backend bails with a clear
  // error. Disable the button until then so the user doesn't get a useless toast.
  const ready = body !== null;

  return (
    <section className="mt-4 rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-700 dark:bg-slate-900">
      <header className="flex items-center justify-between">
        <h4 className="text-sm font-semibold text-slate-800 dark:text-slate-100">AI 助手</h4>
        <button
          type="button"
          disabled={!ready || summarizing}
          onClick={() => {
            void summarize();
          }}
          className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {summarizing ? '生成中…' : summary ? '重新总结' : '总结'}
        </button>
      </header>

      {!summary && !summarizing && (
        <p className="mt-3 text-xs text-slate-500 dark:text-slate-400">
          {ready ? '点击「总结」让 Claude 提炼这封邮件的要点。' : '等待正文加载…'}
        </p>
      )}

      {summarizing && (
        <p className="mt-3 animate-pulse text-xs text-slate-500 dark:text-slate-400">
          模型思考中（首次约 1–3 秒）…
        </p>
      )}

      {summary && <SummaryView summary={summary} />}
    </section>
  );
}

function SummaryView({ summary }: { summary: SummaryResult }) {
  return (
    <div className="mt-3 space-y-3">
      <p className="text-sm font-medium text-slate-900 dark:text-slate-100">{summary.tldr}</p>
      {summary.bullets.length > 0 && (
        <ul className="list-disc space-y-1 pl-5 text-sm text-slate-700 dark:text-slate-300">
          {summary.bullets.map((b, i) => (
            <li key={`${String(i)}-${b.slice(0, 8)}`}>{b}</li>
          ))}
        </ul>
      )}
      <footer className="text-[10px] text-slate-400">
        <UsageLine summary={summary} />
      </footer>
    </div>
  );
}

function UsageLine({ summary }: { summary: SummaryResult }) {
  if (summary.source === 'cached') {
    return <span title="ai_results 命中，无 token 消耗">{summary.model} · 缓存命中 · 0 token</span>;
  }
  const parts: string[] = [summary.model];
  if (summary.inputTokens !== null) {
    parts.push(`输入 ${String(summary.inputTokens)}`);
  }
  if (summary.outputTokens !== null) {
    parts.push(`输出 ${String(summary.outputTokens)}`);
  }
  if (summary.cacheReadTokens !== null && summary.cacheReadTokens > 0) {
    parts.push(`prompt 缓存命中 ${String(summary.cacheReadTokens)}`);
  }
  parts.push(`语言 ${summary.language}`);
  return <span>{parts.join(' · ')}</span>;
}
