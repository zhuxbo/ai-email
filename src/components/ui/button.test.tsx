import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Button } from './button';

describe('Button', () => {
  it('renders label and fires onClick', async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>发送</Button>);
    await userEvent.click(screen.getByRole('button', { name: '发送' }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it('does not fire when disabled', async () => {
    const onClick = vi.fn();
    render(
      <Button onClick={onClick} disabled>
        发送
      </Button>,
    );
    await userEvent.click(screen.getByRole('button', { name: '发送' }));
    expect(onClick).not.toHaveBeenCalled();
  });
});
