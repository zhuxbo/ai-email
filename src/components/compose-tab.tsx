import { useComposeStore } from '../lib/store/compose';
import { useMailStore } from '../lib/store/mail';

export function ComposeTab() {
  const replyContext = useComposeStore((s) => s.replyContext);
  const fromAccountId = useComposeStore((s) => s.fromAccountId);
  const to = useComposeStore((s) => s.to);
  const cc = useComposeStore((s) => s.cc);
  const subject = useComposeStore((s) => s.subject);
  const intentZh = useComposeStore((s) => s.intentZh);
  const bodyForeign = useComposeStore((s) => s.bodyForeign);
  const bodyZhBack = useComposeStore((s) => s.bodyZhBack);
  const bilingual = useComposeStore((s) => s.bilingual);
  const aiAssisted = useComposeStore((s) => s.aiAssisted);
  const draftSource = useComposeStore((s) => s.draftSource);
  const drafting = useComposeStore((s) => s.drafting);
  const backTranslating = useComposeStore((s) => s.backTranslating);
  const sending = useComposeStore((s) => s.sending);
  const error = useComposeStore((s) => s.error);
  const receiptInfo = useComposeStore((s) => s.receiptInfo);
  const setField = useComposeStore((s) => s.setField);
  const runDraft = useComposeStore((s) => s.runDraft);
  const refreshBackTranslation = useComposeStore((s) => s.refreshBackTranslation);
  const runSend = useComposeStore((s) => s.runSend);

  const accounts = useMailStore((s) => s.accounts);

  const isReply = replyContext !== null;

  // 当前账户的展示名
  const fromAccount = accounts.find((a) => a.id === fromAccountId);
  const fromLabel = fromAccount
    ? (fromAccount.displayName ?? fromAccount.email)
    : (fromAccountId ?? '（账户加载中…）');

  return (
    <form
      className="space-y-3 text-xs"
      onSubmit={(e) => {
        e.preventDefault();
        void runSend();
      }}
    >
      {/* 发送账户 */}
      <div>
        <label htmlFor="compose-from" className="block font-medium text-text-2">
          发送账户
        </label>
        {isReply ? (
          <p className="mt-1 rounded border border-border-1 bg-surface-2 px-2 py-1 text-text-1">
            {fromLabel}
          </p>
        ) : (
          <select
            id="compose-from"
            value={fromAccountId ?? ''}
            onChange={(e) => {
              setField({ fromAccountId: e.currentTarget.value });
            }}
            className="mt-1 w-full rounded border border-border-1 bg-surface-1 px-2 py-1 text-text-1"
          >
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.displayName ?? a.email}
              </option>
            ))}
          </select>
        )}
      </div>

      {/* 收件人 */}
      <label className="block">
        <span className="block font-medium text-text-2">收件人</span>
        <input
          type="text"
          required
          value={to}
          onChange={(e) => {
            setField({ to: e.currentTarget.value });
          }}
          className="mt-1 w-full rounded border border-border-1 bg-surface-1 px-2 py-1 text-text-1"
        />
      </label>

      {/* 抄送 */}
      <label className="block">
        <span className="block font-medium text-text-2">抄送</span>
        <input
          type="text"
          value={cc}
          onChange={(e) => {
            setField({ cc: e.currentTarget.value });
          }}
          className="mt-1 w-full rounded border border-border-1 bg-surface-1 px-2 py-1 text-text-1"
        />
      </label>

      {/* 主题 */}
      <label className="block">
        <span className="block font-medium text-text-2">主题</span>
        <input
          type="text"
          required
          value={subject}
          onChange={(e) => {
            setField({ subject: e.currentTarget.value });
          }}
          className="mt-1 w-full rounded border border-border-1 bg-surface-1 px-2 py-1 text-text-1"
        />
      </label>

      {/* 回复模式：中文意图 + AI 起草 */}
      {isReply && (
        <div>
          <label className="block">
            <span className="block font-medium text-text-2">中文意图（可选）</span>
            <div className="mt-1 flex gap-2">
              <input
                type="text"
                value={intentZh}
                onChange={(e) => {
                  setField({ intentZh: e.currentTarget.value });
                }}
                placeholder='例如"婉拒"、"确认约下周一"'
                className="flex-1 rounded border border-border-1 bg-surface-1 px-2 py-1 text-text-1"
              />
              <button
                type="button"
                disabled={drafting}
                onClick={() => {
                  void runDraft();
                }}
                className="rounded bg-surface-2 px-3 py-1 font-medium text-text-1 hover:bg-surface-3 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {drafting ? '起草中…' : 'AI 起草'}
              </button>
              {/* #71 仅在有 AI 草稿时显示"重新生成"按钮，避免覆盖纯手写正文 */}
              {(aiAssisted || draftSource !== null) && (
                <button
                  type="button"
                  disabled={drafting}
                  onClick={() => {
                    void runDraft(true);
                  }}
                  className="rounded bg-surface-2 px-2 py-1 text-[10px] font-medium text-text-2 hover:bg-surface-3 disabled:cursor-not-allowed disabled:opacity-50"
                  title="强制重新生成，忽略缓存"
                >
                  {drafting ? '…' : '重新生成'}
                </button>
              )}
            </div>
          </label>
        </div>
      )}

      {/* 正文（外文） */}
      <label className="block">
        <span className="block font-medium text-text-2">正文（外文）</span>
        <textarea
          required
          value={bodyForeign}
          onChange={(e) => {
            setField({ bodyForeign: e.currentTarget.value });
          }}
          rows={10}
          className="mt-1 w-full rounded border border-border-1 bg-surface-1 px-2 py-1 font-mono text-text-1"
        />
      </label>

      {/* 中文对照 */}
      {bilingual && bodyZhBack !== null && (
        <div>
          <div className="flex items-center justify-between">
            <span className="block font-medium text-text-2">中文对照</span>
            <button
              type="button"
              disabled={backTranslating}
              onClick={() => {
                void refreshBackTranslation();
              }}
              className="rounded bg-surface-2 px-2 py-0.5 text-[10px] font-medium text-text-2 hover:bg-surface-3 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {backTranslating ? '翻译中…' : '刷新对照'}
            </button>
          </div>
          <p className="mt-1 rounded border border-border-1 bg-surface-2 px-2 py-1 text-text-1 whitespace-pre-wrap">
            {bodyZhBack}
          </p>
        </div>
      )}

      {/* 错误 */}
      {error && (
        <p className="rounded bg-red-50 px-2 py-1 text-xs text-red-700 dark:bg-red-950 dark:text-red-300">
          {error}
        </p>
      )}

      {/* 回执 */}
      {receiptInfo && (
        <p className="rounded bg-emerald-50 px-2 py-1 text-xs text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300">
          {receiptInfo}
        </p>
      )}

      {/* 发送 */}
      <div className="flex justify-end pt-1">
        <button
          type="submit"
          disabled={sending}
          className="rounded bg-accent px-4 py-1.5 font-medium text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {sending ? '发送中…' : '发送'}
        </button>
      </div>
    </form>
  );
}
