import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { TranslationView } from './translation-view';
import type { TranslateResult } from '../lib/types';
const t: TranslateResult = {
  target: 'zh-CN',
  subject: '标题',
  body: '正文',
  source: 'fresh',
  model: 'm',
  inputTokens: 10,
  outputTokens: 5,
  cacheReadTokens: 0,
};
describe('TranslationView', () => {
  it('渲染译文', () => {
    render(<TranslationView translation={t} />);
    expect(screen.getByText('标题')).toBeInTheDocument();
    expect(screen.getByText('正文')).toBeInTheDocument();
  });
});
