import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../tauri', () => ({
  senderFiltersList: vi.fn(),
  senderFiltersAdd: vi.fn(),
  senderFiltersRemove: vi.fn(),
}));

import { senderFiltersAdd, senderFiltersList, senderFiltersRemove } from '../tauri';
import { useSenderFilters } from './sender-filters';

const reset = () => {
  useSenderFilters.setState({ filters: [], error: null });
};

describe('sender-filters store', () => {
  beforeEach(() => {
    reset();
    vi.clearAllMocks();
  });

  it('load 填充 filters', async () => {
    vi.mocked(senderFiltersList).mockResolvedValue([
      {
        id: '1',
        listType: 'black',
        matchType: 'domain',
        pattern: 'x.com',
        note: null,
        createdAt: 't',
      },
    ]);
    await useSenderFilters.getState().load();
    expect(useSenderFilters.getState().filters).toHaveLength(1);
  });

  it('add 成功后重拉、失败落 error', async () => {
    vi.mocked(senderFiltersAdd).mockResolvedValue({
      id: '2',
      listType: 'white',
      matchType: 'address',
      pattern: 'a@x.com',
      note: null,
      createdAt: 't',
    });
    vi.mocked(senderFiltersList).mockResolvedValue([]);
    await useSenderFilters.getState().add('white', 'a@x.com');
    expect(senderFiltersAdd).toHaveBeenCalledWith('white', 'a@x.com', undefined);

    vi.mocked(senderFiltersAdd).mockRejectedValue(new Error('该条目已在白名单中'));
    await useSenderFilters.getState().add('white', 'a@x.com');
    expect(useSenderFilters.getState().error).toContain('已在白名单中');
  });

  it('remove 失败时列表不变 + error 落位', async () => {
    useSenderFilters.setState({
      filters: [
        {
          id: '1',
          listType: 'black',
          matchType: 'domain',
          pattern: 'x.com',
          note: null,
          createdAt: 't',
        },
      ],
      error: null,
    });
    vi.mocked(senderFiltersRemove).mockRejectedValue(new Error('boom'));
    await useSenderFilters.getState().remove('1');
    expect(useSenderFilters.getState().filters).toHaveLength(1);
    expect(useSenderFilters.getState().error).toBeTruthy();
  });
});
