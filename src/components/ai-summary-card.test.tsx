// src/components/ai-summary-card.test.tsx
import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { AiSummaryCard } from './ai-summary-card';
import { useAiStore } from '../lib/store/ai';
import { useMailStore } from '../lib/store/mail';
describe('AiSummaryCard', () => {
  beforeEach(() => {
    useAiStore.setState({
      summary: null,
      summarizing: false,
      models: [],
      roleDefaults: [],
    } as never);
    useMailStore.setState({
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: 'x', html: null, fetchedAt: '' },
    } as never);
  });
  it('有摘要渲染 tldr', () => {
    useAiStore.setState({
      summary: {
        tldr: '要点',
        bullets: [],
        language: 'zh',
        source: 'cached',
        model: 'm',
        inputTokens: null,
        outputTokens: null,
        cacheReadTokens: null,
      },
    } as never);
    render(<AiSummaryCard />);
    expect(screen.getByText('要点')).toBeInTheDocument();
  });
});
