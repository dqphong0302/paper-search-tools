import { useCallback, useRef, useState } from 'react';
import { Paper } from '../types';
import { gatewayFetch } from '../lib/gateway';
import { CitationDirection, CitationState } from '../components/Explorer.shared';

/** Per-paper citation lists ("cited by" / "references"), fetched on demand. */
export function useCitations() {
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [citations, setCitations] = useState<Record<string, CitationState>>({});

  const load = useCallback(async (paper: Paper, direction: CitationDirection) => {
    setCitations((prev) => ({
      ...prev,
      [paper.id]: { direction, loading: true, error: null, items: prev[paper.id]?.items ?? [] },
    }));
    try {
      const res = await gatewayFetch(
        `/api/citations?id=${encodeURIComponent(paper.id)}&direction=${direction}&limit=15`
      );
      const data = await res.json().catch(() => null);
      if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
      setCitations((prev) => ({
        ...prev,
        [paper.id]: { direction, loading: false, error: null, items: Array.isArray(data?.items) ? data.items : [] },
      }));
    } catch (e) {
      setCitations((prev) => ({
        ...prev,
        [paper.id]: { direction, loading: false, error: (e as Error).message, items: [] },
      }));
    }
  }, []);

  // Read through refs so `toggle` keeps one identity; depending on the maps would
  // invalidate every card's `memo` whenever any one row loaded its citations.
  const citationsRef = useRef(citations);
  citationsRef.current = citations;
  const openRef = useRef(open);
  openRef.current = open;

  const toggle = useCallback((paper: Paper) => {
    const opening = !openRef.current[paper.id];
    setOpen((prev) => ({ ...prev, [paper.id]: opening }));
    if (opening && !citationsRef.current[paper.id]) void load(paper, 'cited_by');
  }, [load]);

  return { open, citations, load, toggle };
}
