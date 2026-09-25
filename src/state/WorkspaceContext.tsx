import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { Workspace } from '../types';
import { gatewayFetch } from '../lib/gateway';

/** The built-in library the desktop UI saves "Interest" papers into (see db.rs). */
export const INTEREST_LIBRARY_ID = '__interest_library__';
const ACTIVE_KEY = 'sg_active_workspace';

export interface WorkspaceOption {
  id: string;
  name: string;
  isLibrary: boolean;
  paperCount?: number;
}

const LIBRARY: WorkspaceOption = { id: INTEREST_LIBRARY_ID, name: 'Interest Library', isLibrary: true };

interface WorkspaceContextValue {
  workspaces: WorkspaceOption[];
  active: WorkspaceOption;
  /** Scope for search/download history; the library keeps the unscoped, app-wide history. */
  scopeId?: string;
  select: (id: string) => void;
  create: (name: string) => Promise<void>;
  rename: (id: string, name: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  reload: () => Promise<void>;
}

const noop = async () => {};
const WorkspaceContext = createContext<WorkspaceContextValue>({
  workspaces: [LIBRARY],
  active: LIBRARY,
  select: () => {},
  create: noop,
  rename: noop,
  remove: noop,
  reload: noop,
});

function readActive(): string {
  try {
    return localStorage.getItem(ACTIVE_KEY) || INTEREST_LIBRARY_ID;
  } catch {
    return INTEREST_LIBRARY_ID;
  }
}

function writeActive(id: string) {
  try {
    localStorage.setItem(ACTIVE_KEY, id);
  } catch {
    /* storage unavailable — the choice lasts for this session only */
  }
}

async function send(path: string, init: RequestInit): Promise<unknown> {
  const res = await gatewayFetch(path, {
    ...init,
    headers: { 'Content-Type': 'application/json', ...init.headers },
  });
  const data = await res.json().catch(() => null);
  if (!res.ok) throw new Error((data as { error?: string } | null)?.error || `HTTP ${res.status}`);
  return data;
}

/**
 * Project workspaces live in the gateway (REST + MCP). The desktop UI shows the
 * built-in Interest Library first, then every project an agent or the user made.
 */
export const WorkspaceProvider: React.FC<{ ready: boolean; children: React.ReactNode }> = ({ ready, children }) => {
  const [projects, setProjects] = useState<WorkspaceOption[]>([]);
  const [activeId, setActiveId] = useState(readActive);

  const reload = useCallback(async () => {
    try {
      const res = await gatewayFetch('/api/workspaces');
      const data = await res.json().catch(() => null);
      if (!res.ok || !Array.isArray(data)) return;
      setProjects((data as Workspace[]).map((w) => ({ id: w.id, name: w.name, isLibrary: false, paperCount: w.paper_count })));
    } catch {
      /* Offline or locked: the library stays available on its own. */
    }
  }, []);

  useEffect(() => {
    if (ready) void reload();
  }, [ready, reload]);

  const select = useCallback((id: string) => {
    setActiveId(id);
    writeActive(id);
  }, []);

  const create = useCallback(async (name: string) => {
    const created = (await send('/api/workspaces', { method: 'POST', body: JSON.stringify({ name }) })) as Workspace;
    await reload();
    if (created?.id) select(created.id);
  }, [reload, select]);

  const rename = useCallback(async (id: string, name: string) => {
    await send(`/api/workspaces/${encodeURIComponent(id)}`, { method: 'PATCH', body: JSON.stringify({ name }) });
    await reload();
  }, [reload]);

  const remove = useCallback(async (id: string) => {
    await send(`/api/workspaces/${encodeURIComponent(id)}`, { method: 'DELETE' });
    if (id === activeId) select(INTEREST_LIBRARY_ID);
    await reload();
  }, [activeId, reload, select]);

  const value = useMemo<WorkspaceContextValue>(() => {
    const workspaces = [LIBRARY, ...projects];
    // A stored id can outlive its workspace (deleted by an agent); fall back to the library.
    const active = workspaces.find((w) => w.id === activeId) ?? LIBRARY;
    return {
      workspaces,
      active,
      scopeId: active.isLibrary ? undefined : active.id,
      select,
      create,
      rename,
      remove,
      reload,
    };
  }, [projects, activeId, select, create, rename, remove, reload]);

  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
};

export function useWorkspace(): WorkspaceContextValue {
  return useContext(WorkspaceContext);
}
