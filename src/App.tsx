import React, { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react';
import { SideNav, TabType } from './components/layout/SideNav';
import { SearchPage } from './components/SearchPage';
const ResearchLibrary = lazy(() => import('./components/ResearchWorkspace').then((m) => ({ default: m.ResearchLibrary })));
const AgentGateway = lazy(() => import('./components/AgentGateway').then((m) => ({ default: m.AgentGateway })));
const SettingsPage = lazy(() => import('./components/SettingsPage').then((m) => ({ default: m.SettingsPage })));
import { ChevronRight, Download, RefreshCw, Search, WifiOff } from 'lucide-react';
import { DEFAULT_GATEWAY_PORT } from './lib/gateway';
import { readSettings, saveSettings, SettingsAuthRequired } from './lib/settings';
import { TopicSetupModal } from './components/TopicSetupModal';
import { useGatewayStatus } from './hooks/useGatewayStatus';
import { useAutoUpdate } from './hooks/useAutoUpdate';
import { useLibrary } from './hooks/useLibrary';
import { useSearchNavigation } from './hooks/useSearchNavigation';
import { useWorkspace, WorkspaceProvider } from './state/WorkspaceContext';

const TAB_TITLES: Record<TabType, string> = {
  search: 'Search',
  research: 'Library',
  gateway: 'Connections',
  settings: 'Settings',
};

export const App: React.FC = () => {
  const gateway = useGatewayStatus();
  return (
    <WorkspaceProvider ready={gateway.port !== null}>
      <AppShell gateway={gateway} />
    </WorkspaceProvider>
  );
};

const AppShell: React.FC<{ gateway: ReturnType<typeof useGatewayStatus> }> = ({ gateway }) => {
  const { port, telemetry, online, refresh: refreshStatus } = gateway;
  const ready = port !== null;
  const [activeTab, setActiveTab] = useState<TabType>('search');
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [topicSetupRequired, setTopicSetupRequired] = useState(false);
  const [defaultTopic, setDefaultTopic] = useState('default');

  const workspace = useWorkspace();
  const library = useLibrary(workspace.active.id, ready);
  const update = useAutoUpdate();
  const search = useSearchNavigation(useCallback(() => setActiveTab('search'), []));

  useEffect(() => {
    if (!ready) return;
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
  }, [ready]);

  const completeTopicSetup = async (preset: string, sources: string[]) => {
    await saveSettings({
      domain_preset: preset,
      enabled_sources: sources.join(','),
      topic_setup_completed: 'true',
    });
    setDefaultTopic(preset);
    setTopicSetupRequired(false);
  };

  const handleManualRefresh = async () => {
    setIsRefreshing(true);
    await Promise.all([refreshStatus(), library.load(), workspace.reload()]);
    setTimeout(() => setIsRefreshing(false), 400);
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

  const offline = online === false;
  const activePort = port ?? DEFAULT_GATEWAY_PORT;
  const statusTone = offline ? 'rose' : 'emerald';

  return (
    <div className="cockpit-layout">
      {topicSetupRequired && <TopicSetupModal onComplete={completeTopicSetup} />}
      <SideNav
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        isCollapsed={isCollapsed}
        setIsCollapsed={setIsCollapsed}
        savedCount={library.papers.length}
      />

      <div className="cockpit-stage">
        <header className="cockpit-top-bar">
          <div className="cockpit-breadcrumb">
            <span className="breadcrumb-root">ScholarGate</span>
            <ChevronRight size={14} className="text-dim" />
            <h1 className="breadcrumb-current">{TAB_TITLES[activeTab]}</h1>
          </div>

          <form className="search-omnibox shell-omnibox" onSubmit={(event) => { event.preventDefault(); search.submit(search.draft); }}>
            <Search size={16} className="omnibox-icon" />
            <input ref={globalInputRef} id="global-search-input" className="search-input" type="text" placeholder="Search papers, authors, DOI… (⌘K)" value={search.draft} onChange={(event) => search.setDraft(event.target.value)} aria-label="Global search" />
            <button id="global-search-submit" type="submit" className="search-submit-btn" disabled={!search.draft.trim()}><span>Search</span></button>
          </form>

          <div className="top-bar-actions">
            <div className={`cockpit-status-pill status-${statusTone}`} title={offline ? 'Disconnected from local gateway' : 'Local gateway active (REST + MCP)'}>
              <span className="pulse-dot" />
              <span>127.0.0.1:{activePort} {online === null ? '...' : offline ? 'OFFLINE' : 'ONLINE'}</span>
            </div>
            <button id="refresh-gateway-status" className="action-btn action-btn-icon" onClick={handleManualRefresh} title="Refresh system status" aria-label="Refresh system status"><RefreshCw size={14} className={isRefreshing ? 'animate-spin' : ''} /></button>
          </div>
        </header>

        <main className="cockpit-scroll-body">
          {update.version && (
            <div className="alert alert-info page-alert" role="status">
              <Download size={16} />
              <div>
                <div className="alert-title">ScholarGate {update.version} is available</div>
                {update.error && <div>{update.error}</div>}
              </div>
              <div className="alert-actions">
                <button id="update-later" className="action-btn" onClick={() => void update.later()} disabled={update.installing}>Later</button>
                <button id="update-install" className="action-btn action-btn-primary" onClick={() => void update.install()} disabled={update.installing}>
                  {update.installing ? 'Installing…' : 'Install & restart'}
                </button>
              </div>
            </div>
          )}
          {library.error && <div className="alert alert-warning page-alert" role="alert">{library.error}</div>}
          {offline && (
            <div className="alert alert-danger page-alert">
              <WifiOff size={16} />
              <div><div className="alert-title">Gateway Connection Lost (127.0.0.1:{activePort})</div><div>Search, PDF downloads and the interest list resume as soon as the gateway is back.</div></div>
              <button className="action-btn alert-actions" onClick={handleManualRefresh}><RefreshCw size={13} className={isRefreshing ? 'animate-spin' : ''} /><span>Retry</span></button>
            </div>
          )}

          {!ready ? (
            <div role="status" className="page-container">Connecting to the local gateway…</div>
          ) : (
            <Suspense fallback={<div role="status">Loading…</div>}>
              {activeTab === 'search' && <SearchPage onToggleInterest={library.toggle} interestedPaperIds={library.savedIds} initialQuery={search.query} draftQuery={search.draft} searchNonce={search.nonce} onNavigateToExplorer={search.submit} onShowOverview={search.clear} defaultTopic={defaultTopic} />}
              {activeTab === 'research' && <ResearchLibrary papers={library.papers} onRemovePaper={library.remove} onUpdatePaper={library.update} onRerunSearch={search.submit} />}
              {activeTab === 'gateway' && <AgentGateway telemetry={telemetry} isOnline={online === true} onRefresh={refreshStatus} />}
              {activeTab === 'settings' && <SettingsPage />}
            </Suspense>
          )}
        </main>
      </div>
    </div>
  );
};

export default App;
