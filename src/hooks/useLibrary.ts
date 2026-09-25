import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Paper, WorkspacePaper, WorkspacePaperPatch } from '../types';
import { gatewayFetch } from '../lib/gateway';
import { INTEREST_LIBRARY_ID } from '../state/WorkspaceContext';

/** The library keeps its dedicated endpoint; project workspaces use the workspace API. */
function papersPath(workspaceId: string, paperId?: string): string {
  const base = workspaceId === INTEREST_LIBRARY_ID
    ? '/api/library'
    : `/api/workspaces/${encodeURIComponent(workspaceId)}/papers`;
  return paperId === undefined ? base : `${base}?paper_id=${encodeURIComponent(paperId)}`;
}

async function request(path: string, init?: RequestInit): Promise<unknown> {
  const res = await gatewayFetch(path, init);
  const data = await res.json().catch(() => null);
  if (!res.ok) throw new Error((data as { error?: string } | null)?.error || `HTTP ${res.status}`);
  return data;
}

/**
 * Papers saved in one workspace, with the mutations the UI needs. Stale responses
 * (an older load, or a load for the workspace the user just left) are discarded.
 */
export function useLibrary(workspaceId: string, ready: boolean) {
  const [papers, setPapers] = useState<WorkspacePaper[]>([]);
  const [error, setError] = useState('');
  const requestId = useRef(0);
  // Per-paper guard: a second click on the same paper waits, other papers do not.
  const pending = useRef(new Set<string>());

  const load = useCallback(async () => {
    const id = ++requestId.current;
    try {
      const data = await request(papersPath(workspaceId));
      if (!Array.isArray(data)) throw new Error('Unexpected response');
      if (id !== requestId.current) return;
      setPapers(data);
      setError('');
    } catch (err) {
      if (id !== requestId.current) return;
      setError(`Could not load the interest list: ${(err as Error).message}`);
    }
  }, [workspaceId]);

  useEffect(() => {
    if (!ready) return;
    setPapers([]);
    void load();
  }, [ready, load]);

  const savedIds = useMemo(() => new Set(papers.map((item) => item.paper.id)), [papers]);

  const mutate = useCallback(async (paperId: string, optimistic: (list: WorkspacePaper[]) => WorkspacePaper[], init: RequestInit, path: string) => {
    if (pending.current.has(paperId)) return;
    pending.current.add(paperId);
    setError('');
    setPapers(optimistic);
    try {
      await request(path, init);
    } catch (err) {
      setError(`Could not update the interest list: ${(err as Error).message}`);
    } finally {
      pending.current.delete(paperId);
      await load();
    }
  }, [load]);

  const remove = useCallback((paperId: string) => mutate(
    paperId,
    (list) => list.filter((item) => item.paper.id !== paperId),
    { method: 'DELETE' },
    papersPath(workspaceId, paperId),
  ), [mutate, workspaceId]);

  const toggle = useCallback((paper: Paper) => {
    if (savedIds.has(paper.id)) return remove(paper.id);
    return mutate(
      paper.id,
      (list) => [{ paper, added_at: Math.floor(Date.now() / 1000) }, ...list],
      { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ paper }) },
      papersPath(workspaceId),
    );
  }, [mutate, remove, savedIds, workspaceId]);

  const update = useCallback(async (paperId: string, patch: WorkspacePaperPatch) => {
    try {
      await request(papersPath(workspaceId, paperId), {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(patch),
      });
      setPapers((current) => current.map((item) => (item.paper.id === paperId ? { ...item, ...patch } : item)));
    } catch (err) {
      setError(`Could not save your changes: ${(err as Error).message}`);
    }
  }, [workspaceId]);

  return { papers, savedIds, error, load, toggle, remove, update };
}
