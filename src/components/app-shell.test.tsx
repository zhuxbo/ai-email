import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AppShell } from './app-shell';
import { useUiStore } from '../lib/store/ui';
import { useMailStore } from '../lib/store/mail';

// mock mail store：compose 依赖 mail store，tauri mock 避免报错
vi.mock('../lib/store/compose', () => ({
  useComposeStore: {
    getState: () => ({ reset: vi.fn(), openBlank: vi.fn(), openReply: vi.fn() }),
  },
}));
vi.mock('../lib/tauri', () => ({
  accountsList: vi.fn().mockResolvedValue([]),
  unifiedInbox: vi.fn().mockResolvedValue({ messages: [], errors: {} }),
  messageBody: vi.fn().mockResolvedValue(null),
  mailboxesList: vi.fn().mockResolvedValue([]),
  messagesList: vi.fn().mockResolvedValue([]),
  aiClassify: vi.fn().mockResolvedValue([]),
  accountRemove: vi.fn().mockResolvedValue(undefined),
  messageSetSeen: vi.fn().mockResolvedValue(undefined),
  messageSetFlagged: vi.fn().mockResolvedValue(undefined),
  messageDelete: vi.fn().mockResolvedValue(undefined),
  inboxSync: vi.fn().mockResolvedValue({ newMessageCount: 0, totalInMailbox: 0 }),
}));

vi.mock('../lib/hooks/use-breakpoint', () => ({ useBreakpoint: () => 'mobile' }));

const noop = vi.fn();
const navProps = {
  accounts: [],
  selectedAccountId: null,
  mailboxes: [],
  selectedMailboxId: null,
  syncing: false,
  onSelectAccount: noop,
  onSelectMailbox: noop,
  onAddAccount: noop,
  onSync: noop,
  onOpenSettings: noop,
  onOpenAutoReply: noop,
  autoReplyCount: 0,
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
    useMailStore.setState({ selectedMessageId: 'm1' } as never);
    const { rerender } = render(shell(0));
    expect(screen.getByText('LIST')).toBeInTheDocument();
    expect(screen.queryByText('DETAIL')).not.toBeInTheDocument();

    rerender(shell(1));
    expect(screen.getByText('DETAIL')).toBeInTheDocument();
  });

  it('back returns to list; re-opening the same message (seq increments) re-enters detail', async () => {
    useUiStore.setState({ mobileView: 'list', drawerOpen: false });
    useMailStore.setState({ selectedMessageId: 'm1' } as never);
    const { rerender } = render(shell(1));
    expect(screen.getByText('DETAIL')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: '返回列表' }));
    expect(screen.getByText('LIST')).toBeInTheDocument();

    // 重选同一封邮件：selectMessage 仍推进 messageOpenSeq → 再次进详情（B1 回归）。
    rerender(shell(2));
    expect(screen.getByText('DETAIL')).toBeInTheDocument();
  });

  it('#15 选中邮件被删除（selectedMessageId 变 null）→ 移动端自动回列表视图', () => {
    // 先进入 detail 视图
    useUiStore.setState({ mobileView: 'list', drawerOpen: false });
    useMailStore.setState({ selectedMessageId: 'm1' } as never);
    const { rerender } = render(shell(1));
    expect(screen.getByText('DETAIL')).toBeInTheDocument();

    // 模拟邮件被删除：selectedMessageId 变 null
    act(() => {
      useMailStore.setState({ selectedMessageId: null } as never);
    });
    // 触发 rerender 使 effect 有机会运行
    rerender(shell(1));

    // 移动端应自动回到列表视图
    expect(screen.getByText('LIST')).toBeInTheDocument();
    expect(screen.queryByText('DETAIL')).not.toBeInTheDocument();
  });

  it('#15 切筛选（selectedMessageId 变 null）→ 移动端自动回列表', () => {
    useUiStore.setState({ mobileView: 'detail', drawerOpen: false });
    useMailStore.setState({ selectedMessageId: 'm1' } as never);
    render(shell(1));
    expect(screen.getByText('DETAIL')).toBeInTheDocument();

    // 切筛选导致 selectedMessageId=null
    act(() => {
      useMailStore.setState({ selectedMessageId: null } as never);
    });

    expect(screen.getByText('LIST')).toBeInTheDocument();
  });
});
