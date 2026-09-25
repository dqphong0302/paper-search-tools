import React, { useEffect, useRef, useState } from 'react';
import { gatewayFetch, getGatewayPort } from '../lib/gateway';

interface ResponseData {
  query: string;
  results: { title: string; url: string; snippet: string; engines: string[] }[];
  warnings: unknown[];
  elapsed_ms: number;
}

export const WebSearch: React.FC = () => {
  const [query, setQuery] = useState('');
  const [limit, setLimit] = useState(10);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [data, setData] = useState<ResponseData | null>(null);
  const requestRef = useRef<AbortController | null>(null);
  useEffect(() => () => requestRef.current?.abort(), []);
  const search = async (event: React.FormEvent) => {
    event.preventDefault();
    requestRef.current?.abort();
    const controller = new AbortController();
    requestRef.current = controller;
    setBusy(true); setError(''); setData(null);
    try {
      const response = await gatewayFetch('/api/web/search', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: query.trim(), limit: limit || 10 }), signal: controller.signal,
      });
      const result = await response.json().catch(() => null);
      if (!response.ok) throw new Error(result?.error || `HTTP ${response.status}`);
      if (!controller.signal.aborted) setData(result);
    } catch (e) { if (!controller.signal.aborted) setError(String(e)); }
    finally { if (!controller.signal.aborted) setBusy(false); }
  };
  return <section className="page-container">
    <h2>Academic & Web Search</h2>
    <p className="page-subtitle">SearXNG Connector · Local gateway 127.0.0.1:{getGatewayPort()} · Configure in Settings → System & Storage. Web search results provide general literature and reference context outside indexed academic databases.</p>
    <form onSubmit={search} style={{ display: 'flex', gap: 12, flexWrap: 'wrap', marginBlock: 20 }}>
      <label htmlFor="web-query" style={{ flex: 1 }}>Query
        <input id="web-query" className="field-input" placeholder="e.g. machine learning in medical diagnosis" required maxLength={4000} value={query} onChange={(e) => setQuery(e.target.value)} />
      </label>
      <label htmlFor="web-limit">Result Limit
        <input id="web-limit" className="field-input" type="number" min={1} max={50} required value={limit} onChange={(e) => setLimit(Number(e.target.value))} />
      </label>
      <button id="web-search-submit" className="action-btn action-btn-primary" disabled={busy || !query.trim()}>{busy ? 'Searching…' : 'Search Web'}</button>
    </form>
    {error && <div className="alert alert-warning" role="alert">{error}</div>}
    {data && <div>
      <p role="status" style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 12 }}>{data.results.length} results for “{data.query}” · {data.elapsed_ms} ms</p>
      {data.warnings.length > 0 && <p role="alert" className="alert alert-warning" style={{ marginBottom: 12 }}>Some upstream search engines timed out; coverage may be partial.</p>}
      {data.results.map((item) => <article key={item.url} style={{ paddingBlock: 16, borderBottom: '1px solid var(--cockpit-border)' }}>
        <h3 style={{ fontSize: 15, marginBottom: 4 }}><a href={item.url} target="_blank" rel="noopener noreferrer" style={{ color: 'var(--primary-cyan)', textDecoration: 'none' }}>{item.title}</a></h3>
        <p style={{ overflowWrap: 'anywhere', fontSize: 12, color: 'var(--text-dim)', marginBottom: 6 }}>{item.url}</p>
        <p style={{ fontSize: 13, color: 'var(--text-muted)', lineHeight: 1.5, marginBottom: 6 }}>{item.snippet}</p>
        <small style={{ fontSize: 11, color: 'var(--text-dim)' }}>Source: SearXNG{item.engines.length > 0 ? ` / ${item.engines.join(', ')}` : ''}</small>
      </article>)}
    </div>}
  </section>;
};
