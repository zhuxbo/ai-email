// src/components/message-actions.test.tsx
import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MessageActions } from './message-actions';
import { useMailStore } from '../lib/store/mail';
import { useAiStore } from '../lib/store/ai';
describe('MessageActions', () => {
  beforeEach(() => {
    useMailStore.setState({
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: 'x', html: null, fetchedAt: '' },
    } as never);
    useAiStore.setState({
      translation: null,
      translating: false,
      models: [],
      roleDefaults: [],
    } as never);
  });
  it('AI 写为占位（disabled + P3）', () => {
    render(<MessageActions />);
    const b = screen.getByRole('button', { name: /AI 写/ });
    expect(b).toBeDisabled();
    expect(b.getAttribute('title')).toContain('P3');
  });
});
