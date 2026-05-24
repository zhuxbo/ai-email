// AI provider settings modal. Three sections:
//   1. Configured models list (delete buttons)
//   2. Add-model form (preset picker → display name + API key → submit)
//   3. Role assignments (which model serves summary / classify / translate / draft)
//
// Backend invariants (see commands/ai_config.rs):
//   • API key goes straight to OS keychain — never DB, never logs, never wire round-trip.
//   • ON DELETE RESTRICT on ai_role_defaults.model_id — deleting a model still serving a
//     role surfaces a clear SQL error. We pre-check and prompt instead of letting the DB
//     reject silently.

import { useEffect, useState } from 'react';

import { PRESETS, presetById, type AiPreset } from '../lib/ai-presets';
import { useMailStore } from '../lib/store/mail';
import type { AiModel, AiRole } from '../lib/types';

interface Props {
  open: boolean;
  onClose: () => void;
}

const ROLE_LABELS: { role: AiRole; label: string; description: string }[] = [
  { role: 'summary', label: '摘要', description: '提炼邮件要点 (Sonnet 量级)' },
  { role: 'classify', label: '分类', description: '标签 + 优先级 (Haiku 量级，批处理)' },
  { role: 'translate', label: '翻译', description: '英文/日文邮件 → 中文 (Sonnet 量级)' },
  { role: 'draft', label: '起草回复', description: '生成回信草稿 (Sonnet 量级)' },
];

export function AiSettingsDialog({ open, onClose }: Props) {
  const models = useMailStore((s) => s.models);
  const roleDefaults = useMailStore((s) => s.roleDefaults);
  const loadAiConfig = useMailStore((s) => s.loadAiConfig);

  useEffect(() => {
    if (open) {
      void loadAiConfig();
    }
  }, [open, loadAiConfig]);

  if (!open) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4"
      onClick={onClose}
    >
      <div
        onClick={(e) => {
          e.stopPropagation();
        }}
        className="flex max-h-[90vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg bg-white shadow-xl dark:bg-slate-900"
      >
        <header className="flex items-center justify-between border-b border-slate-200 px-6 py-3 dark:border-slate-700">
          <h2 className="text-lg font-semibold text-slate-900 dark:text-slate-100">AI 模型配置</h2>
          <button
            type="button"
            onClick={onClose}
            className="text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200"
            aria-label="关闭"
          >
            ×
          </button>
        </header>

        <div className="flex-1 space-y-6 overflow-auto px-6 py-4">
          <ModelsSection models={models} roleDefaults={roleDefaults} />
          <AddModelSection />
          <RoleAssignmentsSection models={models} roleDefaults={roleDefaults} />
        </div>
      </div>
    </div>
  );
}

// ── Existing models list ────────────────────────────────────────────────────

function ModelsSection({
  models,
  roleDefaults,
}: {
  models: AiModel[];
  roleDefaults: { role: AiRole; modelId: string }[];
}) {
  const removeModel = useMailStore((s) => s.removeModel);

  if (models.length === 0) {
    return (
      <section>
        <h3 className="mb-2 text-sm font-semibold text-slate-700 dark:text-slate-300">
          已配置模型
        </h3>
        <p className="rounded border border-dashed border-slate-300 p-3 text-xs text-slate-500 dark:border-slate-600 dark:text-slate-400">
          尚未添加任何模型。在下面「添加模型」中开始 — 至少配置一个并将它指派给「摘要」角色才能使用
          AI 功能。
        </p>
      </section>
    );
  }

  return (
    <section>
      <h3 className="mb-2 text-sm font-semibold text-slate-700 dark:text-slate-300">已配置模型</h3>
      <ul className="space-y-2">
        {models.map((m) => {
          const usedFor = roleDefaults.filter((r) => r.modelId === m.id).map((r) => r.role);
          return (
            <li
              key={m.id}
              className="flex items-start justify-between gap-3 rounded border border-slate-200 p-3 text-sm dark:border-slate-700"
            >
              <div className="min-w-0 flex-1">
                <div className="font-medium text-slate-900 dark:text-slate-100">
                  {m.displayName}
                </div>
                <div className="text-xs text-slate-500 dark:text-slate-400">
                  {m.provider} · {m.modelId}
                  {m.baseUrl && (
                    <>
                      {' · '}
                      <span className="break-all">{m.baseUrl}</span>
                    </>
                  )}
                </div>
                {usedFor.length > 0 && (
                  <div className="mt-1 flex flex-wrap gap-1">
                    {usedFor.map((r) => (
                      <span
                        key={r}
                        className="rounded bg-blue-100 px-1.5 py-0.5 text-[10px] font-medium text-blue-700 dark:bg-blue-950 dark:text-blue-300"
                      >
                        {r}
                      </span>
                    ))}
                  </div>
                )}
              </div>
              <button
                type="button"
                onClick={() => {
                  if (usedFor.length > 0) {
                    window.alert(
                      `该模型正用于：${usedFor.join(', ')}。请先在「角色指派」中换一个模型，然后再删除。`,
                    );
                    return;
                  }
                  if (
                    window.confirm(`确认删除「${m.displayName}」？API key 会从 keychain 删除。`)
                  ) {
                    void removeModel(m.id).catch(() => {
                      /* error already surfaced via store */
                    });
                  }
                }}
                className="shrink-0 rounded px-2 py-1 text-xs text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
              >
                删除
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

// ── Add model form ──────────────────────────────────────────────────────────

function AddModelSection() {
  const addModel = useMailStore((s) => s.addModel);

  const [presetId, setPresetId] = useState<string>(PRESETS[0]?.id ?? 'anthropic');
  const preset = presetById(presetId);

  const [displayName, setDisplayName] = useState(preset.defaultDisplayName);
  const [modelId, setModelId] = useState(preset.modelId);
  const [baseUrl, setBaseUrl] = useState(preset.baseUrl ?? '');
  const [apiKey, setApiKey] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  function pickPreset(next: string) {
    setPresetId(next);
    const p = presetById(next);
    setDisplayName(p.defaultDisplayName);
    setModelId(p.modelId);
    setBaseUrl(p.baseUrl ?? '');
  }

  async function onSubmit(e: React.SyntheticEvent<HTMLFormElement>) {
    e.preventDefault();
    setSubmitting(true);
    setLocalError(null);
    try {
      await addModel({
        displayName: displayName.trim() || preset.defaultDisplayName || preset.label,
        provider: preset.provider,
        modelId: modelId.trim(),
        baseUrl: baseUrl.trim() === '' ? null : baseUrl.trim(),
        apiKey: apiKey.trim(),
      });
      setApiKey('');
      setLocalError(null);
    } catch (err) {
      setLocalError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <section>
      <h3 className="mb-2 text-sm font-semibold text-slate-700 dark:text-slate-300">添加模型</h3>
      <form
        onSubmit={(e) => {
          void onSubmit(e);
        }}
        className="space-y-3 rounded border border-slate-200 p-3 dark:border-slate-700"
      >
        <PresetField presetId={presetId} preset={preset} onChange={pickPreset} />

        <div className="grid grid-cols-2 gap-3 text-xs">
          <label>
            <span className="block font-medium text-slate-700 dark:text-slate-300">显示名</span>
            <input
              type="text"
              required
              value={displayName}
              onChange={(e) => {
                setDisplayName(e.currentTarget.value);
              }}
              placeholder={preset.defaultDisplayName}
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 dark:border-slate-600 dark:bg-slate-800"
            />
          </label>
          <label>
            <span className="block font-medium text-slate-700 dark:text-slate-300">Model ID</span>
            <input
              type="text"
              required
              value={modelId}
              onChange={(e) => {
                setModelId(e.currentTarget.value);
              }}
              placeholder={preset.modelId || 'e.g. gpt-4o'}
              className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 font-mono dark:border-slate-600 dark:bg-slate-800"
            />
          </label>
        </div>

        <label className="block text-xs">
          <span className="block font-medium text-slate-700 dark:text-slate-300">
            Base URL <span className="font-normal text-slate-500">(留空使用 provider 默认)</span>
          </span>
          <input
            type="url"
            value={baseUrl}
            onChange={(e) => {
              setBaseUrl(e.currentTarget.value);
            }}
            placeholder={preset.baseUrl ?? '默认'}
            className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 font-mono dark:border-slate-600 dark:bg-slate-800"
          />
        </label>

        <label className="block text-xs">
          <span className="block font-medium text-slate-700 dark:text-slate-300">API Key</span>
          <input
            type="password"
            required
            autoComplete="off"
            value={apiKey}
            onChange={(e) => {
              setApiKey(e.currentTarget.value);
            }}
            placeholder="sk-..."
            className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 font-mono dark:border-slate-600 dark:bg-slate-800"
          />
          <span className="mt-1 block text-[10px] text-slate-500">
            存入 OS keychain (service: com.zhuxbo.aiemail.ai)，不入库、不入日志。
          </span>
        </label>

        {localError && (
          <p className="rounded bg-red-50 px-2 py-1 text-xs text-red-700 dark:bg-red-950 dark:text-red-300">
            {localError}
          </p>
        )}

        <div className="flex justify-end">
          <button
            type="submit"
            disabled={submitting}
            className="rounded bg-blue-600 px-3 py-1 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            {submitting ? '保存中…' : '添加'}
          </button>
        </div>
      </form>
    </section>
  );
}

function PresetField({
  presetId,
  preset,
  onChange,
}: {
  presetId: string;
  preset: AiPreset;
  onChange: (id: string) => void;
}) {
  return (
    <label className="block text-xs">
      <span className="block font-medium text-slate-700 dark:text-slate-300">服务商</span>
      <select
        value={presetId}
        onChange={(e) => {
          onChange(e.currentTarget.value);
        }}
        className="mt-1 w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
      >
        {PRESETS.map((p) => (
          <option key={p.id} value={p.id}>
            {p.label}
          </option>
        ))}
      </select>
      <span className="mt-1 block text-[10px] text-slate-500">{preset.hint}</span>
    </label>
  );
}

// ── Role assignments ────────────────────────────────────────────────────────

function RoleAssignmentsSection({
  models,
  roleDefaults,
}: {
  models: AiModel[];
  roleDefaults: { role: AiRole; modelId: string }[];
}) {
  const setRoleDefault = useMailStore((s) => s.setRoleDefault);
  const clearRoleDefault = useMailStore((s) => s.clearRoleDefault);

  if (models.length === 0) {
    return null;
  }

  const lookup = new Map(roleDefaults.map((r) => [r.role, r.modelId]));

  return (
    <section>
      <h3 className="mb-2 text-sm font-semibold text-slate-700 dark:text-slate-300">角色指派</h3>
      <p className="mb-2 text-xs text-slate-500 dark:text-slate-400">
        每个 AI 操作对应一个模型。摘要必须指派，否则「总结」按钮不可用。
      </p>
      <ul className="space-y-2">
        {ROLE_LABELS.map(({ role, label, description }) => {
          const current = lookup.get(role) ?? '';
          return (
            <li
              key={role}
              className="flex items-center justify-between gap-3 rounded border border-slate-200 p-2 dark:border-slate-700"
            >
              <div className="min-w-0 flex-1 text-xs">
                <div className="font-medium text-slate-800 dark:text-slate-200">{label}</div>
                <div className="text-slate-500 dark:text-slate-400">{description}</div>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <select
                  value={current}
                  onChange={(e) => {
                    const next = e.currentTarget.value;
                    if (next === '') {
                      void clearRoleDefault(role);
                    } else {
                      void setRoleDefault(role, next);
                    }
                  }}
                  className="rounded border border-slate-300 bg-white px-2 py-1 text-xs dark:border-slate-600 dark:bg-slate-800"
                >
                  <option value="">— 未指派 —</option>
                  {models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.displayName}
                    </option>
                  ))}
                </select>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
