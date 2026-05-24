// AI assist panel embedded in the message detail view.
//
// Sprint 2 surface: summarize only. Sprint 3 will add classify chips, Sprint 4 a translate
// toggle, Sprint 5 a draft-reply trigger. Each operation hangs off useMailStore so the
// panel stays a presentation-only component.

import { useMailStore } from '../lib/store/mail';
import type { SummaryResult, TranslateResult } from '../lib/types';

const DEFAULT_TRANSLATE_TARGET = 'zh-CN';

export function AiPanel() {
  const body = useMailStore((s) => s.body);
  const summary = useMailStore((s) => s.summary);
  const summarizing = useMailStore((s) => s.summarizing);
  const summarize = useMailStore((s) => s.summarizeSelectedMessage);
  const translation = useMailStore((s) => s.translation);
  const translating = useMailStore((s) => s.translating);
  const translate = useMailStore((s) => s.translateSelectedMessage);
  const clearTranslation = useMailStore((s) => s.clearTranslation);
  const roleDefaults = useMailStore((s) => s.roleDefaults);
  const models = useMailStore((s) => s.models);

  const summaryDefault = roleDefaults.find((r) => r.role === 'summary');
  const translateDefault = roleDefaults.find((r) => r.role === 'translate');
  const summaryReady =
    summaryDefault !== undefined && models.some((m) => m.id === summaryDefault.modelId);
  const translateReady =
    translateDefault !== undefined && models.some((m) => m.id === translateDefault.modelId);

  const hasBody = body !== null;
  const summarizeEnabled = hasBody && summaryReady;
  const translateEnabled = hasBody && translateReady;

  let placeholder: string;
  if (!hasBody) {
    placeholder = '等待正文加载…';
  } else if (!summaryReady && !translateReady) {
    placeholder = '尚未在 ⚙ AI 模型配置中指派模型到摘要 / 翻译角色。';
  } else {
    placeholder = '点击「总结」提炼要点，或「翻译」转换为中文。';
  }

  return (
    <section className="mt-4 rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-700 dark:bg-slate-900">
      <header className="flex items-center justify-between gap-2">
        <h4 className="text-sm font-semibold text-slate-800 dark:text-slate-100">AI 助手</h4>
        <div className="flex gap-2">
          <button
            type="button"
            disabled={!translateEnabled || translating}
            onClick={() => {
              if (translation) {
                clearTranslation();
              } else {
                void translate(DEFAULT_TRANSLATE_TARGET);
              }
            }}
            className="rounded bg-slate-100 px-3 py-1 text-xs font-medium text-slate-700 hover:bg-slate-200 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-slate-800 dark:text-slate-200 dark:hover:bg-slate-700"
            title={translateReady ? `翻译为 ${DEFAULT_TRANSLATE_TARGET}` : '未配置翻译模型'}
          >
            {translating ? '翻译中…' : translation ? '隐藏翻译' : '翻译'}
          </button>
          <button
            type="button"
            disabled={!summarizeEnabled || summarizing}
            onClick={() => {
              void summarize();
            }}
            className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {summarizing ? '生成中…' : summary ? '重新总结' : '总结'}
          </button>
        </div>
      </header>

      {!summary && !summarizing && !translation && !translating && (
        <p className="mt-3 text-xs text-slate-500 dark:text-slate-400">{placeholder}</p>
      )}

      {summarizing && (
        <p className="mt-3 animate-pulse text-xs text-slate-500 dark:text-slate-400">
          模型思考中（首次约 1–3 秒）…
        </p>
      )}
      {summary && <SummaryView summary={summary} />}

      {translating && (
        <p className="mt-3 animate-pulse text-xs text-slate-500 dark:text-slate-400">
          翻译中（长邮件可能需要 5–15 秒）…
        </p>
      )}
      {translation && <TranslationView translation={translation} />}
    </section>
  );
}

function TranslationView({ translation }: { translation: TranslateResult }) {
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

function UsageLine(props: UsageProps) {
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
