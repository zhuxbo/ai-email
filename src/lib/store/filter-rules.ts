// AI 过滤规则 store：写后重拉（仿 auto-reply）。全局规则（无 accountId）。

import { create } from 'zustand';

import * as tauri from '../tauri';
import type { FilterRule, FilterRuleInput } from '../types';
import { errMsg } from '../utils';

interface FilterRulesState {
  rules: FilterRule[];
  loading: boolean;
  error: string | null;
  loadRules: () => Promise<void>;
  addRule: (input: FilterRuleInput) => Promise<void>;
  updateRule: (rule: FilterRule) => Promise<void>;
  removeRule: (id: string) => Promise<void>;
  toggleRule: (id: string, enabled: boolean) => Promise<void>;
  clearError: () => void;
}

export const useFilterRulesStore = create<FilterRulesState>((set, get) => ({
  rules: [],
  loading: false,
  error: null,

  loadRules: async () => {
    set({ loading: true, error: null });
    try {
      const rules = await tauri.filterRulesList();
      set({ rules });
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ loading: false });
    }
  },

  addRule: async (input) => {
    set({ error: null });
    try {
      await tauri.filterRuleAdd(input);
      await get().loadRules();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  updateRule: async (rule) => {
    set({ error: null });
    try {
      await tauri.filterRuleUpdate(rule);
      await get().loadRules();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  removeRule: async (id) => {
    set({ error: null });
    try {
      await tauri.filterRuleRemove(id);
      await get().loadRules();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  toggleRule: async (id, enabled) => {
    set({ error: null });
    try {
      await tauri.filterRuleSetEnabled(id, enabled);
      await get().loadRules();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  clearError: () => {
    set({ error: null });
  },
}));
