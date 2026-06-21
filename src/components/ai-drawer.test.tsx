import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { AiDrawer } from './ai-drawer';
import { useUiStore } from '../lib/store/ui';
import { useMailStore } from '../lib/store/mail';
import { useAiStore } from '../lib/store/ai';
import { useComposeStore } from '../lib/store/compose';

describe('AiDrawer', () => {
  beforeEach(() => {
    useAiStore.setState({
      summary: null,
      translation: null,
      models: [],
      roleDefaults: [],
    } as never);
    useMailStore.setState({ selectedMessageId: null, body: null, accounts: [] } as never);
    useComposeStore.getState().openBlank();
  });
  it('drawerTab=summary 渲染摘要 tab', () => {
    useUiStore.setState({ drawerTab: 'summary' } as never);
    render(<AiDrawer />);
    expect(screen.getByRole('button', { name: '摘要' })).toBeInTheDocument();
  });
  it('drawerTab=compose 渲染写信 tab', () => {
    useUiStore.setState({ drawerTab: 'compose' } as never);
    render(<AiDrawer />);
    expect(screen.getByRole('button', { name: '发送' })).toBeInTheDocument();
  });
});
