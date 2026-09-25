import React, { useState } from 'react';
import { Database, Globe, RefreshCw, Server, Trash2, Zap } from 'lucide-react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { gatewayFetch, getGatewayPort } from '../../lib/gateway';
import { BackupRestore } from '../BackupRestore';
import { UpdateCenter } from '../UpdateCenter';
import { isValidHttpUrl, SettingsFormApi } from './model';
import { Card, CheckboxField, Field, SecretField, TestStatus, TextField } from './ui';
import { useConnectionTests } from './useConnectionTests';

const NUMERIC_FIELDS: { key: string; label: string; min: string; max: string; step?: string; id?: string }[] = [
  { key: 'gateway_port', label: 'Port (effective upon app restart)', min: '1024', max: '65535' },
  { key: 'search_timeout_seconds', label: 'Source Request Timeout (seconds)', min: '1', max: '120' },
  { key: 'max_results_default', label: 'Default Max Results (1–50)', min: '1', max: '50' },
  { key: 'cache_ttl_hours', label: 'Cache TTL (hours, 1–720)', min: '1', max: '720' },
  { key: 'rate_limit_per_minute', label: 'Rate Limit / min per Agent (0 = disabled)', min: '0', max: '100000' },
  { key: 'search_delay_ms', label: 'Delay Between Searches (ms, 0–60000)', min: '0', max: '60000', step: '100', id: 'search-delay-ms' },
];

export const GatewayTab: React.FC<SettingsFormApi> = ({ config, set, onError }) => {
  const values = config as Record<string, string>;
  const tests = useConnectionTests();
  const [cacheCleared, setCacheCleared] = useState(false);
  const port = getGatewayPort();

  const testWebSearch = () => {
    if (!isValidHttpUrl(config.web_search_url)) return tests.fail('web_search', 'Enter a valid HTTP(S) web-search base URL.');
    void tests.run('web_search', '/api/test-searxng', { url: config.web_search_url, categories: 'general', engines: '' });
  };

  const chooseDownloadDirectory = async () => {
    try {
      const selected = await invoke<string | null>('choose_download_directory');
      if (selected) set('download_directory', selected);
    } catch (e) {
      onError(`Unable to choose download directory: ${String(e)}`);
    }
  };

  const clearCache = async () => {
    if (!window.confirm('Clear all SQLite search cache?')) return;
    try {
      const res = await gatewayFetch('/api/cache/clear', { method: 'POST' });
      const data = await res.json().catch(() => null);
      if (!res.ok || data?.success !== true) throw new Error(data?.error || `HTTP ${res.status}`);
      setCacheCleared(true);
      setTimeout(() => setCacheCleared(false), 2000);
    } catch (e) {
      onError(`Unable to clear cache: ${(e as Error).message}`);
    }
  };

  return (
    <div className="u-stack u-gap-14">
      <Card advanced title="Local Gateway Server" subtitle={`Serving at 127.0.0.1:${port} (REST + MCP)`} icon={<Server size={16} className="icon-accent" />}>
        <div className="settings-grid settings-grid-200">
          {NUMERIC_FIELDS.map((f) => (
            <TextField key={f.key} id={f.id} label={f.label} type="number" min={f.min} max={f.max} step={f.step} value={values[f.key]} onValue={(v) => set(f.key, v)} />
          ))}
          <Field label="PDF Download Directory">
            <div className="u-row u-gap-6 u-nowrap">
              <input id="download-directory" type="text" className="settings-input" value={config.download_directory} onChange={(e) => set('download_directory', e.target.value)} placeholder="Default: Documents/ScholarGate/Papers" />
              {isTauri() && <button id="choose-download-directory" type="button" className="action-btn" onClick={() => void chooseDownloadDirectory()}>Browse…</button>}
            </div>
          </Field>
        </div>
        <p className="settings-note">Delay queues cache-miss searches to prevent request bursts; cached pages remain instant. Timeout applies per source request. Changing the port requires a full restart.</p>
        {config.gateway_port !== String(port) && (
          <div className="alert alert-warning" role="status">
            Running on port {port}; configured port {config.gateway_port || '—'} takes effect after a full restart. MCP installs use the saved configured port.
          </div>
        )}
      </Card>

      <Card
        title="Outbound Proxy"
        subtitle="Route academic source and MetaSearch requests through one HTTP(S) proxy; takes effect immediately after saving"
        icon={<Globe size={16} className="icon-accent" />}
        right={<CheckboxField id="proxy-enabled" label="Enable proxy" checked={config.proxy_enabled === 'true'} onChange={(on) => set('proxy_enabled', String(on))} />}
      >
        <SecretField name="proxy_url" value={config.proxy_url} onChange={set} />
        <p className="settings-note">Credentials may be embedded in the URL and are stored as a write-only secret. Use Check Sources to verify routing.</p>
      </Card>

      <Card title="Security & Authentication" subtitle="Token protects all endpoints; write-only secret uses the OS keychain when available" icon={<Server size={16} className="tone-rose" />}>
        <SecretField name="mcp_auth_token" value={config.mcp_auth_token} onChange={set} placeholder="Leave blank for backward compatibility" />
        <p className="settings-note">
          When a token is set, all routes (<code>/api/*</code>, <code>/mcp</code>, <code>/sse</code>, <code>/messages</code>) require <code>Authorization: Bearer …</code>; only <code>/health</code> and filtered <code>GET /api/config</code> remain public.
        </p>
      </Card>

      <Card advanced title="Web Search" subtitle="Dedicated SearXNG connector for Web Search tab; independent of academic literature sources" icon={<Globe size={16} className="icon-accent" />}>
        <CheckboxField label="Enable web search via SearXNG" checked={config.web_search_enabled === 'true'} onChange={(on) => set('web_search_enabled', String(on))} />
        <TextField id="web-search-url" type="url" label="Base URL for web search" value={config.web_search_url} onValue={(v) => set('web_search_url', v)} placeholder="http://localhost:8080" />
        <div className="u-row u-gap-10">
          <button id="test-web-search" type="button" className="action-btn" onClick={testWebSearch} disabled={!config.web_search_url || tests.results.web_search?.loading}>
            {tests.results.web_search?.loading ? <RefreshCw size={13} className="animate-spin" /> : <Zap size={13} />}
            <span>Test Connection</span>
          </button>
          <div className="u-grow"><TestStatus status={tests.results.web_search} /></div>
        </div>
      </Card>

      <Card advanced title="System Cache" subtitle="Clear local SQLite search cache to fetch fresh records directly from source APIs" icon={<Database size={16} className="text-muted" />}>
        <button className="action-btn action-btn-danger-outline u-self-start" onClick={() => void clearCache()}>
          <Trash2 size={14} />
          <span>{cacheCleared ? 'Cache Cleared!' : 'Clear Search Cache'}</span>
        </button>
      </Card>

      <Card title="Backup & Restore" subtitle="Portable local backup of research data" icon={<Database size={16} className="icon-accent" />}>
        <BackupRestore />
      </Card>

      <Card title="Update Center" subtitle="Signed automatic updates from the public stable release channel" icon={<RefreshCw size={16} className="tone-emerald" />}>
        <UpdateCenter />
      </Card>
    </div>
  );
};
