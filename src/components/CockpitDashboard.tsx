import React, { useState, useEffect, useMemo } from 'react';
import {
  Search,
  Database,
  Cpu,
  Zap,
  ArrowRight,
  Activity,
  Layers,
  Globe,
  Stethoscope,
  Sparkles,
} from 'lucide-react';
import searchCatalog from '../lib/searchCatalog.json';
import { TelemetryStats } from '../types';
import { gatewayFetch } from '../lib/gateway';

/**
 * One icon per source group. Order matters: "AI & Syntheses" also contains
 * "AI", so the more specific label is matched first.
 */
const groupIcon = (name: string) => {
  if (name.includes('Syntheses')) return <Sparkles size={13} style={{ color: '#f59e0b' }} />;
  if (name.includes('Biomedical')) return <Stethoscope size={13} style={{ color: '#ef4444' }} />;
  if (name.includes('Vietnamese')) return <span style={{ fontSize: 12 }}>🇻🇳</span>;
  if (name.includes('CS') || name.includes('Engineering')) return <Cpu size={13} style={{ color: '#2563eb' }} />;
  if (name.includes('Multidisciplinary')) return <Globe size={13} style={{ color: '#0284c7' }} />;
  return <Database size={13} style={{ color: '#8b5cf6' }} />;
};

interface CockpitDashboardProps {
  telemetry: TelemetryStats | null;
  isOnline: boolean;
  onNavigateToExplorer: (query: string) => void;
  port: number;
  /** The app shell already provides the search box. */
  hideSearch?: boolean;
}

const ALL_SOURCES = searchCatalog.sources.map((source) => ({ ...source, tag: source.group }));
const PRESET_TOPICS = searchCatalog.presets.filter((preset) => preset.query);

export const CockpitDashboard: React.FC<CockpitDashboardProps> = ({
  telemetry,
  isOnline,
  onNavigateToExplorer,
  port,
  hideSearch = false,
}) => {
  const [omniboxInput, setOmniboxInput] = useState('');
  const [enabledSources, setEnabledSources] = useState<string[] | null>(null);

  // Source list reflects what is actually enabled in Settings, not a fixed "6/6".
  useEffect(() => {
    const load = async () => {
      try {
        const res = await gatewayFetch('/api/config');
        if (!res.ok) { setEnabledSources(null); return; }
        const cfg = await res.json();
        const preset = (cfg.domain_preset || 'auto') as string;
        if (preset === 'custom' && typeof cfg.enabled_sources === 'string') {
          setEnabledSources(
            cfg.enabled_sources
              .split(',')
              .map((s: string) => s.trim().toLowerCase())
              .filter(Boolean)
          );
        } else setEnabledSources(searchCatalog.presets.find((item) => item.id === preset)?.sources ?? []);
      } catch {
        setEnabledSources(null);
      }
    };
    load();
  }, [port, isOnline]);

  const handleSearchSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (omniboxInput.trim()) onNavigateToExplorer(omniboxInput.trim());
  };

  const isSourceOn = (id: string) => enabledSources?.includes(id) ?? false;
  const activeCount = ALL_SOURCES.filter((s) => isSourceOn(s.id)).length;

  const groupedSources = useMemo(() => {
    const map = new Map<string, typeof ALL_SOURCES>();
    for (const s of ALL_SOURCES) {
      const g = s.tag || 'Other';
      if (!map.has(g)) map.set(g, []);
      map.get(g)!.push(s);
    }
    return Array.from(map.entries()).map(([groupName, list]) => {
      const onCount = list.filter((s) => isSourceOn(s.id)).length;
      return {
        groupName,
        sources: list,
        onCount,
        totalCount: list.length,
      };
    });
  }, [enabledSources]);

  const hasStats = !!telemetry;
  const fmt = (value: string | number | undefined) =>
    hasStats && value !== undefined ? value : '—';

  return (
    <div className="page-container">
      {/* Header */}
      <div className="page-header">
        <div>
          <h2 className="dashboard-brand-title">ScholarGate</h2>
          <p className="page-subtitle">Multidisciplinary Academic Discovery & AI Agent Gateway</p>
        </div>
      </div>

      {/* Omnibox (hidden when embedded under the app-shell search) */}
      <div>
        {!hideSearch && (
          <form id="dashboard-search" onSubmit={handleSearchSubmit} className="search-omnibox">
            <Search size={18} style={{ color: 'var(--primary-cyan)', marginLeft: 4 }} />
            <input
              id="dashboard-search-input"
              type="text"
              className="search-input"
              placeholder="Search papers, authors, DOI, or research topics… (⌘K)"
              value={omniboxInput}
              onChange={(e) => setOmniboxInput(e.target.value)}
              aria-label="Quick research query"
              autoFocus
            />
            <button id="dashboard-search-submit" type="submit" className="search-submit-btn" disabled={!omniboxInput.trim()}>
              <span>Search</span>
              <ArrowRight size={14} />
            </button>
          </form>
        )}

        <div className="presets-carousel" style={{ marginTop: 10, marginBottom: 0 }}>
          {PRESET_TOPICS.map((topic) => (
            <button
              key={topic.id}
              id={`dashboard-preset-${topic.id}`}
              type="button"
              className="preset-chip"
              onClick={() => onNavigateToExplorer(topic.query)}
            >
              {topic.label}
            </button>
          ))}
        </div>
      </div>

      {/* Metrics */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(210px, 1fr))',
          gap: 12,
        }}
      >
        <div className="cockpit-card" style={{ padding: 14 }}>
          <div className="cockpit-card-title">
            <Cpu size={15} style={{ color: 'var(--primary-cyan)' }} />
            <span>Total Queries</span>
          </div>
          <div className="stat-value">{fmt(telemetry?.total_queries.toLocaleString())}</div>
          <div className="stat-subtext">AI Agents & User Searches</div>
        </div>

        <div className="cockpit-card" style={{ padding: 14 }}>
          <div className="cockpit-card-title">
            <Database size={15} style={{ color: 'var(--status-emerald)' }} />
            <span>Cache Hit Rate</span>
          </div>
          <div className="stat-value" style={{ color: hasStats ? 'var(--status-emerald)' : undefined }}>
            {hasStats ? `${Math.round(telemetry!.cache_hit_rate)}%` : '—'}
          </div>
          <div className="cockpit-progress-bar-bg">
            <div
              className="cockpit-progress-bar-fill"
              style={{ width: `${hasStats ? Math.min(telemetry!.cache_hit_rate, 100) : 0}%` }}
            />
          </div>
        </div>

        <div className="cockpit-card" style={{ padding: 14 }}>
          <div className="cockpit-card-title">
            <Zap size={15} style={{ color: 'var(--status-amber)' }} />
            <span>Avg Latency</span>
          </div>
          <div className="stat-value">{hasStats ? `${telemetry!.avg_latency_ms} ms` : '—'}</div>
          <div className="stat-subtext">Parallel RRF Multi-Source Fusion</div>
        </div>
      </div>

      {/* Sources */}
      <div className="cockpit-card" style={{ padding: 14 }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            marginBottom: 10,
            gap: 10,
          }}
        >
          <div className="cockpit-card-title">
            <Layers size={15} style={{ color: 'var(--primary-cyan)' }} />
            <span>Active Academic Sources</span>
          </div>
          <span className={`cockpit-badge ${activeCount > 0 ? 'badge-emerald' : 'badge-vjol'}`}>
            {enabledSources === null ? 'Not Configured' : `${activeCount}/${ALL_SOURCES.length} Active`}
          </span>
        </div>

        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fit, minmax(230px, 1fr))',
            gap: 10,
          }}
        >
          {groupedSources.map((g) => {
            const hasActive = g.onCount > 0;
            return (
              <div
                key={g.groupName}
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 6,
                  padding: '10px 12px',
                  background: hasActive ? '#fcfdfe' : '#fff',
                  borderRadius: 'var(--radius-sm)',
                  border: `1px solid ${hasActive ? 'var(--cockpit-border)' : 'var(--cockpit-border-subtle)'}`,
                  opacity: hasActive ? 1 : 0.6,
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontWeight: 600, fontSize: 12 }}>
                    {groupIcon(g.groupName)}
                    <span>{g.groupName}</span>
                  </div>
                  <span
                    className={`cockpit-badge ${g.onCount > 0 ? 'badge-emerald' : 'badge-vjol'}`}
                    style={{ fontSize: 10, padding: '1px 6px' }}
                  >
                    {g.onCount}/{g.totalCount}
                  </span>
                </div>
                <div
                  style={{
                    fontSize: 10.5,
                    color: 'var(--text-dim)',
                    whiteSpace: 'nowrap',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                  }}
                  title={g.sources.map((s) => s.name).join(', ')}
                >
                  {g.sources.map((s) => s.name).join(' · ')}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Recent activity */}
      <div className="cockpit-card" style={{ padding: 14 }}>
        <div className="cockpit-card-title" style={{ marginBottom: 10 }}>
          <Activity size={15} style={{ color: 'var(--status-emerald)' }} />
          <span>Recent Queries</span>
        </div>

        {telemetry?.recent_logs?.length ? (
          <div style={{ overflowX: 'auto' }}>
            <table className="telemetry-table">
              <thead>
                <tr>
                  <th style={{ width: 80 }}>TIME</th>
                  <th style={{ width: 140 }}>CALLER / SOURCE</th>
                  <th>QUERY</th>
                  <th style={{ width: 90, textAlign: 'right' }}>RESULTS</th>
                  <th style={{ width: 90, textAlign: 'right' }}>LATENCY</th>
                </tr>
              </thead>
              <tbody>
                {telemetry.recent_logs.slice(0, 5).map((log) => (
                  <tr key={log.id}>
                    <td style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-dim)', fontSize: 11 }}>
                      {log.timestamp}
                    </td>
                    <td>
                      <span
                        className="cockpit-badge badge-cyan"
                        style={{
                          fontSize: 9,
                          display: 'inline-block',
                          maxWidth: 130,
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                        title={log.agent_name}
                      >
                        {log.agent_name}
                      </span>
                    </td>
                    <td>
                      <button
                        onClick={() => onNavigateToExplorer(log.query)}
                        title="Rerun this query"
                        style={{
                          background: 'none',
                          border: 'none',
                          padding: 0,
                          font: 'inherit',
                          fontWeight: 500,
                          color: 'var(--primary-cyan)',
                          cursor: 'pointer',
                          textAlign: 'left',
                        }}
                      >
                        {log.query}
                      </button>
                    </td>
                    <td style={{ textAlign: 'right', fontFamily: 'var(--font-mono)' }}>
                      <b>{log.result_count}</b> papers
                    </td>
                    <td
                      style={{
                        textAlign: 'right',
                        fontFamily: 'var(--font-mono)',
                        color: 'var(--status-emerald)',
                      }}
                    >
                      {log.latency_ms} ms
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div style={{ padding: '24px 12px', textAlign: 'center', fontSize: 13, color: 'var(--text-muted)' }}>
            {isOnline
              ? 'No queries recorded yet. Search above or invoke via an AI Agent connected to the local gateway.'
              : 'Query logs unavailable while local gateway is disconnected.'}
          </div>
        )}
      </div>
    </div>
  );
};
