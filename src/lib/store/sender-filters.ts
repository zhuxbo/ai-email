import { create } from 'zustand';

import { senderFiltersAdd, senderFiltersList, senderFiltersRemove } from '../tauri';
import type { SenderFilter } from '../types';
import { errMsg } from '../utils';

interface SenderFiltersState {
  filters: SenderFilter[];
  error: string | null;
  load: () => Promise<void>;
  add: (listType: 'black' | 'white', value: string, note?: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

export const useSenderFilters = create<SenderFiltersState>((set, get) => ({
  filters: [],
  error: null,
  load: async () => {
    try {
      set({ filters: await senderFiltersList(), error: null });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },
  add: async (listType, value, note) => {
    try {
      await senderFiltersAdd(listType, value, note);
      await get().load();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },
  remove: async (id) => {
    try {
      await senderFiltersRemove(id);
      await get().load();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },
}));
