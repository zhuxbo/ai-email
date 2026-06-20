import { useAiStore } from '../lib/store/ai';
import { useMailStore } from '../lib/store/mail';
import { TranslationView } from './translation-view';

const DEFAULT_TRANSLATE_TARGET = 'zh-CN';

export function MessageActions() {
  const body = useMailStore((s) => s.body);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const openComposer = useMailStore((s) => s.openComposer);
  const translation = useAiStore((s) => s.translation);
  const translating = useAiStore((s) => s.translating);
  const translate = useAiStore((s) => s.translate);
  const clearTranslation = useAiStore((s) => s.clearTranslation);
  const models = useAiStore((s) => s.models);
  const roleDefaults = useAiStore((s) => s.roleDefaults);

  const translateDefault = roleDefaults.find((r) => r.role === 'translate');
  const translateReady =
    translateDefault !== undefined && models.some((m) => m.id === translateDefault.modelId);

  // 早 return 让闭包里 selectedMessageId 收窄为 string，无需 non-null assertion。
  if (selectedMessageId === null) return null;

  const hasBody = body !== null;

  return (
    <div className="mt-4 border-t border-[var(--color-border)] pt-3">
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={openComposer}
          disabled={!hasBody}
          className="rounded bg-accent px-3 py-1 text-xs font-medium text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          title={hasBody ? '回复这封邮件' : '等待正文加载后才能回复'}
        >
          回复
        </button>
        <button
          type="button"
          disabled={!hasBody || !translateReady || translating}
          onClick={() => {
            if (translation) {
              clearTranslation();
            } else {
              void translate(selectedMessageId, DEFAULT_TRANSLATE_TARGET);
            }
          }}
          className="rounded border border-[var(--color-border)] bg-panel px-3 py-1 text-xs font-medium text-text-1 hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          title={translateReady ? `翻译为 ${DEFAULT_TRANSLATE_TARGET}` : '未配置翻译模型'}
        >
          {translating ? '翻译中…' : translation ? '隐藏翻译' : '翻译'}
        </button>
        <button
          type="button"
          disabled
          title="P3 接入"
          className="rounded border border-[var(--color-border)] px-3 py-1 text-xs font-medium text-text-3 disabled:cursor-not-allowed"
        >
          AI 写
        </button>
        <button
          type="button"
          disabled
          title="P3 接入"
          className="rounded border border-[var(--color-border)] px-3 py-1 text-xs font-medium text-text-3 disabled:cursor-not-allowed"
        >
          归档
        </button>
        <button
          type="button"
          disabled
          title="P3 接入"
          className="rounded border border-[var(--color-border)] px-3 py-1 text-xs font-medium text-text-3 disabled:cursor-not-allowed"
        >
          删除
        </button>
      </div>

      {translating && (
        <p className="mt-3 animate-pulse text-xs text-text-3">翻译中（长邮件可能需要 5–15 秒）…</p>
      )}
      {translation && <TranslationView translation={translation} />}
    </div>
  );
}
