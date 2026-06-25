import { create } from 'zustand';
import * as tauri from '../tauri';
import type {
  AiModel,
  AiRole,
  AddModelForm,
  UpdateModelForm,
  RoleDefault,
  SummaryResult,
  TranslateResult,
} from '../types';
import { errMsg } from '../utils';
interface AiState {
  models: AiModel[];
  roleDefaults: RoleDefault[];
  summary: SummaryResult | null;
  translation: TranslateResult | null;
  summarizing: boolean;
  translating: boolean;
  summarizingFor: string | null;
  translatingFor: string | null;
  error: string | null;
  loadAiConfig: () => Promise<void>;
  summarize: (messageId: string) => Promise<void>;
  translate: (messageId: string, target: string) => Promise<void>;
  clearTranslation: () => void;
  resetForMessage: (messageId: string) => void;
  addModel: (f: AddModelForm) => Promise<AiModel>;
  removeModel: (id: string) => Promise<void>;
  updateModel: (id: string, f: UpdateModelForm) => Promise<AiModel>;
  setRoleDefault: (r: AiRole, m: string) => Promise<void>;
  clearRoleDefault: (r: AiRole) => Promise<void>;
  clearError: () => void;
}
export const useAiStore = create<AiState>((set, get) => ({
  models: [],
  roleDefaults: [],
  summary: null,
  translation: null,
  summarizing: false,
  translating: false,
  summarizingFor: null,
  translatingFor: null,
  error: null,
  loadAiConfig: async () => {
    try {
      const [models, roleDefaults] = await Promise.all([
        tauri.modelsList(),
        tauri.roleDefaultsList(),
      ]);
      set({ models, roleDefaults });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },
  summarize: async (messageId) => {
    set({ summarizing: true, summarizingFor: messageId, error: null });
    try {
      const summary = await tauri.aiSummarize(messageId);
      if (get().summarizingFor === messageId) set({ summary });
    } catch (e) {
      if (get().summarizingFor === messageId) set({ error: errMsg(e) });
    } finally {
      if (get().summarizingFor === messageId) set({ summarizing: false });
    }
  },
  translate: async (messageId, target) => {
    set({ translating: true, translatingFor: messageId, error: null });
    try {
      const translation = await tauri.aiTranslate(messageId, target);
      if (get().translatingFor === messageId) set({ translation });
    } catch (e) {
      if (get().translatingFor === messageId) set({ error: errMsg(e) });
    } finally {
      if (get().translatingFor === messageId) set({ translating: false });
    }
  },
  clearTranslation: () => {
    set({ translation: null, translatingFor: null });
  },
  resetForMessage: (_messageId) => {
    set({
      summary: null,
      translation: null,
      summarizing: false,
      translating: false,
      summarizingFor: null,
      translatingFor: null,
    });
  },
  addModel: async (form) => {
    try {
      const model = await tauri.modelAdd(form);
      set((s) => ({ models: [...s.models, model] }));
      return model;
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    }
  },
  removeModel: async (id) => {
    try {
      await tauri.modelRemove(id);
      set((s) => ({ models: s.models.filter((m) => m.id !== id) }));
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    }
  },
  updateModel: async (id, form) => {
    try {
      const model = await tauri.modelUpdate(id, form);
      set((s) => ({ models: s.models.map((m) => (m.id === id ? model : m)) }));
      return model;
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    }
  },
  setRoleDefault: async (role, modelId) => {
    try {
      await tauri.roleDefaultSet(role, modelId);
      set((s) => ({
        roleDefaults: [...s.roleDefaults.filter((r) => r.role !== role), { role, modelId }],
      }));
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    }
  },
  clearRoleDefault: async (role) => {
    try {
      await tauri.roleDefaultClear(role);
      set((s) => ({ roleDefaults: s.roleDefaults.filter((r) => r.role !== role) }));
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },
  clearError: () => {
    set({ error: null });
  },
}));
