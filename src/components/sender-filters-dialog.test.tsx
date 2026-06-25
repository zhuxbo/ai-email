import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useSenderFilters } from '../lib/store/sender-filters';
import { SenderFiltersPanel } from './sender-filters-dialog';

describe('SenderFiltersPanel', () => {
  beforeEach(() => {
    useSenderFilters.setState({ filters: [], error: null });
    vi.restoreAllMocks();
  });

  it('空态显示提示', () => {
    render(<SenderFiltersPanel />);
    expect(screen.getAllByText(/暂无/).length).toBeGreaterThan(0);
  });

  it('黑区添加调 add(black, value)', async () => {
    const add = vi.fn().mockResolvedValue(undefined);
    useSenderFilters.setState({ add } as never);
    render(<SenderFiltersPanel />);
    const input = screen.getByPlaceholderText(/黑名单/);
    fireEvent.change(input, { target: { value: '@spam.com' } });
    fireEvent.click(screen.getByRole('button', { name: /加入黑名单/ }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledWith('black', '@spam.com');
    });
  });

  it('显示校验错误', () => {
    useSenderFilters.setState({ error: '域名格式非法' });
    render(<SenderFiltersPanel />);
    expect(screen.getByText(/域名格式非法/)).toBeInTheDocument();
  });
});
