import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';

import { ErrorToasts } from './error-toasts';

describe('ErrorToasts', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('3 秒后自动调用 clear（自动消失）', () => {
    const clear = vi.fn();
    render(<ErrorToasts toasts={[{ key: 'mail', text: '同步失败', clear }]} />);
    expect(screen.getByText('同步失败')).toBeInTheDocument();
    expect(clear).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(clear).toHaveBeenCalledTimes(1);
  });

  it('3 秒前不消失', () => {
    const clear = vi.fn();
    render(<ErrorToasts toasts={[{ key: 'mail', text: 'X', clear }]} />);
    act(() => {
      vi.advanceTimersByTime(2999);
    });
    expect(clear).not.toHaveBeenCalled();
  });

  it('父组件重渲染（clear 闭包换新）不重置 3s 计时，且调用最新 clear', () => {
    const clear1 = vi.fn();
    const { rerender } = render(
      <ErrorToasts toasts={[{ key: 'mail', text: 'X', clear: clear1 }]} />,
    );
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    // 同 key/text 重渲染，但 clear 换新闭包（模拟 App.tsx 内联箭头每渲染新标识）
    const clear2 = vi.fn();
    rerender(<ErrorToasts toasts={[{ key: 'mail', text: 'X', clear: clear2 }]} />);
    act(() => {
      vi.advanceTimersByTime(1500); // 累计 3000ms
    });
    // 计时未因重渲染重置（否则累计 3s 时距重渲染仅 1.5s、不触发）；ref 取最新 → 调 clear2 非 clear1
    expect(clear2).toHaveBeenCalledTimes(1);
    expect(clear1).not.toHaveBeenCalled();
  });

  it('点击 × 立即 clear', () => {
    const clear = vi.fn();
    render(<ErrorToasts toasts={[{ key: 'mail', text: 'X', clear }]} />);
    fireEvent.click(screen.getByLabelText('关闭错误提示'));
    expect(clear).toHaveBeenCalledTimes(1);
  });

  it('toast 被移除后清理计时器，不再调用 clear', () => {
    const clear = vi.fn();
    const { rerender } = render(<ErrorToasts toasts={[{ key: 'mail', text: 'X', clear }]} />);
    rerender(<ErrorToasts toasts={[]} />);
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(clear).not.toHaveBeenCalled();
  });

  it('多路错误各自成条、各自独立计时', () => {
    const clearMail = vi.fn();
    const clearAi = vi.fn();
    render(
      <ErrorToasts
        toasts={[
          { key: 'mail', text: '邮件错误', clear: clearMail },
          { key: 'ai', text: 'AI 错误', clear: clearAi },
        ]}
      />,
    );
    expect(screen.getByText('邮件错误')).toBeInTheDocument();
    expect(screen.getByText('AI 错误')).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(clearMail).toHaveBeenCalledTimes(1);
    expect(clearAi).toHaveBeenCalledTimes(1);
  });

  it('空数组不渲染容器', () => {
    const { container } = render(<ErrorToasts toasts={[]} />);
    expect(container.firstChild).toBeNull();
  });
});
