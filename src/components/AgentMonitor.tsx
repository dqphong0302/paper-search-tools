import React, { useState } from 'react';
import { Activity, Terminal, Check, Copy, RefreshCw, Zap, AlertTriangle, Loader2 } from 'lucide-react';
import { TelemetryStats, SearchResponse } from '../types';
import { gatewayFetch, getGatewayPort } from '../lib/gateway';

interface AgentMonitorProps {
  telemetry: TelemetryStats | null;
  isOnline: boolean;
  onRefresh: () => void;
}

export const AgentMonitor: React.FC<AgentMonitorProps> = ({ telemetry, isOnline, onRefresh }) => {
  const [copiedSnippet, setCopiedSnippet] = useState<string | null>(null);
  const [testQuery, setTestQuery] = useState('lung cancer targeted therapy');
  const [testRunning, setTestRunning] = useState(false);
  const [testResult, setTestResult] = useState<SearchResponse | null>(null);
  const [testError, setTestError] = useState<string | null>(null);

  const copyText = (text: string, label: string) => {
    navigator.clipboard.writeText(text);
    setCopiedSnippet(label);
    setTimeout(() => setCopiedSnippet(null), 2000);
  };

  const runTestQuery = async () => {
    if (!testQuery.trim()) return;
    setTestRunning(true);
    setTestError(null);
    try {
      const res = await gatewayFetch('/api/search', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'x-sg-agent': 'Simulator (UI test)' },
        body: JSON.stringify({ query: testQuery.trim(), limit: 3 }),
      });
      if (!res.ok) throw new Error(`Server returned status code ${res.status}`);
      setTestResult(await res.json());
      onRefresh();
    } catch (err) {
      setTestResult(null);
      setTestError(
        err instanceof TypeError
          ? `Unable to connect to gateway at 127.0.0.1:${getGatewayPort()}.`
          : (err as Error).message
      );
    } finally {
      setTestRunning(false);
    }
  };

  const claudeConfig = JSON.stringify(
    {
      mcpServers: {
        scholargate: {
          url: `http://localhost:${getGatewayPort()}/mcp`,
          headers: { Authorization: 'Bearer <MCP_AUTH_TOKEN>' },
        },
      },
    },
    null,
    2
  );

  const pythonSnippet = `import requests

res = requests.post("http://localhost:${getGatewayPort()}/api/search", json={
    "query": "type 2 diabetes mellitus",
    "limit": 5
})
papers = res.json().get("papers", [])
for p in papers:
    print(f"[{p['year']}] {p['title']} ({p['source']})")`;

  const metrics = [
    {
      label: 'GATEWAY STATUS',
      value: isOnline ? telemetry?.gateway_status || 'ONLINE' : 'OFFLINE',
      color: isOnline ? 'var(--status-emerald)' : 'var(--status-rose)',
      sub: `Port ${telemetry?.port ?? getGatewayPort()} (REST + MCP)`,
      dot: true,
    },
    {
      label: 'TOTAL QUERIES',
      value: telemetry ? telemetry.total_queries.toLocaleString() : '—',
      color: 'var(--text-main)',
      sub: 'Dispatched by user & AI Agents',
    },
    {
      label: 'CACHE HIT RATE',
      value: telemetry ? `${telemetry.cache_hit_rate.toFixed(1)}%` : '—',
      color: 'var(--primary-cyan)',
      sub: 'Served from local SQLite cache',
    },
    {
      label: 'AVG LATENCY (LAST 15)',
      // Multi-source searches run into seconds; a five-digit millisecond figure is
      // harder to read than the number it stands for.
      value: telemetry
        ? telemetry.avg_latency_ms >= 1000
          ? `${(telemetry.avg_latency_ms / 1000).toFixed(1)} s`
          : `${telemetry.avg_latency_ms} ms`
        : '—',
      color: 'var(--status-violet)',
      sub: 'Multi-source parallel (RRF k=60)',
    },
  ];

  return (
    <div className="page-container">
      {/* Metrics */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
          gap: 12,
        }}
      >
        {metrics.map((m) => (
          <div key={m.label} className="cockpit-card" style={{ padding: 16 }}>
            <div
              style={{
                fontSize: 11,
                fontFamily: 'var(--font-mono)',
                color: 'var(--text-dim)',
                marginBottom: 6,
              }}
            >
              {m.label}
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              {m.dot && (
                <span
                  className="pulse-dot"
                  style={{ background: m.color, boxShadow: `0 0 8px ${m.color}` }}
                />
              )}
              <span style={{ fontSize: 20, fontWeight: 700, color: m.color }}>{m.value}</span>
            </div>
            <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>{m.sub}</div>
          </div>
        ))}
      </div>

      {/* Logs + simulator */}
      <div className="monitor-logs-grid">
        <div className="cockpit-card" style={{ padding: 18, minWidth: 0 }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              marginBottom: 14,
              gap: 10,
            }}
          >
            <div className="cockpit-card-title">
              <Activity size={15} style={{ color: 'var(--primary-cyan)' }} />
              <span>Real-time Telemetry & Query Logs</span>
            </div>
            <button className="action-btn" onClick={onRefresh} style={{ padding: '4px 8px' }}>
              <RefreshCw size={12} />
              <span>Refresh</span>
            </button>
          </div>

          <div style={{ overflowX: 'auto', maxHeight: 300, overflowY: 'auto' }}>
            <table className="telemetry-table">
              <thead>
                <tr>
                  <th style={{ whiteSpace: 'nowrap' }}>TIME</th>
                  <th>CALLER / AGENT</th>
                  <th>QUERY</th>
                  <th style={{ textAlign: 'right', whiteSpace: 'nowrap' }}>RESULTS</th>
                  <th style={{ textAlign: 'right', whiteSpace: 'nowrap' }}>LATENCY</th>
                  <th>STATUS</th>
                </tr>
              </thead>
              <tbody>
                {telemetry?.recent_logs?.length ? (
                  telemetry.recent_logs.map((log) => (
                    <tr key={log.id}>
                      <td style={{ fontFamily: 'var(--font-mono)', fontSize: 11, whiteSpace: 'nowrap' }}>
                        {log.timestamp}
                      </td>
                      <td
                        style={{
                          maxWidth: 130,
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                          color: 'var(--primary-cyan)',
                          fontWeight: 500,
                        }}
                        title={log.agent_name}
                      >
                        {log.agent_name}
                      </td>
                      <td
                        style={{
                          maxWidth: 160,
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                        title={log.query}
                      >
                        {log.query}
                      </td>
                      <td style={{ fontFamily: 'var(--font-mono)', textAlign: 'right' }}>
                        {log.result_count}
                      </td>
                      <td style={{ fontFamily: 'var(--font-mono)', textAlign: 'right' }}>
                        {log.latency_ms}ms
                      </td>
                      <td>
                        <span
                          style={{
                            color: log.status.includes('OK')
                              ? 'var(--status-emerald)'
                              : 'var(--status-amber)',
                            fontSize: 11,
                            whiteSpace: 'nowrap',
                          }}
                        >
                          {log.status}
                        </span>
                      </td>
                    </tr>
                  ))
                ) : (
                  <tr>
                    <td colSpan={6} style={{ textAlign: 'center', padding: 24, color: 'var(--text-dim)' }}>
                      {isOnline
                        ? 'No queries recorded yet. The CALLER column distinguishes user UI actions from AI agents (MCP/REST).'
                        : 'Local gateway server is offline — cannot read logs.'}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>

        <div className="cockpit-card" style={{ padding: 18, minWidth: 0 }}>
          <div className="cockpit-card-title" style={{ marginBottom: 12 }}>
            <Zap size={15} style={{ color: 'var(--status-amber)' }} />
            <span>Agent Request Simulator</span>
          </div>

          <p style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 12 }}>
            Simulate an autonomous AI Agent dispatching an academic search query to port{' '}
            <code style={{ fontFamily: 'var(--font-mono)' }}>localhost:{getGatewayPort()}</code>:
          </p>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              runTestQuery();
            }}
            style={{ display: 'flex', gap: 8, marginBottom: 12 }}
          >
            <input
              type="text"
              className="field-input"
              value={testQuery}
              onChange={(e) => setTestQuery(e.target.value)}
              aria-label="Test query"
            />
            <button
              type="submit"
              className="action-btn action-btn-primary"
              disabled={testRunning || !testQuery.trim()}
              style={{ flexShrink: 0 }}
            >
              {testRunning ? <Loader2 size={14} className="animate-spin" /> : <Zap size={14} />}
              <span>{testRunning ? 'Dispatching…' : 'Send Test'}</span>
            </button>
          </form>

          {testError && (
            <div className="alert alert-danger" style={{ marginBottom: 10 }}>
              <AlertTriangle size={15} style={{ flexShrink: 0, marginTop: 1 }} />
              <div>{testError}</div>
            </div>
          )}

          {testResult && (
            <div
              style={{
                background: '#f8fafc',
                border: '1px solid var(--cockpit-border)',
                padding: 10,
                borderRadius: 'var(--radius-sm)',
                fontSize: 11,
                fontFamily: 'var(--font-mono)',
                maxHeight: 170,
                overflowY: 'auto',
              }}
            >
              <div style={{ color: 'var(--status-emerald)', fontWeight: 600, marginBottom: 6 }}>
                ✓ Response: {testResult.total} papers ({testResult.elapsed_ms}ms)
              </div>
              {testResult.papers?.map((p, i) => (
                <div key={p.id || i} style={{ marginBottom: 3, lineHeight: 1.4 }}>
                  {i + 1}. {p.title}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Integration snippets */}
      <div className="cockpit-card" style={{ padding: 18 }}>
        <div className="cockpit-card-title" style={{ marginBottom: 14 }}>
          <Terminal size={15} style={{ color: 'var(--status-violet)' }} />
          <span>One-Click Agent Integration Snippets</span>
        </div>

        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fit, minmax(320px, 1fr))',
            gap: 16,
          }}
        >
          {[
            { key: 'claude', title: 'Claude Desktop / Cursor (MCP Streamable HTTP)', code: claudeConfig, color: 'var(--primary-cyan)' },
            { key: 'python', title: 'Python Script / CLI (REST API)', code: pythonSnippet, color: 'var(--status-violet)' },
          ].map((snippet) => (
            <div
              key={snippet.key}
              style={{
                background: '#f8fafc',
                padding: 12,
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--cockpit-border)',
                minWidth: 0,
              }}
            >
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  marginBottom: 8,
                  gap: 8,
                }}
              >
                <span style={{ fontSize: 12, fontWeight: 600, color: snippet.color }}>
                  {snippet.title}
                </span>
                <button
                  className="action-btn"
                  style={{ padding: '2px 8px', fontSize: 11, flexShrink: 0 }}
                  onClick={() => copyText(snippet.code, snippet.key)}
                >
                  {copiedSnippet === snippet.key ? (
                    <Check size={12} color="var(--status-emerald)" />
                  ) : (
                    <Copy size={12} />
                  )}
                  <span>{copiedSnippet === snippet.key ? 'Copied' : 'Copy'}</span>
                </button>
              </div>
              <pre
                style={{
                  fontSize: 11,
                  fontFamily: 'var(--font-mono)',
                  color: 'var(--text-main)',
                  overflowX: 'auto',
                  margin: 0,
                }}
              >
                {snippet.code}
              </pre>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
