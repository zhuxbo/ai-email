// Reply composer modal. Opens from the 回复 button in MessageDetail.
//
// Flow:
//   1. Pre-fill To from the original sender, Subject with "Re: <original>".
//   2. User can type the body freely OR click "AI 起草" with an optional intent ("婉拒",
//      "约下周一", etc.) to have the configured draft model produce a draft. The model
//      response replaces the current body — user can edit before sending.
//   3. Click 发送 → smtp_send → server writes to send_log (success or failure both audited).
//   4. On success, modal closes and an info toast confirms the audit log id.
//
// Hard rule from SPEC § 9: no auto-send. User must click 发送 explicitly each time.

import { useEffect, useMemo, useRef, useState } from 'react';

import * as tauri from '../lib/tauri';
import { useMailStore } from '../lib/store/mail';
import { useAiStore } from '../lib/store/ai';
import type { MessageHeader } from '../lib/types';

function findMessage(messages: MessageHeader[], id: string | null): MessageHeader | null {
  if (id === null) return null;
  return messages.find((m) => m.id === id) ?? null;
}

function defaultSubject(original: string | null): string {
  if (original === null || original.trim() === '') return 'Re:';
  const trimmed = original.trim();
  if (/^re:\s/i.test(trimmed)) return trimmed;
  return `Re: ${trimmed}`;
}

export function ReplyComposer() {
  const open = useMailStore((s) => s.composerOpen);
  const close = useMailStore((s) => s.closeComposer);
  const messages = useMailStore((s) => s.messages);
  const selectedMessageId = useMailStore((s) => s.selectedMessageId);
  const roleDefaults = useAiStore((s) => s.roleDefaults);
  const models = useAiStore((s) => s.models);

  const message = useMemo(
    () => findMessage(messages, selectedMessageId),
    [messages, selectedMessageId],
  );

  const [to, setTo] = useState('');
  const [cc, setCc] = useState('');
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');
  const [intent, setIntent] = useState('');
  const [aiAssisted, setAiAssisted] = useState(false);
  const [drafting, setDrafting] = useState(false);
  const [sending, setSending] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [receiptInfo, setReceiptInfo] = useState<string | null>(null);
  const lastInitId = useRef<string | null>(null);

  const draftDefault = roleDefaults.find((r) => r.role === 'draft');
  const draftReady =
    draftDefault !== undefined && models.some((m) => m.id === draftDefault.modelId);

  useEffect(() => {
    if (!open) return;
    if (message === null) return;
    if (lastInitId.current === message.id) return;
    lastInitId.current = message.id;
    setTo(message.fromAddr ?? '');
    setCc('');
    setSubject(defaultSubject(message.subject));
    setBody('');
    setIntent('');
    setAiAssisted(false);
    setLocalError(null);
    setReceiptInfo(null);
  }, [open, message]);

  if (!open) return null;

  if (message === null) {
    return (
      <div
        role="dialog"
        aria-modal="true"
        className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4"
        onClick={close}
      >
        <div
          className="rounded bg-white p-6 text-sm dark:bg-slate-900"
          onClick={(e) => {
            e.stopPropagation();
          }}
        >
          先选中一封邮件再回复。
        </div>
      </div>
    );
  }

  async function runDraft() {
    if (message === null) return;
    setDrafting(true);
    setLocalError(null);
    try {
      const result = await tauri.aiDraftReply(message.id, intent.trim() || null);
      setBody(result.body);
      if (subject.trim() === '' || subject.trim() === 'Re:') {
        setSubject(result.subject);
      }
      setAiAssisted(true);
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : String(e));
    } finally {
      setDrafting(false);
    }
  }

  async function runSend() {
    if (message === null) return;
    if (!window.confirm('确认发送？此操作不可撤销，并会写入 send_log 审计表。')) return;
    setSending(true);
    setLocalError(null);
    try {
      const receipt = await tauri.smtpSend({
        accountId: message.accountId,
        to: to
          .split(',')
          .map((s) => s.trim())
          .filter((s) => s !== ''),
        cc: cc
          .split(',')
          .map((s) => s.trim())
          .filter((s) => s !== ''),
        subject: subject.trim(),
        body,
        inReplyTo: message.id,
        aiAssisted,
      });
      setReceiptInfo(
        `已发送，send_log ${receipt.sendLog.id.slice(0, 8)} · ${receipt.sendLog.smtpResponse ?? ''}`,
      );
      // Give the user a beat to see the receipt then close.
      setTimeout(() => {
        close();
      }, 1200);
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : String(e));
    } finally {
      setSending(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4"
      onClick={close}
    >
      <form
        onClick={(e) => {
          e.stopPropagation();
        }}
        onSubmit={(e) => {
          e.preventDefault();
          void runSend();
        }}
        className="flex w-full max-w-2xl flex-col overflow-hidden rounded-lg bg-white shadow-xl dark:bg-slate-900"
      >
        <header className="flex items-center justify-between border-b border-slate-200 px-6 py-3 dark:border-slate-700">
          <h2 className="text-lg font-semibold text-slate-900 dark:text-slate-100">回复</h2>
          <button
            type="button"
            onClick={close}
            disabled={sending}
            className="text-slate-500 hover:text-slate-700 disabled:opacity-50 dark:text-slate-400 dark:hover:text-slate-200"
            aria-label="关闭"
          >
            ×
          </button>
        </header>

        <div className="space-y-3 px-6 py-4 text-xs">
          <Field label="收件人 (用逗号分隔多个)">
            <input
              type="text"
              required
              value={to}
              onChange={(e) => {
                setTo(e.currentTarget.value);
              }}
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
            />
          </Field>

          <Field label="抄送 (可选)">
            <input
              type="text"
              value={cc}
              onChange={(e) => {
                setCc(e.currentTarget.value);
              }}
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
            />
          </Field>

          <Field label="主题">
            <input
              type="text"
              required
              value={subject}
              onChange={(e) => {
                setSubject(e.currentTarget.value);
              }}
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
            />
          </Field>

          <div className="flex items-end gap-2">
            <div className="flex-1">
              <Field label={`AI 起草意图 (可选，例如 "婉拒"、"约下周一")`}>
                <input
                  type="text"
                  value={intent}
                  onChange={(e) => {
                    setIntent(e.currentTarget.value);
                  }}
                  className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
                />
              </Field>
            </div>
            <button
              type="button"
              disabled={!draftReady || drafting}
              onClick={() => {
                void runDraft();
              }}
              className="rounded bg-slate-100 px-3 py-1 text-xs font-medium text-slate-700 hover:bg-slate-200 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-slate-800 dark:text-slate-200 dark:hover:bg-slate-700"
              title={draftReady ? 'AI 起草回复' : '未在 ⚙ AI 配置 中指派起草角色'}
            >
              {drafting ? '起草中…' : 'AI 起草'}
            </button>
          </div>

          <Field label={`正文${aiAssisted ? '（AI 起草，发送时会标记 ai_assisted=true）' : ''}`}>
            <textarea
              required
              value={body}
              onChange={(e) => {
                setBody(e.currentTarget.value);
                setAiAssisted(false);
              }}
              rows={12}
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 font-mono text-sm dark:border-slate-600 dark:bg-slate-800"
            />
          </Field>

          {localError && (
            <p className="rounded bg-red-50 px-2 py-1 text-xs text-red-700 dark:bg-red-950 dark:text-red-300">
              {localError}
            </p>
          )}
          {receiptInfo && (
            <p className="rounded bg-emerald-50 px-2 py-1 text-xs text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300">
              {receiptInfo}
            </p>
          )}
        </div>

        <footer className="flex justify-end gap-2 border-t border-slate-200 px-6 py-3 dark:border-slate-700">
          <button
            type="button"
            onClick={close}
            disabled={sending}
            className="rounded px-3 py-1 text-sm text-slate-600 hover:bg-slate-100 disabled:opacity-50 dark:text-slate-300 dark:hover:bg-slate-800"
          >
            取消
          </button>
          <button
            type="submit"
            disabled={sending}
            className="rounded bg-blue-600 px-4 py-1 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            {sending ? '发送中…' : '发送'}
          </button>
        </footer>
      </form>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="block font-medium text-slate-700 dark:text-slate-300">{label}</span>
      {children}
    </label>
  );
}
