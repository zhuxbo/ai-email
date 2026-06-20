import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

const bpMock = vi.fn(() => 'desktop');
vi.mock('../../lib/hooks/use-breakpoint', () => ({ useBreakpoint: () => bpMock() }));

import { Drawer } from './drawer';

beforeEach(() => {
  bpMock.mockReturnValue('desktop');
});

describe('Drawer', () => {
  it('desktop: renders side panel, no overlay', () => {
    render(
      <Drawer open onClose={vi.fn()}>
        <p>内容</p>
      </Drawer>,
    );
    expect(screen.getByText('内容')).toBeInTheDocument();
    expect(screen.queryByRole('presentation')).not.toBeInTheDocument();
  });

  it('mobile: clicking overlay fires onClose', async () => {
    bpMock.mockReturnValue('mobile');
    const onClose = vi.fn();
    render(
      <Drawer open onClose={onClose}>
        <p>内容</p>
      </Drawer>,
    );
    await userEvent.click(screen.getByRole('presentation'));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('renders nothing when closed', () => {
    render(
      <Drawer open={false} onClose={vi.fn()}>
        <p>内容</p>
      </Drawer>,
    );
    expect(screen.queryByText('内容')).not.toBeInTheDocument();
  });
});
