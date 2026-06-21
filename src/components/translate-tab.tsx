import { useAiStore } from '../lib/store/ai';
import { useMailStore } from '../lib/store/mail';
import { TranslationView } from './translation-view';

const DEFAULT_TRANSLATE_TARGET = 'zh-CN';

export function TranslateTab() {
  const translation = useAiStore((s) => s.translation);
  const translating = useAiStore((s) => s.translating);
  const translate = useAiStore((s) => s.translate);
  const clearTranslation = useAiStore((s) => s.clearTranslation);
  const models = useAiStore((s) => s.models);
  const roleDefaults = useAiStore((s) => s.roleDefaults);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const body = useMailStore((s) => s.body);

  if (selectedMessageId === null) {
    return <p className="text-sm text-text-3">在左侧选一封邮件再用翻译。</p>;
  }

  const translateDefault = roleDefaults.find((r) => r.role === 'translate');
  const translateReady =
    translateDefault !== undefined && models.some((m) => m.id === translateDefault.modelId);
  const enabled = body !== null && translateReady;

  let placeholder: string;
  if (body === null) {
    placeholder = '等待正文加载…';
  } else if (!translateReady) {
    placeholder = '尚未在 ⚙ AI 模型配置中指派翻译模型。';
  } else {
    placeholder = `点击「翻译」将邮件翻译为 ${DEFAULT_TRANSLATE_TARGET}。`;
  }

  return (
    <div className="space-y-3">
      <header className="flex items-center justify-between gap-2">
        <h4 className="text-sm font-semibold text-text-1">AI 翻译</h4>
        <div className="flex gap-2">
          <button
            type="button"
            disabled={!enabled || translating}
            onClick={() => {
              void translate(selectedMessageId, DEFAULT_TRANSLATE_TARGET);
            }}
            className="rounded bg-accent px-3 py-1 text-xs font-medium text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
            title={translateReady ? `翻译为 ${DEFAULT_TRANSLATE_TARGET}` : '未配置翻译模型'}
          >
            {translating ? '翻译中…' : translation ? '重新翻译' : '翻译'}
          </button>
          {translation && (
            <button
              type="button"
              onClick={() => {
                clearTranslation();
              }}
              className="rounded border border-[var(--color-border)] bg-panel px-3 py-1 text-xs font-medium text-text-1 hover:opacity-90"
            >
              清除
            </button>
          )}
        </div>
      </header>

      {!translation && !translating && <p className="text-xs text-text-3">{placeholder}</p>}
      {translating && (
        <p className="animate-pulse text-xs text-text-3">翻译中（长邮件可能需要 5–15 秒）…</p>
      )}
      {translation && <TranslationView translation={translation} />}
    </div>
  );
}
