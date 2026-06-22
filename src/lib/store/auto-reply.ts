// 自动回复 store：规则（按编辑账户）+ 建议回复队列（聚合）。
// dismiss 乐观移除 + 失败回滚，仿 mail store setFlagOptimistic。
// 「去回复」不在本 store（跨 compose/ui store），由队列组件接线。

import { create } from 'zustand';

import * as tauri from '../tauri';
import type { AutoReplyRule, AutoReplyRuleInput, SuggestedReply } from '../types';
import { errMsg } from '../utils';

interface AutoReplyState {
  rules: AutoReplyRule[];
  rulesAccountId: string | null;
  queue: SuggestedReply[];
  loadingRules: boolean;
  loadingQueue: boolean;
  error: string | null;

  loadRules: (accountId: string) => Promise<void>;
  addRule: (input: AutoReplyRuleInput) => Promise<void>;
  updateRule: (rule: AutoReplyRule) => Promise<void>;
  removeRule: (id: string) => Promise<void>;
  toggleRule: (id: string, enabled: boolean) => Promise<void>;

  loadQueue: () => Promise<void>;
  dismiss: (id: string) => Promise<void>;
  clearError: () => void;
}

export const useAutoReplyStore = create<AutoReplyState>((set, get) => ({
  rules: [],
  rulesAccountId: null,
  queue: [],
  loadingRules: false,
  loadingQueue: false,
  error: null,

  loadRules: async (accountId) => {
    set({ loadingRules: true, rulesAccountId: accountId, error: null });
    try {
      const rules = await tauri.autoReplyRulesList(accountId);
      if (get().rulesAccountId === accountId) set({ rules });
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      if (get().rulesAccountId === accountId) set({ loadingRules: false });
    }
  },

  addRule: async (input) => {
    set({ error: null });
    try {
      await tauri.autoReplyRuleAdd(input);
      await get().loadRules(input.accountId);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  updateRule: async (rule) => {
    set({ error: null });
    try {
      await tauri.autoReplyRuleUpdate(rule);
      await get().loadRules(rule.accountId);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  removeRule: async (id) => {
    const accountId = get().rulesAccountId;
    set({ error: null });
    try {
      await tauri.autoReplyRuleRemove(id);
      if (accountId !== null) await get().loadRules(accountId);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  toggleRule: async (id, enabled) => {
    const accountId = get().rulesAccountId;
    set({ error: null });
    try {
      await tauri.autoReplyRuleSetEnabled(id, enabled);
      if (accountId !== null) await get().loadRules(accountId);
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  loadQueue: async () => {
    set({ loadingQueue: true });
    try {
      const queue = await tauri.suggestedRepliesList();
      set({ queue, error: null });
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ loadingQueue: false });
    }
  },

  dismiss: async (id) => {
    // 乐观移除并清旧错误。失败时不还原陈旧整列快照（会复活并发 dismiss 的其它已移除项），
    // 改为从后端重拉权威队列、再写 error（loadQueue 成功路径会清 error，故 error 后置），
    // 仿 mail.deleteMessage 的失败重载惯例。
    set({ queue: get().queue.filter((s) => s.id !== id), error: null });
    try {
      await tauri.suggestedReplyDismiss(id);
    } catch (e) {
      await get().loadQueue();
      set({ error: errMsg(e) });
    }
  },

  clearError: () => {
    set({ error: null });
  },
}));
