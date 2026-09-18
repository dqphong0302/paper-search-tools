import React, { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react';
import { SideNav, TabType } from './components/layout/SideNav';
import { SearchPage } from './components/SearchPage';
const ResearchLibrary = lazy(() => import('./components/ResearchWorkspace').then((m) => ({ default: m.ResearchLibrary })));
const AgentGateway = lazy(() => import('./components/AgentGateway').then((m) => ({ default: m.AgentGateway })));
const SettingsPage = lazy(() => import('./components/SettingsPage').then((m) => ({ default: m.SettingsPage })));
import { Paper, TelemetryStats, WorkspacePaper, WorkspacePaperPatch } from './types';
import { ChevronRight, RefreshCw, Search, Settings, WifiOff } from 'lucide-react';
import { DEFAULT_GATEWAY_PORT, gatewayFetch, initGateway } from './lib/gateway';
import { readSettings, saveSettings, SettingsAuthRequired } from './lib/settings';
import { TopicSetupModal } from './components/TopicSetupModal';
import { isTauri } from '@tauri-apps/api/core';

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<TabType>('search');
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [explorerQuery, setExplorerQuery] = useState('');
  const [globalQuery, setGlobalQuery] = useState('');
  const [searchNonce, setSearchNonce] = useState(0);
  const [downloadCount, setDownloadCount] = useState(0);
  const [libraryPapers, setLibraryPapers] = useState<WorkspacePaper[]>([]);
  const [libraryError, setLibraryError] = useState('');
  const [libraryBusy, setLibraryBusy] = useState(false);
  const libraryLock = useRef(false);
  const libraryRequestId = useRef(0);

  const [telemetry, setTelemetry] = useState<TelemetryStats | null>(null);
  const [isGatewayOnline, setIsGatewayOnline] = useState<boolean | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [port, setPort] = useState<number | null>(null);
  const [topicSetupRequired, setTopicSetupRequired] = useState(false);
  const [defaultTopic, setDefaultTopic] = useState('default');

  useEffect(() => {
    void initGateway().then(setPort);
  }, []);

  useEffect(() => {
    if (!isTauri()) return;

    const timer = window.setTimeout(() => {
      void (async () => {
        let accepted = false;
        try {
          const { check } = await import('@tauri-apps/plugin-updater');
          const update = await check({ timeout: 30_000 });
          if (!update) return;

          accepted = window.confirm(
            `ScholarGate ${update.version} is available. Download and install it now?`
          );
          if (!accepted) {
            await update.close();
            return;
          }

          await update.downloadAndInstall();
          const { relaunch } = await import('@tauri-apps/plugin-process');
          await relaunch();
        } catch (error) {
          console.error('Automatic update check failed', error);
          if (accepted) {
            window.alert('ScholarGate could not install the update. Please try again later.');
          }
        }
      })();
    }, 3_000);

    return () => window.clearTimeout(timer);
  }, []);

  useEffect(() => {
    if (port === null) return;
    void readSettings()
      .then((settings) => {
        setDefaultTopic(settings.domain_preset || 'default');
        setTopicSetupRequired(settings.topic_setup_completed !== 'true');
      })
      .catch((error) => {
        // A gateway that wants a token is not an error worth a banner here:
        // the Settings page explains it, and the first-run modal must not open
        // on top of a locked gateway.
        if (error instanceof SettingsAuthRequired) return;
      });
  }, [port]);

  const completeTopicSetup = async (preset: string, sources: string[]) => {
    await saveSettings({
      domain_preset: preset,
      enabled_sources: sources.join(','),
      topic_setup_completed: 'true',
    });
    setDefaultTopic(preset);
    setTopicSetupRequired(false);
  };

  const loadLibrary = useCallback(async () => {
    const requestId = ++libraryRequestId.current;
    try {
      const res = await gatewayFetch('/api/library');
      const data = await res.json().catch(() => null);
      if (!res.ok || !Array.isArray(data)) throw new Error(data?.error || `HTTP ${res.status}`);
      if (requestId !== libraryRequestId.current) return;
      setLibraryPapers(data);
      setLibraryError('');
    } catch (error) {
      if (requestId !== libraryRequestId.current) return;
      setLibraryError(`Could not load the interest list: ${(error as Error).message}`);
    }
  }, []);

  const fetchTelemetry = useCallback(async () => {
    if (port === null) return;
    try {
      const res = await gatewayFetch('/api/telemetry');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      setTelemetry(await res.json());
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
      // The offline banner reports the gateway failure.
    }
  }, [port]);

  const handleManualRefresh = async () => {
    setIsRefreshing(true);
    await Promise.all([fetchTelemetry(), fetchDownloadsCount(), loadLibrary()]);
    setTimeout(() => setIsRefreshing(false), 400);
  };

  useEffect(() => {
    if (port === null) return;
    void fetchTelemetry();
    void fetchDownloadsCount();
    void loadLibrary();

    // Closing the window hides it to the tray rather than quitting, so this
    // poll used to keep running — and keep waking the gateway — for a window
    // nobody was looking at. Poll only while the UI is actually visible, and
    // refresh once immediately on becoming visible so nothing looks stale.
    let interval: number | undefined;
    const stop = () => {
      if (interval !== undefined) window.clearInterval(interval);
      interval = undefined;
    };
    const start = () => {
      if (interval !== undefined) return;
      interval = window.setInterval(() => {
        void fetchTelemetry();
        void fetchDownloadsCount();
      }, 10_000);
    };
    const onVisibilityChange = () => {
      if (document.hidden) {
        stop();
        return;
      }
      void fetchTelemetry();
      void fetchDownloadsCount();
      start();
    };

    if (!document.hidden) start();
    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => {
      stop();
      document.removeEventListener('visibilitychange', onVisibilityChange);
    };
  }, [port, fetchTelemetry, fetchDownloadsCount, loadLibrary]);

  // Memoised: this Set is handed down to every result row, and a fresh identity
  // on each render would defeat the rows' `memo`.
  const interestedIds = React.useMemo(
    () => new Set(libraryPapers.map((item) => item.paper.id)),
    [libraryPapers]
  );

  const mutateInterest = async (paper: Paper | null, id: string) => {
    if (libraryLock.current) return;
    libraryLock.current = true;
    setLibraryBusy(true);
    setLibraryError('');
    try {
      const endpoint = `/api/library?paper_id=${encodeURIComponent(id)}`;
      const res = paper
        ? await gatewayFetch('/api/library', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ paper }),
          })
        : await gatewayFetch(endpoint, { method: 'DELETE' });
      const data = await res.json().catch(() => null);
      if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
      await loadLibrary();
    } catch (error) {
      setLibraryError(`Could not update the interest list: ${(error as Error).message}`);
    } finally {
      libraryLock.current = false;
      setLibraryBusy(false);
    }
  };

  const handleToggleInterest = (paper: Paper) => {
    void mutateInterest(interestedIds.has(paper.id) ? null : paper, paper.id);
  };

  const handleRemovePaper = (id: string) => {
    void mutateInterest(null, id);
  };

  const handleUpdatePaper = async (id: string, patch: WorkspacePaperPatch) => {
    try {
      const res = await gatewayFetch(`/api/library?paper_id=${encodeURIComponent(id)}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(patch),
      });
      const data = await res.json().catch(() => null);
      if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
      setLibraryPapers((current) => current.map((item) => (
        item.paper.id === id ? { ...item, ...patch } : item
      )));
    } catch (error) {
      setLibraryError(`Could not save your changes: ${(error as Error).message}`);
    }
  };

  const handleNavigateToExplorer = (query: string) => {
    const next = query.trim();
    if (!next) return;
    setExplorerQuery(next);
    setGlobalQuery(next);
    setSearchNonce((value) => value + 1);
    setActiveTab('search');
  };

  const handleShowOverview = () => {
    setExplorerQuery('');
    setGlobalQuery('');
    setSearchNonce(0);
  };

  const globalInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        globalInputRef.current?.focus();
        globalInputRef.current?.select();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const offline = isGatewayOnline === false;
  const activePort = port ?? DEFAULT_GATEWAY_PORT;
  const tabTitles: Record<TabType, string> = {
    search: 'Search',
    research: 'Library',
    gateway: 'Connections',
    settings: 'Settings',
  };

  return (
    <div className="cockpit-layout">
      {topicSetupRequired && <TopicSetupModal onComplete={completeTopicSetup} />}
      <SideNav
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        isCollapsed={isCollapsed}
        setIsCollapsed={setIsCollapsed}
        savedCount={libraryPapers.length}
        downloadCount={downloadCount}
      />

      <div className="cockpit-stage">
        <header className="cockpit-top-bar">
          <div className="cockpit-breadcrumb">
            <span className="breadcrumb-root">ScholarGate</span>
            <ChevronRight size={14} style={{ color: 'var(--text-dim)' }} />
            <h1 className="breadcrumb-current">{tabTitles[activeTab]}</h1>
          </div>

          <form className="search-omnibox shell-omnibox" onSubmit={(event) => { event.preventDefault(); handleNavigateToExplorer(globalQuery); }}>
            <Search size={16} style={{ color: 'var(--primary-cyan)', marginLeft: 4 }} />
            <input ref={globalInputRef} id="global-search-input" className="search-input" type="text" placeholder="Search papers, authors, DOI… (⌘K)" value={globalQuery} onChange={(event) => setGlobalQuery(event.target.value)} aria-label="Global search" />
            <button id="global-search-submit" type="submit" className="search-submit-btn" disabled={!globalQuery.trim()}><span>Search</span></button>
          </form>

          <div className="top-bar-actions">
            <div className="cockpit-status-pill" title={offline ? 'Disconnected from local gateway' : 'Local gateway active (REST + MCP)'} style={{ color: offline ? 'var(--status-rose)' : 'var(--status-emerald)', background: offline ? 'var(--status-rose-bg)' : 'var(--status-emerald-bg)', borderColor: offline ? 'var(--status-rose-border)' : 'var(--status-emerald-border)' }}>
              <span className="pulse-dot" style={{ background: offline ? 'var(--status-rose)' : 'var(--status-emerald)', boxShadow: `0 0 8px ${offline ? 'var(--status-rose)' : 'var(--status-emerald)'}` }} />
              <span>127.0.0.1:{activePort} {isGatewayOnline === null ? '...' : offline ? 'OFFLINE' : 'ONLINE'}</span>
            </div>
            <button id="refresh-gateway-status" className="action-btn" onClick={handleManualRefresh} title="Refresh system status" aria-label="Refresh system status" style={{ padding: '6px 10px' }}><RefreshCw size={14} className={isRefreshing ? 'animate-spin' : ''} /></button>
            <button id="open-settings" className={`action-btn ${activeTab === 'settings' ? 'action-btn-primary' : ''}`} onClick={() => setActiveTab('settings')} title="Settings & API Keys" aria-label="Settings & API Keys" style={{ padding: '6px 10px' }}><Settings size={14} /></button>
          </div>
        </header>

        <main className="cockpit-scroll-body">
          {libraryError && <div className="alert alert-warning" role="alert">{libraryError}</div>}
          {libraryBusy && <div role="status">Updating the interest list…</div>}
          {offline && (
            <div className="page-container" style={{ marginBottom: 16 }}>
              <div className="alert alert-danger">
                <WifiOff size={16} style={{ flexShrink: 0, marginTop: 1 }} />
                <div><div className="alert-title">Gateway Connection Lost (127.0.0.1:{activePort})</div><div>Search, PDF downloads and the interest list resume as soon as the gateway is back.</div></div>
                <button className="action-btn" onClick={handleManualRefresh} style={{ marginLeft: 'auto' }}><RefreshCw size={13} className={isRefreshing ? 'animate-spin' : ''} /><span>Retry</span></button>
              </div>
            </div>
          )}

          <Suspense fallback={<div role="status">Loading…</div>}>
            {activeTab === 'search' && <SearchPage onToggleInterest={handleToggleInterest} interestedPaperIds={interestedIds} initialQuery={explorerQuery} draftQuery={globalQuery} searchNonce={searchNonce} port={activePort} telemetry={telemetry} isOnline={isGatewayOnline === true} onNavigateToExplorer={handleNavigateToExplorer} onShowOverview={handleShowOverview} defaultTopic={defaultTopic} />}
            {activeTab === 'research' && <ResearchLibrary papers={libraryPapers} onRemovePaper={handleRemovePaper} onUpdatePaper={handleUpdatePaper} onRerunSearch={handleNavigateToExplorer} port={activePort} />}
            {activeTab === 'gateway' && <AgentGateway telemetry={telemetry} isOnline={isGatewayOnline === true} onRefresh={fetchTelemetry} port={activePort} />}
            {activeTab === 'settings' && <SettingsPage port={activePort} />}
          </Suspense>
        </main>
      </div>
    </div>
  );
};

export default App;
