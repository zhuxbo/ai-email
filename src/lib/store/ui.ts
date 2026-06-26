import { create } from 'zustand';

export type Theme = 'light' | 'dark';
export type DrawerTab = 'summary' | 'translate' | 'compose' | 'filter';
export type MobileView = 'list' | 'detail';

const STORAGE_KEY = 'ai-email-theme';

function initialTheme(): Theme {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved === 'light' || saved === 'dark') return saved;
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function applyTheme(theme: Theme): void {
  document.documentElement.classList.toggle('dark', theme === 'dark');
}

interface UiState {
  theme: Theme;
  drawerOpen: boolean;
  drawerTab: DrawerTab;
  mobileView: MobileView;

  toggleTheme: () => void;
  openDrawer: (tab: DrawerTab) => void;
  closeDrawer: () => void;
  setMobileView: (v: MobileView) => void;
}

export const useUiStore = create<UiState>((set, get) => ({
  theme: initialTheme(),
  drawerOpen: false,
  drawerTab: 'summary',
  mobileView: 'list',

  toggleTheme: () => {
    const theme: Theme = get().theme === 'light' ? 'dark' : 'light';
    applyTheme(theme);
    localStorage.setItem(STORAGE_KEY, theme);
    set({ theme });
  },
  openDrawer: (tab) => {
    set({ drawerOpen: true, drawerTab: tab });
  },
  closeDrawer: () => {
    set({ drawerOpen: false });
  },
  setMobileView: (v) => {
    set({ mobileView: v });
  },
}));
