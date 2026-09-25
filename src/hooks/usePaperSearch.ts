import { useCallback, useEffect, useRef, useState } from 'react';
import { SearchResponse } from '../types';
import { gatewayFetch, getGatewayPort } from '../lib/gateway';

export interface SearchRequest {
  query: string;
  limit?: number;
  year_min?: number;
  year_max?: number;
  open_access_only: boolean;
  sources?: string[];
  workspace_id?: string;
}

const PAGE_SIZE = 15;
const MAX_OFFSET = 10_000;

async function postSearch(body: string): Promise<SearchResponse> {
  const res = await gatewayFetch('/api/search', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body,
  });
  if (!res.ok) throw new Error(`Gateway returned error code ${res.status}`);
  return res.json();
}

/**
 * One search and its "Load more" pages. Every request carries an id; a response
 * that arrives after a newer search started (or after unmount) is dropped.
 */
export function usePaperSearch() {
  const [results, setResults] = useState<SearchResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const latestId = useRef(0);
  const completed = useRef<{ id: number; body: string; offset: number } | null>(null);
  const loadingMoreId = useRef<number | null>(null);

  useEffect(() => () => { ++latestId.current; }, []);

  const run = useCallback(async (request: SearchRequest): Promise<SearchResponse | null> => {
    const id = ++latestId.current;
    completed.current = null;
    loadingMoreId.current = null;
    setLoadingMore(false);
    setResults(null);
    setLoading(true);
    setError(null);
    try {
      const body = JSON.stringify(request);
      const data = await postSearch(body);
      if (id !== latestId.current) return null;
      completed.current = { id, body, offset: data.papers.length };
      setResults(data);
      return data;
    } catch (err) {
      if (id !== latestId.current) return null;
      setResults(null);
      setError(
        err instanceof TypeError
          ? `Could not reach the gateway at 127.0.0.1:${getGatewayPort()}. Check that it is running.`
          : (err as Error).message || 'Search failed.'
      );
      return null;
    } finally {
      if (id === latestId.current) setLoading(false);
    }
  }, []);

  const reset = useCallback(() => {
    ++latestId.current;
    completed.current = null;
    setResults(null);
    setError(null);
    setLoading(false);
  }, []);

  const loadMore = useCallback(async () => {
    const search = completed.current;
    if (!search || search.id !== latestId.current || loadingMoreId.current !== null) return;
    const id = search.id;
    loadingMoreId.current = id;
    setLoadingMore(true);
    setError(null);
    try {
      const data = await postSearch(JSON.stringify({ ...JSON.parse(search.body), limit: PAGE_SIZE, offset: search.offset }));
      if (id !== latestId.current) return;
      search.offset += data.papers.length;
      setResults((prev) => {
        if (!prev || id !== latestId.current) return prev;
        const seen = new Set(prev.papers.map((p) => p.id));
        const merged = [...prev.papers, ...data.papers.filter((p) => !seen.has(p.id))];
        return {
          ...prev,
          papers: merged,
          total: merged.length,
          available_total: data.papers.length ? data.available_total ?? prev.available_total : merged.length,
          sources: data.sources ?? prev.sources,
        };
      });
    } catch (e) {
      if (id !== latestId.current) return;
      setError((e as Error).message || 'Could not load more results.');
    } finally {
      if (id === latestId.current) {
        loadingMoreId.current = null;
        setLoadingMore(false);
      }
    }
  }, []);

  const availableTotal = results?.available_total ?? results?.total ?? 0;
  const loadedOffset = completed.current?.offset;
  const canLoadMore =
    !!results &&
    availableTotal > (loadedOffset ?? results.papers.length) &&
    (loadedOffset ?? 0) < MAX_OFFSET;

  return { results, loading, loadingMore, error, setError, run, reset, loadMore, availableTotal, canLoadMore };
}
