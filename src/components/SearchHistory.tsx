import React, { useState, useEffect } from 'react';
import {
  History,
  Search,
  ArrowUpRight,
  Trash2,
  RefreshCw,
  Clock,
  Copy,
  Check,
  Star,
  AlertTriangle
} from 'lucide-react';
import { gatewayFetch, getGatewayPort } from '../lib/gateway';

export interface SearchHistoryItem {
  id: string;
  query: string;
  sources?: string;
  result_count: number;
  elapsed_ms: number;
  created_at: number;
  saved?: boolean;
}

interface SearchHistoryProps {
  onRerunSearch: (query: string) => void;
  workspaceId?: string;
}

export const SearchHistory: React.FC<SearchHistoryProps> = ({ onRerunSearch, workspaceId }) => {
  const [history, setHistory] = useState<SearchHistoryItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [filterText, setFilterText] = useState('');
  const [savedOnly, setSavedOnly] = useState(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const toggleSaved = async (item: SearchHistoryItem) => {
    const next = !item.saved;
    setError(null);
    try {
      const res = await gatewayFetch(`/api/history/searches/${item.id}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ saved: next }),
      });
      if (!res.ok) throw new Error(`Server returned status code ${res.status}`);
      setHistory((prev) => prev.map((row) => (row.id === item.id ? { ...row, saved: next } : row)));
    } catch (e) {
      setError(`Failed to save query: ${(e as Error).message}`);
    }
  };

  const fetchHistory = async () => {
    setLoading(true);
    setError(null);
    try {
      const suffix = workspaceId ? `?workspace_id=${encodeURIComponent(workspaceId)}` : '';
      const res = await gatewayFetch(`/api/history/searches${suffix}`);
      if (!res.ok) throw new Error(`Server returned status code ${res.status}`);
      setHistory(await res.json());
    } catch (e) {
      setError(
        e instanceof TypeError
          ? `Unable to connect to local gateway server at 127.0.0.1:${getGatewayPort()}.`
          : `Failed to load query history: ${(e as Error).message}`
      );
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId]);

  const handleClear = async () => {
    if (!window.confirm('Clear all search history? This cannot be undone.'))
      return;
    setError(null);
    try {
      const suffix = workspaceId ? `?workspace_id=${encodeURIComponent(workspaceId)}` : '';
      const res = await gatewayFetch(`/api/history/searches${suffix}`, { method: "DELETE" });
      if (!res.ok) throw new Error(`Server returned status code ${res.status}`);
      setHistory([]);
    } catch (e) {
      setError(`Failed to clear history: ${(e as Error).message}`);
    }
  };

  const handleDeleteItem = async (id: string) => {
    setError(null);
    try {
      const res = await gatewayFetch(`/api/history/searches/${id}`, { method: 'DELETE' });
      if (!res.ok) throw new Error(`Server returned status code ${res.status}`);
      setHistory((prev) => prev.filter((item) => item.id !== id));
    } catch (e) {
      setError(`Failed to delete query: ${(e as Error).message}`);
    }
  };

  const copyQuery = (id: string, q: string) => {
    navigator.clipboard.writeText(q).then(() => {
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    }).catch(() => setError('Unable to copy search query to clipboard.'));
  };

  const formatTime = (timestamp: number) => {
    if (!timestamp) return 'Recent';
    const date = new Date(timestamp * 1000);
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }) + ' - ' + date.toLocaleDateString();
  };

  const filtered = history.filter(
    (item) =>
      (!savedOnly || item.saved) && item.query.toLowerCase().includes(filterText.toLowerCase())
  );

  return (
    <div className="page-container">
      {/* Header */}
      <div className="page-header" style={{ borderBottom: '1px solid var(--cockpit-border)', paddingBottom: 16 }}>
        <div>
          <h2 className="page-title">
            <History size={17} style={{ color: 'var(--primary-cyan)' }} />
            <span>Search History</span>
            <span className="cockpit-badge badge-cyan">{history.length} Queries</span>
          </h2>
          <p className="page-subtitle">Tracks every query dispatched from the omnibox and autonomous AI agents</p>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <button className="action-btn" onClick={fetchHistory} title="Refresh history">
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
            <span>Sync</span>
          </button>
          {history.length > 0 && (
            <button className="action-btn action-btn-danger" onClick={handleClear}>
              <Trash2 size={14} />
              <span>Clear All</span>
            </button>
          )}
        </div>
      </div>

      {error && (
        <div className="alert alert-danger">
          <AlertTriangle size={16} style={{ flexShrink: 0, marginTop: 1 }} />
          <div>{error}</div>
        </div>
      )}

      {/* Filter Bar */}
      {history.length > 0 && (
        <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
          <div
            style={{
              flex: 1,
              display: 'flex',
              alignItems: 'center',
              background: 'var(--cockpit-card)',
              border: '1px solid var(--cockpit-border)',
              borderRadius: 'var(--radius-sm)',
              padding: '6px 12px',
              gap: 8,
            }}
          >
            <Search size={15} style={{ color: 'var(--text-dim)' }} />
            <input
              id="filter-search-history"
              aria-label="Filter search history"
              type="text"
              placeholder="Filter saved or recent queries..."
              value={filterText}
              onChange={(e) => setFilterText(e.target.value)}
              style={{
                background: 'transparent',
                border: 'none',
                color: 'var(--text-main)',
                fontSize: 13,
                outline: 'none',
                width: '100%',
              }}
            />
          </div>
          <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, whiteSpace: 'nowrap' }}>
            <input
              type="checkbox"
              checked={savedOnly}
              onChange={(e) => setSavedOnly(e.target.checked)}
              style={{ accentColor: 'var(--primary-cyan)' }}
            />
            <span>Starred queries only</span>
          </label>
        </div>
      )}

      {/* List / Empty State */}
      {filtered.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon">
            <Search size={24} />
          </div>
          <div className="empty-state-title">
            {filterText ? 'No queries match your filter' : 'No search history recorded'}
          </div>
          <div className="empty-state-text">
            {filterText
              ? 'Try another keyword or uncheck filter options to see all queries.'
              : 'Execute searches in the Explorer tab to track your queries here.'}
          </div>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {filtered.map((item) => (
            <div
              key={item.id}
              className="cockpit-card"
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '14px 18px',
              }}
            >
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6, flex: 1, minWidth: 0, paddingRight: 16 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <Search size={15} style={{ color: 'var(--primary-cyan)', flexShrink: 0 }} />
                  <span
                    style={{
                      fontSize: 14,
                      fontWeight: 600,
                      color: 'var(--text-main)',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {item.query}
                  </span>
                </div>

                <div style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 11, color: 'var(--text-dim)' }}>
                  <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontFamily: 'var(--font-mono)' }}>
                    <Clock size={12} />
                    {formatTime(item.created_at)}
                  </span>
                  <span>•</span>
                  <span className="cockpit-badge badge-emerald" style={{ fontSize: 10 }}>
                    {item.result_count} results
                  </span>
                  <span>•</span>
                  <span style={{ fontFamily: 'var(--font-mono)' }}>{item.elapsed_ms} ms</span>
                  {item.sources && (
                    <>
                      <span>•</span>
                      <span style={{ color: 'var(--text-muted)' }}>Sources: {item.sources}</span>
                    </>
                  )}
                </div>
              </div>

              <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
                <button
                  className={`action-btn ${item.saved ? 'action-btn-primary' : ''}`}
                  onClick={() => toggleSaved(item)}
                  title={item.saved ? 'Unstar query' : 'Star query'}
                  aria-pressed={item.saved}
                  style={{ padding: '6px 10px' }}
                >
                  <Star size={14} fill={item.saved ? '#f59e0b' : 'none'} color={item.saved ? '#f59e0b' : undefined} />
                </button>
                <button
                  className="action-btn"
                  onClick={() => copyQuery(item.id, item.query)}
                  title="Copy search query"
                  style={{ padding: '6px 10px' }}
                >
                  {copiedId === item.id ? <Check size={12} color="var(--status-emerald)" /> : <Copy size={12} />}
                </button>

                <button
                  className="action-btn action-btn-danger"
                  onClick={() => handleDeleteItem(item.id)}
                  title="Remove query"
                  aria-label="Remove query"
                  style={{ padding: '6px 10px' }}
                >
                  <Trash2 size={14} />
                </button>

                <button
                  className="action-btn action-btn-primary"
                  onClick={() => onRerunSearch(item.query)}
                  title="Rerun query in Explorer"
                  style={{ padding: '6px 14px' }}
                >
                  <span>Rerun</span>
                  <ArrowUpRight size={14} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
