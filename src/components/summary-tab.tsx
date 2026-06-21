import { useAiStore } from '../lib/store/ai';
import { useMailStore } from '../lib/store/mail';
import { UsageLine } from './translation-view';

export function SummaryTab() {
  const summary = useAiStore((s) => s.summary);
  const summarizing = useAiStore((s) => s.summarizing);
  const summarize = useAiStore((s) => s.summarize);
  const models = useAiStore((s) => s.models);
  const roleDefaults = useAiStore((s) => s.roleDefaults);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const body = useMailStore((s) => s.body);

  if (selectedMessageId === null) {
    return <p className="text-sm text-text-3">在左侧选一封邮件再用摘要。</p>;
  }

  const summaryDefault = roleDefaults.find((r) => r.role === 'summary');
  const summaryReady =
    summaryDefault !== undefined && models.some((m) => m.id === summaryDefault.modelId);
  const enabled = body !== null && summaryReady;

  let placeholder: string;
  if (body === null) {
    placeholder = '等待正文加载…';
  } else if (!summaryReady) {
    placeholder = '尚未在 ⚙ AI 模型配置中指派摘要模型。';
  } else {
    placeholder = '点击「总结」提炼要点。';
  }

  return (
    <div className="space-y-3">
      <header className="flex items-center justify-between gap-2">
        <h4 className="text-sm font-semibold text-text-1">AI 摘要</h4>
        <button
          type="button"
          disabled={!enabled || summarizing}
          onClick={() => {
            void summarize(selectedMessageId);
          }}
          className="rounded bg-accent px-3 py-1 text-xs font-medium text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {summarizing ? '生成中…' : summary ? '重新总结' : '总结'}
        </button>
      </header>

      {!summary && !summarizing && <p className="text-xs text-text-3">{placeholder}</p>}
      {summarizing && (
        <p className="animate-pulse text-xs text-text-3">模型思考中（首次约 1–3 秒）…</p>
      )}
      {summary && (
        <div className="space-y-3">
          <p className="text-sm font-medium text-text-1">{summary.tldr}</p>
          {summary.bullets.length > 0 && (
            <ul className="list-disc space-y-1 pl-5 text-sm text-text-2">
              {summary.bullets.map((b, i) => (
                <li key={`${String(i)}-${b.slice(0, 8)}`}>{b}</li>
              ))}
            </ul>
          )}
          <footer className="text-[10px] text-text-3">
            <UsageLine
              source={summary.source}
              model={summary.model}
              inputTokens={summary.inputTokens}
              outputTokens={summary.outputTokens}
              cacheReadTokens={summary.cacheReadTokens}
              extra={`语言 ${summary.language}`}
            />
          </footer>
        </div>
      )}
    </div>
  );
}
