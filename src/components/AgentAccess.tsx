import React, { useCallback, useEffect, useRef, useState } from 'react';
import { gatewayFetch, gatewayUrl } from '../lib/gateway';
import { Workspace } from '../types';

interface AgentGrant {
  id: string;
  name: string;
  workspace_ids: string[];
  writable: boolean;
  revoked: boolean;
}
interface CreatedAgent { agent: AgentGrant; token: string }

/** "Failed to fetch" is the browser's word for "no response at all"; say what that
 *  means here instead of showing the reader a raw TypeError. */
function describe(error: unknown): string {
  const message = (error as Error)?.message || String(error);
  return /failed to fetch|networkerror|load failed/i.test(message)
    ? 'Cannot reach the local gateway. Check that it is running, then press Refresh.'
    : message;
}

async function readResponse<T>(response: Response): Promise<T> {
  const value = await response.json().catch(() => null);
  if (!response.ok) throw new Error(value?.error || (response.status === 401 || response.status === 403
    ? 'Set a gateway administrator token in Settings, then refresh.' : `Request failed (${response.status})`));
  return value as T;
}

export const AgentAccess: React.FC = () => {
  const [agents, setAgents] = useState<AgentGrant[]>([]);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [name, setName] = useState('');
  const [writable, setWritable] = useState(false);
  const [created, setCreated] = useState<CreatedAgent | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [ready, setReady] = useState(false);
  const lock = useRef(false);
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    const [grants, projects] = await Promise.all([
      gatewayFetch('/api/agents').then(readResponse<AgentGrant[]>),
      gatewayFetch('/api/workspaces').then(readResponse<Workspace[]>),
    ]);
    if (!mounted.current) return;
    setAgents(grants); setWorkspaces(projects); setReady(true);
    setSelected(ids => ids.filter(id => projects.some(project => project.id === id)));
  }, []);

  const perform = async (action: () => Promise<void>) => {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError(''); setMessage('');
    try { await action(); } catch (e) { if (mounted.current) setError(describe(e)); }
    finally { lock.current = false; if (mounted.current) setBusy(false); }
  };
  useEffect(() => {
    mounted.current = true;
    void refresh().catch(e => { if (mounted.current) setError(describe(e)); });
    return () => { mounted.current = false; };
  }, [refresh]);

  const create = (event: React.FormEvent) => {
    event.preventDefault();
    void perform(async () => {
      const result = await readResponse<CreatedAgent>(await gatewayFetch('/api/agents', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: name.trim(), workspace_ids: selected, writable }),
      }));
      if (!mounted.current) return;
      setCreated(result); setName('');
      await refresh();
    });
  };
  const test = () => void perform(async () => {
    if (!created) return;
    // Use the new restricted token, never gatewayFetch's administrator token.
    const response = await fetch(gatewayUrl('/mcp'), {
      method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${created.token}` },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: {
        name: 'get_workspace', arguments: { workspace_id: created.agent.workspace_ids[0], limit: 1, fields: [] },
      } }),
    });
    const result = await readResponse<{ error?: { message?: string }; result?: { isError?: boolean } }>(response);
    if (result.error || !result.result || result.result.isError) throw new Error('Workspace access check failed. Check the token and project.');
    setMessage('Connected. Project access verified.');
  });

  return <section className="page-container" aria-labelledby="agent-access-title">
    <div className="page-header"><h2 id="agent-access-title">Agent access</h2>
      <button id="agents-refresh" className="action-btn" disabled={busy} onClick={() => void perform(refresh)}>Refresh</button>
    </div>
    {error && <div className="alert alert-warning" role="alert">{error}</div>}
    {message && <div role="status">{message}</div>}
    <form className="agent-access-form" onSubmit={create}>
      <fieldset disabled={busy || !ready || !!created} className="cockpit-card" style={{ display: 'grid', gap: 12, minWidth: 0 }}>
        <legend>New connection</legend>
        <label htmlFor="agent-name">Name</label>
        <input id="agent-name" className="field-input" value={name} maxLength={60} required placeholder="Research assistant"
          onChange={e => setName(e.target.value)} />
        <label htmlFor="agent-permission">Permission</label>
        <select id="agent-permission" className="field-input" value={writable ? 'write' : 'read'} onChange={e => setWritable(e.target.value === 'write')}>
          <option value="read">Read only</option><option value="write">Read & write</option>
        </select>
        <fieldset style={{ border: 0, display: 'grid', gap: 8 }}><legend>Projects</legend>
          {workspaces.map(workspace => <label key={workspace.id} htmlFor={`agent-workspace-${workspace.id}`}>
            <input id={`agent-workspace-${workspace.id}`} type="checkbox" checked={selected.includes(workspace.id)}
              onChange={e => setSelected(ids => e.target.checked ? [...ids, workspace.id] : ids.filter(id => id !== workspace.id))} /> {workspace.name}
          </label>)}
        </fieldset>
        <button id="agent-create" className="action-btn action-btn-primary" type="submit" disabled={!name.trim() || !selected.length}>Create connection</button>
      </fieldset>
    </form>
    {created && <section className="cockpit-card agent-access-token" aria-label="New agent token" style={{ display: 'grid', gap: 10 }}>
      <p>Copy now. This token is shown only once.</p>
      <label htmlFor="agent-new-token">Agent token</label>
      <input id="agent-new-token" className="field-input" type="password" value={created.token} readOnly autoComplete="off" />
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
        <button id="agent-copy-config" className="action-btn" disabled={busy} onClick={() => void perform(async () => {
          await navigator.clipboard.writeText(JSON.stringify({ mcpServers: { scholargateway: {
            url: gatewayUrl('/mcp'), headers: { Authorization: `Bearer ${created.token}` },
          } } }, null, 2));
          setMessage('MCP configuration copied. Keep it private.');
        })}>Copy MCP config</button>
        <button id="agent-test" className="action-btn" disabled={busy} onClick={test}>Test access</button>
        <button id="agent-dismiss-token" className="action-btn" disabled={busy} onClick={() => { setCreated(null); setMessage(''); }}>Done</button>
      </div>
    </section>}
    {/* Before the admin token exists there is nothing to list, and rendering the
        card anyway left an empty bordered box under the form. */}
    {(ready || agents.length > 0) && <section className="cockpit-card" aria-label="Agent connections">
      {!agents.length && <p>No connections yet.</p>}
      {agents.map(agent => <article key={agent.id} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '12px 0', borderBottom: '1px solid var(--cockpit-border)' }}>
        <div style={{ flex: 1, minWidth: 0, overflowWrap: 'anywhere' }}><h3>{agent.name}</h3>
          <p>{agent.revoked ? 'Revoked' : agent.writable ? 'Read & write' : 'Read only'} · {agent.workspace_ids.map(id => workspaces.find(w => w.id === id)?.name || 'Removed project').join(', ')}</p>
        </div>
        {!agent.revoked && <button id={`agent-revoke-${agent.id}`} className="action-btn" disabled={busy} onClick={() => {
          if (!window.confirm(`Revoke ${agent.name}? Its token will stop working.`)) return;
          void perform(async () => {
            await readResponse(await gatewayFetch(`/api/agents/${encodeURIComponent(agent.id)}`, { method: 'DELETE' }));
            if (created?.agent.id === agent.id) setCreated(null);
            await refresh(); setMessage('Connection revoked.');
          });
        }}>Revoke</button>}
      </article>)}
    </section>}
  </section>;
};
