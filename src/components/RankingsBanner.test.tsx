// @vitest-environment jsdom
import { act, ReactNode } from 'react';
import { createRoot, Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RankingsBanner } from './RankingsBanner';
import { rankingStatus, updateRankings } from '../lib/rankings';

vi.mock('../lib/rankings', () => ({ rankingStatus: vi.fn(), updateRankings: vi.fn(), importRankingsCsv: vi.fn() }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;
const render = async (node: ReactNode) => { await act(async () => { root.render(node); }); };

beforeEach(() => {
  localStorage.clear();
  host = document.createElement('div');
  document.body.appendChild(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.clearAllMocks();
});

describe('RankingsBanner', () => {
  it('offers to load rankings when none are loaded, then disappears', async () => {
    vi.mocked(rankingStatus).mockResolvedValue({ journals: 0, download_url: '' });
    vi.mocked(updateRankings).mockResolvedValue({ journals: 29000, download_url: '' });
    const onLoaded = vi.fn();
    await render(<RankingsBanner onLoaded={onLoaded} />);
    expect(host.textContent).toContain('See journal quartiles');
    await act(async () => { host.querySelector<HTMLButtonElement>('#rankings-banner-download')!.click(); });
    expect(onLoaded).toHaveBeenCalledOnce();
    expect(host.textContent).toBe('');
  });

  it('stays hidden when rankings exist or it was dismissed', async () => {
    vi.mocked(rankingStatus).mockResolvedValue({ journals: 10, download_url: '' });
    await render(<RankingsBanner />);
    expect(host.textContent).toBe('');

    localStorage.setItem('sg_rankings_banner_dismissed', '1');
    vi.mocked(rankingStatus).mockResolvedValue({ journals: 0, download_url: '' });
    await render(<RankingsBanner key="again" />);
    expect(host.textContent).toBe('');
    expect(rankingStatus).toHaveBeenCalledOnce();
  });
});
