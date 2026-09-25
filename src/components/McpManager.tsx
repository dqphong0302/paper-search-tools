import React, { useState } from 'react';
import { getGatewayPort } from '../lib/gateway';
import { invoke, isTauri } from '@tauri-apps/api/core';

interface ConfigView {
  path: string;
  revision: string;
  servers: Record<string, unknown>;
  managed: Record<string, { definition: unknown; enabled: boolean }>;
  backup?: string;
}

export const McpManager: React.FC = () => {
  const SAMPLE = JSON.stringify({ url: `http://127.0.0.1:${getGatewayPort()}/mcp` }, null, 2);
  const [path, setPath] = useState('');
  const [view, setView] = useState<ConfigView | null>(null);
  const [name, setName] = useState('scholargate');
  const [definition, setDefinition] = useState(SAMPLE);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const native = isTauri();
  const run = async (action: () => Promise<void>) => {
    setBusy(true); setError(''); setMessage('');
    try { await action(); } catch (error) { setError(String(error)); } finally { setBusy(false); }
  };
  const edit = (action: string, serverName: string) => {
    if (!view || !window.confirm(`${action}: ${serverName}\nFile: ${view.path}\nThe application will create a backup and only modify app-managed entries. The client may execute stdio MCP commands upon reloading configuration.`)) return;
    void run(async () => {
      const result = await invoke<ConfigView>('edit_mcp_config', {
        path: view.path, expectedRevision: view.revision, name: serverName, action,
        definition: action === 'add' || action === 'update' ? JSON.parse(definition) : null,
      });
      setView(result); setMessage(`Saved. Backup: ${result.backup}. Reload configuration in your AI client to apply changes.`);
    });
  };
  const allNames = [...new Set([...Object.keys(view?.servers || {}), ...Object.keys(view?.managed || {})])].sort();

  return <section className="cockpit-card" aria-labelledby="mcp-manager-title" style={{ display: 'grid', gap: 12 }}>
    <h2 id="mcp-manager-title">MCP Client Configuration Manager</h2>
    <p>Supports JSON configurations with an <code>mcpServers</code> key: stdio, HTTP, and SSE. Choose the canonical file, not a symlink. Adding config does not automatically start external servers.</p>
    {!native && <div className="alert alert-warning">Reading and editing client configuration is only available inside the desktop application.</div>}
    {error && <div role="alert" className="alert alert-danger">{error}</div>}
    {message && <div role="status" className="alert" style={{ overflowWrap: 'anywhere' }}>{message}</div>}
    <fieldset disabled={!native || busy} style={{ display: 'grid', gap: 10, border: 0, padding: 0, minWidth: 0 }}>
      <label htmlFor="mcp-config-path">Target configuration file (absolute path, parent directory must exist)</label>
      <input id="mcp-config-path" className="field-input" value={path} placeholder="/absolute/path/to/client/mcp.json" onChange={(event) => { setPath(event.target.value); setView(null); }} />
      <button id="mcp-read-config" className="action-btn" disabled={!path.trim()} onClick={() => void run(async () => setView(await invoke<ConfigView>('read_mcp_config', { path })))}>Read Configuration / Prepare File</button>
    </fieldset>
    <label htmlFor="mcp-server-name">Server Identifier</label>
    <input id="mcp-server-name" className="field-input" value={name} disabled={busy} onChange={(event) => setName(event.target.value)} />
    <label htmlFor="mcp-server-definition">Server JSON Definition — prefer referencing environment variables; do not paste secrets into shared configs</label>
    <textarea id="mcp-server-definition" className="field-input" rows={8} value={definition} disabled={busy} spellCheck={false} onChange={(event) => setDefinition(event.target.value)} style={{ fontFamily: 'var(--font-mono)', resize: 'vertical' }} />
    <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
      <button id="mcp-template-http" className="action-btn" disabled={busy} onClick={() => setDefinition(SAMPLE)}>ScholarGate HTTP Template</button>
      <button id="mcp-template-stdio" className="action-btn" disabled={busy} onClick={() => setDefinition(JSON.stringify({ command: '/absolute/path/to/node', args: ['/absolute/path/to/server.js'], env: {} }, null, 2))}>stdio Template</button>
      <button id="mcp-copy-config" className="action-btn" disabled={busy} onClick={() => void run(async () => {
        if (!/^[a-zA-Z0-9_-]{1,64}$/.test(name)) throw new Error('Invalid server identifier');
        await navigator.clipboard.writeText(JSON.stringify({ mcpServers: { [name]: JSON.parse(definition) } }, null, 2));
        setMessage('Server configuration copied to clipboard.');
      })}>Copy Configuration</button>
      <button id="mcp-test-http" className="action-btn" disabled={!native || busy} onClick={() => {
        if (!window.confirm('Send MCP initialize request to the configured URL? Headers will be transmitted directly without following redirects.')) return;
        void run(async () => setMessage(JSON.stringify(await invoke('test_mcp_http', { definition: JSON.parse(definition) }), null, 2)));
      }}>Test HTTP Initialize</button>
      <button id="mcp-add-server" className="action-btn action-btn-primary" disabled={!native || busy || !view || !name.trim()} onClick={() => edit('add', name)}>Add to Client</button>
      <button id="mcp-update-server" className="action-btn" disabled={!native || busy || !view?.managed[name]} onClick={() => edit('update', name)}>Save Selected Item</button>
    </div>
    {view && <section aria-labelledby="mcp-servers-title">
      <h3 id="mcp-servers-title">Servers in Config File</h3>
      {allNames.length === 0 && <p>No servers configured yet.</p>}
      {allNames.map((serverName) => {
        const managed = view.managed[serverName];
        return <article key={serverName} style={{ padding: '12px 0', borderTop: '1px solid var(--cockpit-border)' }}>
          <strong>{serverName}</strong> · {managed ? (managed.enabled ? 'App-managed · Enabled' : 'App-managed · Disabled') : 'Existing · Read-only'}
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 8 }}>
            <button id={`mcp-select-${serverName}`} className="action-btn" disabled={busy} onClick={() => { setName(serverName); setDefinition(JSON.stringify(managed?.definition ?? view.servers[serverName], null, 2)); }}>View / Edit</button>
            {managed && <>
              <button id={`mcp-toggle-${serverName}`} className="action-btn" disabled={!native || busy} onClick={() => edit(managed.enabled ? 'disable' : 'enable', serverName)}>{managed.enabled ? 'Disable' : 'Enable'}</button>
              <button id={`mcp-remove-${serverName}`} className="action-btn" disabled={!native || busy} onClick={() => edit('remove', serverName)}>Remove from Config</button>
            </>}
          </div>
        </article>;
      })}
    </section>}
  </section>;
};
