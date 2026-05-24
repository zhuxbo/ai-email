// Provider presets used to pre-fill the add-model form. Most domestic vendors expose the
// OpenAI /v1/chat/completions schema — they differ only in base URL and the model_id they
// accept. The user can override `modelId` if they want a different SKU from the vendor.

import type { AiProvider } from './types';

export interface AiPreset {
  /** Stable slug used by the UI dropdown. */
  id: string;
  /** What the user sees in the dropdown. */
  label: string;
  provider: AiProvider;
  /** null = use the provider's default base URL on the Rust side. */
  baseUrl: string | null;
  /** Default model id; user can override. */
  modelId: string;
  /** Short hint shown under the form. */
  hint: string;
  /** Default `displayName` value when the user picks this preset (still editable). */
  defaultDisplayName: string;
}

const ANTHROPIC: AiPreset = {
  id: 'anthropic',
  label: 'Anthropic Claude',
  provider: 'anthropic',
  baseUrl: null,
  modelId: 'claude-sonnet-4-6',
  defaultDisplayName: 'Claude Sonnet 4.6',
  hint: 'Anthropic 原生接口 · 在 console.anthropic.com 获取 sk-ant-* 密钥',
};

const OPENAI: AiPreset = {
  id: 'openai',
  label: 'OpenAI',
  provider: 'openai',
  baseUrl: null,
  modelId: 'gpt-4o',
  defaultDisplayName: 'GPT-4o',
  hint: 'OpenAI 官方 · 在 platform.openai.com 获取 sk-* 密钥',
};

const DEEPSEEK: AiPreset = {
  id: 'deepseek',
  label: 'DeepSeek',
  provider: 'openai',
  baseUrl: 'https://api.deepseek.com',
  modelId: 'deepseek-chat',
  defaultDisplayName: 'DeepSeek V3',
  hint: 'DeepSeek 兼容 OpenAI 接口 · platform.deepseek.com 获取密钥',
};

const ZHIPU: AiPreset = {
  id: 'zhipu',
  label: '智谱 GLM',
  provider: 'openai',
  baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
  modelId: 'glm-4-flash',
  defaultDisplayName: '智谱 GLM-4-Flash',
  hint: '智谱 AI 兼容 OpenAI 接口 · bigmodel.cn 获取 API Key',
};

const MOONSHOT: AiPreset = {
  id: 'moonshot',
  label: 'Moonshot Kimi',
  provider: 'openai',
  baseUrl: 'https://api.moonshot.cn/v1',
  modelId: 'moonshot-v1-8k',
  defaultDisplayName: 'Kimi V1 8K',
  hint: 'Moonshot 兼容 OpenAI 接口 · platform.moonshot.cn 获取密钥',
};

const QWEN: AiPreset = {
  id: 'qwen',
  label: '通义千问',
  provider: 'openai',
  baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
  modelId: 'qwen-turbo',
  defaultDisplayName: '通义千问 Turbo',
  hint: '阿里云通义 · DashScope 兼容模式 · dashscope.aliyun.com 获取 sk-*',
};

const CUSTOM: AiPreset = {
  id: 'custom',
  label: '自定义 OpenAI 兼容',
  provider: 'openai',
  baseUrl: '',
  modelId: '',
  defaultDisplayName: '',
  hint: '任何提供 /v1/chat/completions 接口的服务（Groq / Together / Ollama 等）',
};

export const PRESETS: AiPreset[] = [ANTHROPIC, OPENAI, DEEPSEEK, ZHIPU, MOONSHOT, QWEN, CUSTOM];

export function presetById(id: string): AiPreset {
  return PRESETS.find((p) => p.id === id) ?? ANTHROPIC;
}
