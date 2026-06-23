import { create } from 'zustand';

/** DB 初始化状态机：loading → ready | error（单向，终态不可逆）*/
export type DbStatus = 'loading' | 'ready' | 'error';

interface DbState {
  status: DbStatus;
  errorMessage: string | null;

  setReady: () => void;
  setError: (message: string) => void;
}

export const useDbStore = create<DbState>((set, get) => ({
  status: 'loading',
  errorMessage: null,

  // 状态机只前进：已处于终态（ready/error）时忽略重复信号，防止事件与查询竞争时倒退。
  setReady: () => {
    if (get().status === 'loading') {
      set({ status: 'ready', errorMessage: null });
    }
  },
  setError: (message) => {
    if (get().status === 'loading') {
      set({ status: 'error', errorMessage: message });
    }
  },
}));
