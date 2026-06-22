// Modal for adding a new email account. User picks a provider preset → host/ports auto-fill;
// they type email + display name + authorization code; on submit we call the store, which
// stores the auth code in the OS keychain and kicks off the first inbox sync in background.

import { useState } from 'react';

/**
 * #69: 解析端口字符串为整数，返回有效端口数值，无效时返回 null。
 *
 * 规则（与后端 validate_port(i32) 一致）：
 * - 必须是整数（小数部分非零则拒绝，1.0 → 1 可接受）
 * - 范围 1–65535（TCP 端口合法范围）
 *
 * 导出供测试，不属于公开 API。
 */
export function parsePort(raw: string): number | null {
  const trimmed = raw.trim();
  if (trimmed === '') return null;
  const asNum = Number(trimmed);
  if (!Number.isFinite(asNum)) return null;
  const asInt = Math.trunc(asNum);
  // 拒绝小数部分非零的值（如 993.5），但接受 1.0 → 1
  if (asNum !== asInt) return null;
  if (asInt < 1 || asInt > 65535) return null;
  return asInt;
}

import { useMailStore } from '../lib/store/mail';
import { providerById, PROVIDERS, type ProviderId } from '../lib/providers';

interface Props {
  open: boolean;
  onClose: () => void;
}

const DEFAULT_PROVIDER: ProviderId = 'qq';

export function AddAccountDialog({ open, onClose }: Props) {
  const addAccount = useMailStore((s) => s.addAccount);

  const [providerId, setProviderId] = useState<ProviderId>(DEFAULT_PROVIDER);
  const preset = providerById(providerId);

  const [email, setEmail] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [authCode, setAuthCode] = useState('');
  const [imapHost, setImapHost] = useState(preset.imapHost);
  // #69: 端口存为字符串以保留用户输入的原始值，提交时经 parsePort 校验为合法整数
  const [imapPort, setImapPort] = useState(String(preset.imapPort));
  const [smtpHost, setSmtpHost] = useState(preset.smtpHost);
  const [smtpPort, setSmtpPort] = useState(String(preset.smtpPort));
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // #47: reset all fields (including sensitive auth code) when the dialog is closed or
  // cancelled, so state does not linger across open cycles.
  function resetForm() {
    const defaultPreset = providerById(DEFAULT_PROVIDER);
    setProviderId(DEFAULT_PROVIDER);
    setEmail('');
    setDisplayName('');
    setAuthCode('');
    setImapHost(defaultPreset.imapHost);
    setImapPort(String(defaultPreset.imapPort));
    setSmtpHost(defaultPreset.smtpHost);
    setSmtpPort(String(defaultPreset.smtpPort));
    setError(null);
  }

  function handleClose() {
    // #52: ignore close requests while a submit is in flight — mirrors the cancel button's
    // disabled={submitting} guard so the backdrop click is consistent with it.
    if (submitting) return;
    resetForm();
    onClose();
  }

  function changeProvider(next: ProviderId) {
    setProviderId(next);
    const p = providerById(next);
    setImapHost(p.imapHost);
    setImapPort(String(p.imapPort));
    setSmtpHost(p.smtpHost);
    setSmtpPort(String(p.smtpPort));
  }

  async function onSubmit(e: React.SyntheticEvent<HTMLFormElement>) {
    e.preventDefault();
    // #69: 在提交前解析并校验端口，确保传给后端的是合法整数（对应后端 i32 端口字段）
    const parsedImapPort = parsePort(imapPort);
    const parsedSmtpPort = parsePort(smtpPort);
    if (parsedImapPort === null) {
      setError('IMAP 端口必须是 1–65535 之间的整数');
      return;
    }
    if (parsedSmtpPort === null) {
      setError('SMTP 端口必须是 1–65535 之间的整数');
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      await addAccount({
        email: email.trim(),
        displayName: displayName.trim() || null,
        provider: providerId,
        imapHost: imapHost.trim(),
        imapPort: parsedImapPort,
        smtpHost: smtpHost.trim(),
        smtpPort: parsedSmtpPort,
        authCode: authCode.trim(),
      });
      resetForm();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  if (!open) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4"
      onClick={handleClose}
    >
      <form
        onClick={(e) => {
          e.stopPropagation();
        }}
        onSubmit={(e) => {
          void onSubmit(e);
        }}
        className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl dark:bg-slate-900"
      >
        <h2 className="text-lg font-semibold text-slate-900 dark:text-slate-100">添加邮箱账户</h2>
        <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">{preset.authCodeHelp}</p>

        <div className="mt-4 grid gap-3">
          <label className="text-xs">
            <span className="block font-medium text-slate-700 dark:text-slate-300">服务商</span>
            <select
              value={providerId}
              onChange={(e) => {
                changeProvider(e.currentTarget.value as ProviderId);
              }}
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
            >
              {PROVIDERS.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </label>

          <label className="text-xs">
            <span className="block font-medium text-slate-700 dark:text-slate-300">邮箱地址</span>
            <input
              type="email"
              required
              value={email}
              onChange={(e) => {
                setEmail(e.currentTarget.value);
              }}
              placeholder="you@qq.com"
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
            />
          </label>

          <label className="text-xs">
            <span className="block font-medium text-slate-700 dark:text-slate-300">
              显示名（可选）
            </span>
            <input
              type="text"
              value={displayName}
              onChange={(e) => {
                setDisplayName(e.currentTarget.value);
              }}
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
            />
          </label>

          <label className="text-xs">
            <span className="block font-medium text-slate-700 dark:text-slate-300">
              授权码 / 应用专用密码
            </span>
            <input
              type="password"
              required
              autoComplete="off"
              value={authCode}
              onChange={(e) => {
                setAuthCode(e.currentTarget.value);
              }}
              placeholder="16 位 授权码"
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
            />
            <span className="mt-1 block text-[10px] text-slate-500">
              授权码会写入 OS keychain，不会写入数据库或日志。
            </span>
          </label>

          <details className="text-xs">
            <summary className="cursor-pointer font-medium text-slate-700 dark:text-slate-300">
              高级 — IMAP / SMTP 地址
            </summary>
            <div className="mt-2 grid grid-cols-2 gap-2">
              <label>
                <span className="block text-slate-600 dark:text-slate-400">IMAP host</span>
                <input
                  type="text"
                  required
                  value={imapHost}
                  onChange={(e) => {
                    setImapHost(e.currentTarget.value);
                  }}
                  className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
                />
              </label>
              <label>
                <span className="block text-slate-600 dark:text-slate-400">IMAP port</span>
                <input
                  type="number"
                  required
                  value={imapPort}
                  onChange={(e) => {
                    setImapPort(e.currentTarget.value);
                  }}
                  className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
                />
              </label>
              <label>
                <span className="block text-slate-600 dark:text-slate-400">SMTP host</span>
                <input
                  type="text"
                  required
                  value={smtpHost}
                  onChange={(e) => {
                    setSmtpHost(e.currentTarget.value);
                  }}
                  className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
                />
              </label>
              <label>
                <span className="block text-slate-600 dark:text-slate-400">SMTP port</span>
                <input
                  type="number"
                  required
                  value={smtpPort}
                  onChange={(e) => {
                    setSmtpPort(e.currentTarget.value);
                  }}
                  className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
                />
              </label>
            </div>
          </details>

          {error && (
            <p className="rounded bg-red-50 px-2 py-1 text-xs text-red-700 dark:bg-red-950 dark:text-red-300">
              {error}
            </p>
          )}
        </div>

        <div className="mt-5 flex justify-end gap-2">
          <button
            type="button"
            onClick={handleClose}
            disabled={submitting}
            className="rounded px-3 py-1 text-sm text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
          >
            取消
          </button>
          <button
            type="submit"
            disabled={submitting}
            className="rounded bg-blue-600 px-3 py-1 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            {submitting ? '保存中…' : '添加并同步'}
          </button>
        </div>
      </form>
    </div>
  );
}
