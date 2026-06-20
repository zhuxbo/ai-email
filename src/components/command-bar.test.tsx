import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CommandBar } from './command-bar';
import { useUiStore } from '../lib/store/ui';

beforeEach(() => {
  useUiStore.setState({ theme: 'light' });
  document.documentElement.classList.remove('dark');
});

describe('CommandBar', () => {
  it('reports query changes', async () => {
    const onQuery = vi.fn();
    render(<CommandBar onQueryChange={onQuery} onAiCommand={vi.fn()} />);
    await userEvent.type(screen.getByPlaceholderText(/搜索全部账户/), 'hi');
    expect(onQuery).toHaveBeenLastCalledWith('hi');
  });

  it('toggles theme via button', async () => {
    render(<CommandBar onQueryChange={vi.fn()} onAiCommand={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: '切换主题' }));
    expect(useUiStore.getState().theme).toBe('dark');
  });
});
