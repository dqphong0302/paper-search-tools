import React, { lazy, Suspense, useState, useEffect, useCallback, useRef } from 'react';
import { SideNav, TabType } from './components/layout/SideNav';
import { SearchPage } from './components/SearchPage';
const ResearchWorkspace = lazy(() => import('./components/ResearchWorkspace').then(m => ({ default: m.ResearchWorkspace })));
const AgentGateway = lazy(() => import('./components/AgentGateway').then(m => ({ default: m.AgentGateway })));
const SettingsPage = lazy(() => import('./components/SettingsPage').then(m => ({ default: m.SettingsPage })));
import { Paper, TelemetryStats, Workspace, WorkspacePaper, WorkspacePaperPatch } from './types';
import { Settings, RefreshCw, ChevronRight, WifiOff, Search, Plus, X } from 'lucide-react';
import { DEFAULT_GATEWAY_PORT, gatewayFetch, initGateway } from './lib/gateway';

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<TabType>('search');
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [explorerQuery, setExplorerQuery] = useState('');
  const [globalQuery, setGlobalQuery] = useState('');
  // Bumped on every "search this" action so navigating to the same query re-runs it.
  const [searchNonce, setSearchNonce] = useState(0);
  const [downloadCount, setDownloadCount] = useState(0);

  // ---- Research workspace (project) state --------------------------------
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string>(
    () => (typeof localStorage !== 'undefined' ? localStorage.getItem('sg_active_workspace') || '' : '')
  );
  const [workspaceData, setWorkspaceData] = useState<{ id: string; papers: WorkspacePaper[] }>({ id: '', papers: [] });
  const workspacePapers = workspaceData.id === activeWorkspaceId ? workspaceData.papers : [];
  const currentWorkspaceId = useRef(activeWorkspaceId);
  currentWorkspaceId.current = activeWorkspaceId;
  const workspaceRequestId = useRef(0);
  const [workspaceError, setWorkspaceError] = useState('');
  const [workspaceBusy, setWorkspaceBusy] = useState(false);
  const workspaceLock = useRef(false);

  // Modal dialog state for workspace create/rename/delete
  const [modalState, setModalState] = useState<
    | { type: 'create' }
    | { type: 'rename'; id: string; currentName: string }
    | { type: 'delete'; id: string; name: string }
    | null
  >(null);
  const [modalInput, setModalInput] = useState('');

  useEffect(() => {
    if (activeWorkspaceId) localStorage.setItem('sg_active_workspace', activeWorkspaceId);
  }, [activeWorkspaceId]);

  const loadWorkspaces = useCallback(async () => {
    try {
      const res = await gatewayFetch('/api/workspaces');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const list: Workspace[] = await res.json();
      setWorkspaces(list);
      setWorkspaceError('');
      setActiveWorkspaceId((current) =>
        current && list.some((w) => w.id === current) ? current : list[0]?.id ?? ''
      );
    } catch (e) {
      setWorkspaceError(`Unable to load workspaces: ${(e as Error).message}`);
    }
  }, []);

  const loadWorkspacePapers = useCallback(async (id: string) => {
    if (id !== currentWorkspaceId.current) return;
    const requestId = ++workspaceRequestId.current;
    if (!id) {
      setWorkspaceData({ id, papers: [] });
      return;
    }
    try {
      const res = await gatewayFetch(`/api/workspaces/${id}/papers`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const papers: WorkspacePaper[] = await res.json();
      if (id !== currentWorkspaceId.current || requestId !== workspaceRequestId.current) return;
      setWorkspaceData({ id, papers });
      setWorkspaceError('');
    } catch (e) {
      if (id !== currentWorkspaceId.current || requestId !== workspaceRequestId.current) return;
      setWorkspaceError(`Unable to load workspace papers: ${(e as Error).message}`);
    }
  }, []);

  const [telemetry, setTelemetry] = useState<TelemetryStats | null>(null);
  const [isGatewayOnline, setIsGatewayOnline] = useState<boolean | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  // Resolve the real port/token once, then poll. Polling before the port is
  // known would query 8795 even when a custom port is configured.
  const [port, setPort] = useState<number | null>(null);
  useEffect(() => {
    void initGateway().then(setPort);
  }, []);

  const fetchTelemetry = useCallback(async () => {
    if (port === null) return;
    try {
      const res = await gatewayFetch('/api/telemetry');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data: TelemetryStats = await res.json();
      setTelemetry(data);
      setIsGatewayOnline(true);
    } catch {
      setIsGatewayOnline(false);
    }
  }, [port]);

  const fetchDownloadsCount = useCallback(async () => {
    if (port === null) return;
    try {
      const res = await gatewayFetch('/api/history/downloads');
      if (res.ok) {
        const data = await res.json();
        if (Array.isArray(data)) setDownloadCount(data.length);
      }
    } catch {
      /* gateway offline — the banner already says so */
    }
  }, [port]);

  const handleManualRefresh = async () => {
    setIsRefreshing(true);
    await Promise.all([fetchTelemetry(), fetchDownloadsCount(), loadWorkspaces()]);
    setTimeout(() => setIsRefreshing(false), 400);
  };

  useEffect(() => {
    if (port === null) return;
    fetchTelemetry();
    fetchDownloadsCount();
    void loadWorkspaces();
    const interval = setInterval(() => {
      fetchTelemetry();
      fetchDownloadsCount();
    }, 10000);
    return () => clearInterval(interval);
  }, [port, fetchTelemetry, fetchDownloadsCount, loadWorkspaces]);

  useEffect(() => {
    if (port === null) return;
    void loadWorkspacePapers(activeWorkspaceId);
    return () => { ++workspaceRequestId.current; };
  }, [port, activeWorkspaceId, loadWorkspacePapers]);

  // ---- Workspace mutations ------------------------------------------------

  const handleCreateWorkspace = () => {
    setModalInput('');
    setModalState({ type: 'create' });
  };

  const handleRenameWorkspace = (id: string, currentName: string) => {
    setModalInput(currentName);
    setModalState({ type: 'rename', id, currentName });
  };

  const handleDeleteWorkspace = (id: string, name: string) => {
    if (workspaces.length <= 1) {
      setWorkspaceError('At least one workspace must be retained.');
      return;
    }
    setModalState({ type: 'delete', id, name });
  };

  const handleModalSubmit = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!modalState) return;

    if (modalState.type === 'create') {
      const name = modalInput.trim();
      if (!name) return;
      try {
        const res = await gatewayFetch('/api/workspaces', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ name }),
        });
        const data = await res.json().catch(() => null);
        if (!res.ok || !data?.id) throw new Error(data?.error || `HTTP ${res.status}`);
        await loadWorkspaces();
        setActiveWorkspaceId(data.id);
        setModalState(null);
      } catch (e) {
        setWorkspaceError(`Unable to create workspace: ${(e as Error).message}`);
      }
    } else if (modalState.type === 'rename') {
      const name = modalInput.trim();
      if (!name || name === modalState.currentName) {
        setModalState(null);
        return;
      }
      try {
        const res = await gatewayFetch(`/api/workspaces/${modalState.id}`, {
          method: 'PATCH',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ name }),
        });
        const data = await res.json().catch(() => null);
        if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
        await loadWorkspaces();
        setModalState(null);
      } catch (e) {
        setWorkspaceError(`Unable to rename workspace: ${(e as Error).message}`);
      }
    } else if (modalState.type === 'delete') {
      try {
        const res = await gatewayFetch(`/api/workspaces/${modalState.id}`, { method: 'DELETE' });
        const data = await res.json().catch(() => null);
        if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
        if (activeWorkspaceId === modalState.id) setActiveWorkspaceId('');
        await loadWorkspaces();
        setModalState(null);
      } catch (e) {
        setWorkspaceError(`Unable to delete workspace: ${(e as Error).message}`);
      }
    }
  };

  const isSaved = (paperId: string) => workspacePapers.some((wp) => wp.paper.id === paperId);

  const mutateWorkspacePaper = async (paper: Paper | null, id: string) => {
    if (!activeWorkspaceId) {
      setWorkspaceError('Please select or create a workspace before saving papers.');
      return;
    }
    if (workspaceLock.current) return;
    workspaceLock.current = true;
    setWorkspaceBusy(true);
    setWorkspaceError('');
    try {
      const endpoint = `/api/workspaces/${activeWorkspaceId}/papers?paper_id=${encodeURIComponent(id)}`;
      if (paper) {
        const res = await gatewayFetch(`/api/workspaces/${activeWorkspaceId}/papers`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ paper }),
        });
        const data = await res.json().catch(() => null);
        if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
      } else {
        const res = await gatewayFetch(endpoint, { method: 'DELETE' });
        const data = await res.json().catch(() => null);
        if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
      }
      await loadWorkspacePapers(activeWorkspaceId);
      await loadWorkspaces();
    } catch (error) {
      setWorkspaceError(`Failed to update workspace: ${String(error)}`);
    } finally {
      workspaceLock.current = false;
      setWorkspaceBusy(false);
    }
  };

  const handleSavePaper = (paper: Paper) => {
    void mutateWorkspacePaper(isSaved(paper.id) ? null : paper, paper.id);
  };
  const handleRemovePaper = (id: string) => { void mutateWorkspacePaper(null, id); };

  const handleUpdatePaper = async (id: string, patch: WorkspacePaperPatch) => {
    if (!activeWorkspaceId) return;
    try {
      const res = await gatewayFetch(
        `/api/workspaces/${activeWorkspaceId}/papers?paper_id=${encodeURIComponent(id)}`,
        { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(patch) }
      );
      const data = await res.json().catch(() => null);
      if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
      if (currentWorkspaceId.current !== activeWorkspaceId) return;
      setWorkspaceData((prev) => prev.id !== activeWorkspaceId ? prev : {
        ...prev,
        papers: prev.papers.map((wp) => (wp.paper.id === id ? { ...wp, ...patch } : wp)),
      });
    } catch (e) {
      setWorkspaceError(`Unable to save changes: ${(e as Error).message}`);
    }
  };

  const handleNavigateToExplorer = (query: string) => {
    const q = query.trim();
    if (!q) return;
    setExplorerQuery(q);
    setGlobalQuery(q);
    setSearchNonce((n) => n + 1);
    setActiveTab('search');
  };

  const handleShowOverview = () => {
    setExplorerQuery('');
    setGlobalQuery('');
    setSearchNonce(0);
  };

  // ⌘K / Ctrl+K jumps straight to the shell search box
  const globalInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        globalInputRef.current?.focus();
        globalInputRef.current?.select();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const savedIds = new Set(workspacePapers.map((wp) => wp.paper.id));
  const offline = isGatewayOnline === false;
  const activePort = port ?? DEFAULT_GATEWAY_PORT;
  const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId);

  const tabTitles: Record<TabType, string> = {
    search: 'Search',
    research: 'Library',
    gateway: 'Connections',
    settings: 'Settings',
  };

  return (
    <div className="cockpit-layout">
      <SideNav
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        isCollapsed={isCollapsed}
        setIsCollapsed={setIsCollapsed}
        savedCount={workspacePapers.length}
        downloadCount={downloadCount}
      />

      <div className="cockpit-stage">
        <header className="cockpit-top-bar">
          <div className="cockpit-breadcrumb">
            <span className="breadcrumb-root">ScholarGateway</span>
            <ChevronRight size={14} style={{ color: 'var(--text-dim)' }} />
            <h1 className="breadcrumb-current">{tabTitles[activeTab]}</h1>
          </div>

          <form
            className="search-omnibox shell-omnibox"
            onSubmit={(e) => {
              e.preventDefault();
              handleNavigateToExplorer(globalQuery);
            }}
          >
            <Search size={16} style={{ color: 'var(--primary-cyan)', marginLeft: 4 }} />
            <input
              ref={globalInputRef}
              id="global-search-input"
              className="search-input"
              type="text"
              placeholder="Search papers, authors, DOI… (⌘K)"
              value={globalQuery}
              onChange={(e) => setGlobalQuery(e.target.value)}
              aria-label="Global search"
            />
            <button
              id="global-search-submit"
              type="submit"
              className="search-submit-btn"
              disabled={!globalQuery.trim()}
            >
              <span>Search</span>
            </button>
          </form>

          <div className="top-bar-actions">
            <select
              id="workspace-switcher"
              className="field-input"
              aria-label="Active workspace"
              value={activeWorkspaceId}
              onChange={(e) => setActiveWorkspaceId(e.target.value)}
              style={{ width: 'auto', maxWidth: 190 }}
            >
              {workspaces.length === 0 && <option value="">(No workspace)</option>}
              {workspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name} ({w.paper_count})
                </option>
              ))}
            </select>
            <button
              id="workspace-create"
              className="action-btn"
              onClick={handleCreateWorkspace}
              title="Create new research workspace"
              aria-label="Create new research workspace"
              style={{ padding: '6px 10px' }}
            >
              <Plus size={14} />
            </button>

            <div
              className="cockpit-status-pill"
              title={
                offline
                  ? 'Disconnected from local gateway'
                  : 'Local gateway active (REST + MCP)'
              }
              style={{
                color: offline ? 'var(--status-rose)' : 'var(--status-emerald)',
                background: offline ? 'var(--status-rose-bg)' : 'var(--status-emerald-bg)',
                borderColor: offline ? 'var(--status-rose-border)' : 'var(--status-emerald-border)',
              }}
            >
              <span
                className="pulse-dot"
                style={{
                  background: offline ? 'var(--status-rose)' : 'var(--status-emerald)',
                  boxShadow: `0 0 8px ${offline ? 'var(--status-rose)' : 'var(--status-emerald)'}`,
                }}
              />
              <span>
                127.0.0.1:{activePort} {isGatewayOnline === null ? '...' : offline ? 'OFFLINE' : 'ONLINE'}
              </span>
            </div>

            <button
              id="refresh-gateway-status"
              className="action-btn"
              onClick={handleManualRefresh}
              title="Refresh system status"
              aria-label="Refresh system status"
              style={{ padding: '6px 10px' }}
            >
              <RefreshCw size={14} className={isRefreshing ? 'animate-spin' : ''} />
            </button>

            <button
              id="open-settings"
              className={`action-btn ${activeTab === 'settings' ? 'action-btn-primary' : ''}`}
              onClick={() => setActiveTab('settings')}
              title="Settings & API Keys"
              aria-label="Settings & API Keys"
              style={{ padding: '6px 10px' }}
            >
              <Settings size={14} />
            </button>
          </div>
        </header>

        <main className="cockpit-scroll-body">
          {workspaceError && <div className="alert alert-warning" role="alert">{workspaceError}</div>}
          {workspaceBusy && <div role="status">Updating workspace…</div>}
          {offline && (
            <div className="page-container" style={{ marginBottom: 16 }}>
              <div className="alert alert-danger">
                <WifiOff size={16} style={{ flexShrink: 0, marginTop: 1 }} />
                <div>
                  <div className="alert-title">Gateway Connection Lost (127.0.0.1:{activePort})</div>
                  <div>
                    Paper discovery, PDF downloads, and workspace sync will resume once the local gateway is reachable.
                  </div>
                </div>
                <button className="action-btn" onClick={handleManualRefresh} style={{ marginLeft: 'auto' }}>
                  <RefreshCw size={13} className={isRefreshing ? 'animate-spin' : ''} />
                  <span>Retry</span>
                </button>
              </div>
            </div>
          )}

          <Suspense fallback={<div role="status">Loading…</div>}>
          {activeTab === 'search' && (
            <SearchPage
              onSavePaper={handleSavePaper}
              savedPaperIds={savedIds}
              initialQuery={explorerQuery}
              draftQuery={globalQuery}
              searchNonce={searchNonce}
              port={activePort}
              telemetry={telemetry}
              isOnline={isGatewayOnline === true}
              onNavigateToExplorer={handleNavigateToExplorer}
              onShowOverview={handleShowOverview}
              workspaceId={activeWorkspaceId}
            />
          )}

          {activeTab === 'research' && (
            <ResearchWorkspace
              workspace={activeWorkspace}
              workspacePapers={workspacePapers}
              onRemovePaper={handleRemovePaper}
              onUpdatePaper={handleUpdatePaper}
              onRenameWorkspace={handleRenameWorkspace}
              onDeleteWorkspace={handleDeleteWorkspace}
              onRerunSearch={handleNavigateToExplorer}
              port={activePort}
            />
          )}

          {activeTab === 'gateway' && (
            <AgentGateway
              telemetry={telemetry}
              isOnline={isGatewayOnline === true}
              onRefresh={fetchTelemetry}
              port={activePort}
            />
          )}

          {activeTab === 'settings' && <SettingsPage port={activePort} />}
          </Suspense>
        </main>
      </div>

      {modalState && (
        <div className="modal-overlay" onClick={() => setModalState(null)}>
          <div className="modal-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <div className="modal-title">
                {modalState.type === 'create' && 'New Research Workspace'}
                {modalState.type === 'rename' && 'Rename Workspace'}
                {modalState.type === 'delete' && 'Delete Workspace'}
              </div>
              <button
                className="modal-close-btn"
                onClick={() => setModalState(null)}
                aria-label="Close dialog"
              >
                <X size={16} />
              </button>
            </div>
            <form onSubmit={handleModalSubmit}>
              <div className="modal-body">
                {modalState.type === 'delete' ? (
                  <p style={{ fontSize: '13px', color: 'var(--text-muted)', lineHeight: 1.5 }}>
                    Are you sure you want to delete workspace <strong>"{modalState.name}"</strong>?
                    Papers will remain in the shared database and will only be removed from this workspace.
                  </p>
                ) : (
                  <div>
                    <label
                      htmlFor="workspace-name-input"
                      style={{
                        display: 'block',
                        fontSize: '12px',
                        fontWeight: 600,
                        color: 'var(--text-muted)',
                        marginBottom: 6,
                      }}
                    >
                      Workspace Name
                    </label>
                    <input
                      id="workspace-name-input"
                      type="text"
                      className="field-input"
                      style={{ width: '100%' }}
                      placeholder="e.g., Type 2 Diabetes Clinical Trials"
                      value={modalInput}
                      onChange={(e) => setModalInput(e.target.value)}
                      autoFocus
                    />
                  </div>
                )}
              </div>
              <div className="modal-footer">
                <button
                  type="button"
                  className="action-btn"
                  onClick={() => setModalState(null)}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="action-btn action-btn-primary"
                  style={modalState.type === 'delete' ? { background: 'var(--status-rose)', color: '#fff', borderColor: 'var(--status-rose)' } : {}}
                  disabled={modalState.type !== 'delete' && !modalInput.trim()}
                >
                  {modalState.type === 'create' && 'Create'}
                  {modalState.type === 'rename' && 'Save'}
                  {modalState.type === 'delete' && 'Delete'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};

export default App;
