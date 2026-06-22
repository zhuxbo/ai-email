// src/components/message-actions.test.tsx
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MessageActions } from './message-actions';
import { useMailStore } from '../lib/store/mail';
import { useComposeStore } from '../lib/store/compose';
import { useUiStore } from '../lib/store/ui';

describe('MessageActions', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    useMailStore.setState({
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: 'x', html: null, fetchedAt: '' },
      messages: [
        {
          id: 'm1',
          accountId: 'a1',
          subject: 's',
          fromAddr: 'x',
          toAddrs: [],
          ccAddrs: [],
          sentAt: null,
          snippet: null,
          flags: [],
        } as never,
      ],
    } as never);
    useUiStore.setState({ drawerOpen: false, drawerTab: 'summary' } as never);
  });

  it('回复打开 compose tab', async () => {
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '回复' }));
    expect(useUiStore.getState().drawerTab).toBe('compose');
    expect(useUiStore.getState().drawerOpen).toBe(true);
    expect(useComposeStore.getState().replyContext?.messageId).toBe('m1');
  });

  it('翻译打开 translate tab', async () => {
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '翻译' }));
    expect(useUiStore.getState().drawerTab).toBe('translate');
    expect(useUiStore.getState().drawerOpen).toBe(true);
  });

  it('摘要打开 summary tab', async () => {
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '摘要' }));
    expect(useUiStore.getState().drawerTab).toBe('summary');
    expect(useUiStore.getState().drawerOpen).toBe(true);
  });

  it('删除走 confirm + deleteMessage', async () => {
    vi.stubGlobal('confirm', () => true);
    const del = vi.fn();
    useMailStore.setState({
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: 'x', html: null, fetchedAt: '' },
      messages: [{ id: 'm1', accountId: 'a1', flags: [] } as never],
      deleteMessage: del,
    } as never);
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '删除' }));
    expect(del).toHaveBeenCalledWith('m1');
  });

  it('加星按未加星态调 setFlagged(id,true)', async () => {
    const setFlagged = vi.fn();
    useMailStore.setState({
      selectedMessageId: 'm1',
      body: null,
      messages: [{ id: 'm1', accountId: 'a1', flags: [] } as never],
      setFlagged,
    } as never);
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '加星' }));
    expect(setFlagged).toHaveBeenCalledWith('m1', true);
  });

  it('不再有归档按钮', () => {
    useMailStore.setState({
      selectedMessageId: 'm1',
      body: null,
      messages: [{ id: 'm1', accountId: 'a1', flags: [] } as never],
    } as never);
    render(<MessageActions />);
    expect(screen.queryByRole('button', { name: '归档' })).toBeNull();
  });

  it('confirm 取消时不调 deleteMessage', async () => {
    vi.stubGlobal('confirm', () => false);
    const del = vi.fn();
    useMailStore.setState({
      selectedMessageId: 'm1',
      body: null,
      messages: [{ id: 'm1', accountId: 'a1', flags: [] } as never],
      deleteMessage: del,
    } as never);
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '删除' }));
    expect(del).not.toHaveBeenCalled();
  });

  it('已读态点标记未读调 setSeen(id,false)', async () => {
    const setSeen = vi.fn();
    useMailStore.setState({
      selectedMessageId: 'm1',
      body: null,
      messages: [{ id: 'm1', accountId: 'a1', flags: ['\\Seen'] } as never],
      setSeen,
    } as never);
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '标记未读' }));
    expect(setSeen).toHaveBeenCalledWith('m1', false);
  });
});
