import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SummaryTab } from './summary-tab';
import { useMailStore } from '../lib/store/mail';
import { useAiStore } from '../lib/store/ai';

describe('SummaryTab', () => {
  beforeEach(() => {
    useAiStore.setState({
      summary: null,
      summarizing: false,
      models: [],
      roleDefaults: [],
    } as never);
    useMailStore.setState({ selectedMessageId: null, body: null } as never);
  });
  it('无选中邮件显示引导', () => {
    render(<SummaryTab />);
    expect(screen.getByText(/选一封邮件/)).toBeInTheDocument();
  });
});
