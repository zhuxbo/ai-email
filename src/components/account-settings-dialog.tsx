// 账户管理对话框：列出所有邮箱账户，每个可「编辑」（显示名 / IMAP·SMTP / 授权码留空不改）
// 或「删除」（二次确认）。邮箱地址与服务商作为账户标识固定不可改。
//
// 取代了 nav-rail 头像「右键直接删除」的旧交互——删除现在必须经此对话框 + window.confirm。

import { useState } from 'react';

import { useMailStore } from '../lib/store/mail';
import { parsePort } from './add-account-dialog';
import type { Account, UpdateAccountForm } from '../lib/types';

export function AccountsPanel() {
  const accounts = useMailStore((s) => s.accounts);

  return (
    <div className="space-y-3">
      {accounts.length === 0 ? (
        <p className="rounded border border-dashed border-slate-300 p-3 text-xs text-slate-500 dark:border-slate-600 dark:text-slate-400">
          还没有账户。关闭本窗口后，点左栏 ＋ 添加邮箱。
        </p>
      ) : (
        accounts.map((a) => <AccountRow key={a.id} account={a} />)
      )}
    </div>
  );
}

function AccountRow({ account }: { account: Account }) {
  const removeAccount = useMailStore((s) => s.removeAccount);
  const [editing, setEditing] = useState(false);

  if (editing) {
    return (
      <div className="rounded border border-blue-300 p-3 dark:border-blue-700">
        <AccountEditForm
          account={account}
          onDone={() => {
            setEditing(false);
          }}
        />
      </div>
    );
  }

  return (
    <div className="flex items-start justify-between gap-3 rounded border border-slate-200 p-3 text-sm dark:border-slate-700">
      <div className="min-w-0 flex-1">
        <div className="font-medium text-slate-900 dark:text-slate-100">
          {account.displayName ?? account.email}
        </div>
        <div className="break-all text-xs text-slate-500 dark:text-slate-400">
          {account.email} · {account.provider}
        </div>
        <div className="break-all text-xs text-slate-500 dark:text-slate-400">
          IMAP {account.imapHost}:{account.imapPort} · SMTP {account.smtpHost}:{account.smtpPort}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <button
          type="button"
          onClick={() => {
            setEditing(true);
          }}
          className="rounded px-2 py-1 text-xs text-blue-600 hover:bg-blue-50 dark:text-blue-400 dark:hover:bg-blue-950"
        >
          编辑
        </button>
        <button
          type="button"
          onClick={() => {
            if (
              window.confirm(
                `确认删除账户「${account.email}」？授权码会从 keychain 删除，本地邮件清空。`,
              )
            ) {
              void removeAccount(account.id);
            }
          }}
          className="rounded px-2 py-1 text-xs text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
        >
          删除
        </button>
      </div>
    </div>
  );
}

function AccountEditForm({ account, onDone }: { account: Account; onDone: () => void }) {
  const updateAccount = useMailStore((s) => s.updateAccount);

  const [displayName, setDisplayName] = useState(account.displayName ?? '');
  const [imapHost, setImapHost] = useState(account.imapHost);
  const [imapPort, setImapPort] = useState(String(account.imapPort));
  const [smtpHost, setSmtpHost] = useState(account.smtpHost);
  const [smtpPort, setSmtpPort] = useState(String(account.smtpPort));
  const [authCode, setAuthCode] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  async function onSubmit(e: React.SyntheticEvent<HTMLFormElement>) {
    e.preventDefault();
    const pImap = parsePort(imapPort);
    const pSmtp = parsePort(smtpPort);
    if (pImap === null) {
      setLocalError('IMAP 端口必须是 1–65535 之间的整数');
      return;
    }
    if (pSmtp === null) {
      setLocalError('SMTP 端口必须是 1–65535 之间的整数');
      return;
    }
    setSubmitting(true);
    setLocalError(null);
    try {
      const form: UpdateAccountForm = {
        displayName: displayName.trim() === '' ? null : displayName.trim(),
        imapHost: imapHost.trim(),
        imapPort: pImap,
        smtpHost: smtpHost.trim(),
        smtpPort: pSmtp,
      };
      // 授权码留空＝保持原值；只有填了新值才覆盖 keychain。
      const code = authCode.trim();
      if (code !== '') form.authCode = code;
      await updateAccount(account.id, form);
      onDone();
    } catch (err) {
      setLocalError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      onSubmit={(e) => {
        void onSubmit(e);
      }}
      className="space-y-3"
    >
      <div className="flex items-center justify-between">
        <span className="break-all text-xs font-semibold text-slate-700 dark:text-slate-300">
          编辑「{account.email}」
        </span>
        <span className="shrink-0 text-[10px] text-slate-500">
          {account.provider}（邮箱与服务商不可改）
        </span>
      </div>

      <label className="block text-xs">
        <span className="block font-medium text-slate-700 dark:text-slate-300">显示名</span>
        <input
          type="text"
          value={displayName}
          onChange={(e) => {
            setDisplayName(e.currentTarget.value);
          }}
          placeholder="可选"
          className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
        />
      </label>

      <div className="grid grid-cols-2 gap-2 text-xs">
        <label>
          <span className="block font-medium text-slate-700 dark:text-slate-300">IMAP host</span>
          <input
            type="text"
            required
            value={imapHost}
            onChange={(e) => {
              setImapHost(e.currentTarget.value);
            }}
            className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
          />
        </label>
        <label>
          <span className="block font-medium text-slate-700 dark:text-slate-300">IMAP port</span>
          <input
            type="number"
            required
            value={imapPort}
            onChange={(e) => {
              setImapPort(e.currentTarget.value);
            }}
            className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
          />
        </label>
        <label>
          <span className="block font-medium text-slate-700 dark:text-slate-300">SMTP host</span>
          <input
            type="text"
            required
            value={smtpHost}
            onChange={(e) => {
              setSmtpHost(e.currentTarget.value);
            }}
            className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
          />
        </label>
        <label>
          <span className="block font-medium text-slate-700 dark:text-slate-300">SMTP port</span>
          <input
            type="number"
            required
            value={smtpPort}
            onChange={(e) => {
              setSmtpPort(e.currentTarget.value);
            }}
            className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
          />
        </label>
      </div>

      <label className="block text-xs">
        <span className="block font-medium text-slate-700 dark:text-slate-300">
          授权码 / 应用专用密码
        </span>
        <input
          type="password"
          autoComplete="off"
          value={authCode}
          onChange={(e) => {
            setAuthCode(e.currentTarget.value);
          }}
          placeholder="留空＝保持原授权码不变"
          className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 font-mono dark:border-slate-600 dark:bg-slate-800"
        />
      </label>

      {localError && (
        <p className="rounded bg-red-50 px-2 py-1 text-xs text-red-700 dark:bg-red-950 dark:text-red-300">
          {localError}
        </p>
      )}

      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onDone}
          className="rounded px-3 py-1 text-sm text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
        >
          取消
        </button>
        <button
          type="submit"
          disabled={submitting}
          className="rounded bg-blue-600 px-3 py-1 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
        >
          {submitting ? '保存中…' : '保存'}
        </button>
      </div>
    </form>
  );
}
