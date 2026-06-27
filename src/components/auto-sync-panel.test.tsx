import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { AutoSyncPanel } from './auto-sync-panel';
import { useMailStore } from '../lib/store/mail';

beforeEach(() => {
  useMailStore.setState({
    autoSyncIntervalMin: 5,
    lastSyncAt: null,
    accountErrors: {},
    accounts: [],
    setAutoSyncInterval: vi.fn(),
  } as never);
});

describe('AutoSyncPanel', () => {
  it('改间隔下拉调 setAutoSyncInterval（传数字）', () => {
    const spy = vi.fn();
    useMailStore.setState({ setAutoSyncInterval: spy } as never);
    render(<AutoSyncPanel />);
    fireEvent.change(screen.getByRole('combobox'), { target: { value: '15' } });
    expect(spy).toHaveBeenCalledWith(15);
  });

  it('lastSyncAt=null 显示尚未同步', () => {
    render(<AutoSyncPanel />);
    expect(screen.getByText(/尚未同步/)).toBeInTheDocument();
  });
});
