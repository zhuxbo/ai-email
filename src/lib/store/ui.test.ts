import { describe, it, expect, beforeEach } from 'vitest';
import { useUiStore, applyTheme } from './ui';

beforeEach(() => {
  useUiStore.setState({
    theme: 'light',
    drawerOpen: false,
    drawerTab: 'summary',
    mobileView: 'list',
  });
  document.documentElement.classList.remove('dark');
  localStorage.clear();
});

describe('ui store', () => {
  it('toggles theme, reflects on <html>, persists to localStorage', () => {
    useUiStore.getState().toggleTheme();
    expect(useUiStore.getState().theme).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(localStorage.getItem('ai-email-theme')).toBe('dark');
    useUiStore.getState().toggleTheme();
    expect(document.documentElement.classList.contains('dark')).toBe(false);
    expect(localStorage.getItem('ai-email-theme')).toBe('light');
  });

  it('opens drawer on a specific tab', () => {
    useUiStore.getState().openDrawer('compose');
    expect(useUiStore.getState().drawerOpen).toBe(true);
    expect(useUiStore.getState().drawerTab).toBe('compose');
  });

  it('switches mobile view', () => {
    useUiStore.getState().setMobileView('detail');
    expect(useUiStore.getState().mobileView).toBe('detail');
  });

  it('applyTheme toggles the html dark class', () => {
    applyTheme('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    applyTheme('light');
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });
});
