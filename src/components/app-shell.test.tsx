import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AppShell } from './app-shell';
import { useUiStore } from '../lib/store/ui';

vi.mock('../lib/hooks/use-breakpoint', () => ({ useBreakpoint: () => 'mobile' }));

const noop = vi.fn();
const navProps = {
  accounts: [],
  selectedAccountId: null,
  syncing: false,
  onSelectAccount: noop,
  onAddAccount: noop,
  onSync: noop,
  onRemoveAccount: noop,
  onOpenSettings: noop,
  onOpenAutoReply: noop,
};

function shell(messageOpenSeq: number) {
  return (
    <AppShell
      nav={navProps}
      onQueryChange={noop}
      messageOpenSeq={messageOpenSeq}
      list={<div>LIST</div>}
      detail={<div>DETAIL</div>}
      drawer={<div>DRAWER</div>}
    />
  );
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('AppShell (mobile)', () => {
  it('shows list at seq 0, enters detail when a message is opened (seq increments)', () => {
    useUiStore.setState({ mobileView: 'list', drawerOpen: false });
    const { rerender } = render(shell(0));
    expect(screen.getByText('LIST')).toBeInTheDocument();
    expect(screen.queryByText('DETAIL')).not.toBeInTheDocument();

    rerender(shell(1));
    expect(screen.getByText('DETAIL')).toBeInTheDocument();
  });

  it('back returns to list; re-opening the same message (seq increments) re-enters detail', async () => {
    useUiStore.setState({ mobileView: 'list', drawerOpen: false });
    const { rerender } = render(shell(1));
    expect(screen.getByText('DETAIL')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: '返回列表' }));
    expect(screen.getByText('LIST')).toBeInTheDocument();

    // 重选同一封邮件：selectMessage 仍推进 messageOpenSeq → 再次进详情（B1 回归）。
    rerender(shell(2));
    expect(screen.getByText('DETAIL')).toBeInTheDocument();
  });
});
