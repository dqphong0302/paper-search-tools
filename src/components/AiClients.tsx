import React, { useCallback, useEffect, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { AlertTriangle, CheckCircle2, Circle, Package, Plug, RefreshCw, Trash2 } from 'lucide-react';

export interface ClientSkillState {
  name: string;
  source: string;
  installed: boolean;
  managed: boolean;
  enabled: boolean;
}

export interface AiClientStatus {
  id: string;
  name: string;
  detected: boolean;
  mcp_path: string;
  mcp_format: string;
  mcp_installed: boolean;
  mcp_managed: boolean;
  mcp_entry: string | null;
  mcp_error: string | null;
  skills_path: string | null;
  skills: ClientSkillState[];
  skills_error: string | null;
  note: string;
  token_note: string | null;
}

type Action = 'install_mcp' | 'remove_mcp' | 'install_skills' | 'remove_skills';

const StatusPill: React.FC<{ ok: boolean; okLabel: string; offLabel: string }> = ({ ok, okLabel, offLabel }) => (
  <span
    className={`cockpit-badge ${ok ? 'badge-emerald' : 'badge-vjol'}`}
    style={{ fontSize: 9.5, display: 'inline-flex', alignItems: 'center', gap: 4 }}
  >
    {ok ? <CheckCircle2 size={10} /> : <Circle size={10} />}
    {ok ? okLabel : offLabel}
  </span>
);

/**
 * One-click setup for the AI clients installed on this machine. Detection and
 * every edit happen in the Rust backend, which keeps backups and refuses to
 * touch entries this app did not create.
 */
export const AiClients: React.FC = () => {
  const native = isTauri();
  const [clients, setClients] = useState<AiClientStatus[]>([]);
  const [busy, setBusy] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [tokenNote, setTokenNote] = useState('');

  const refresh = useCallback(async () => {
    if (!native) return;
    setLoading(true);
    try {
      setClients(await invoke<AiClientStatus[]>('list_ai_clients'));
      setError('');
    } catch (e) {
      setError(`Unable to inspect AI clients: ${String(e)}`);
    } finally {
      setLoading(false);
    }
  }, [native]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const run = async (client: AiClientStatus, action: Action) => {
    const confirmations: Record<Action, string> = {
      install_mcp: `Add the ScholarGateway MCP server to ${client.name}?\nFile: ${client.mcp_path}\nA backup of the current file is kept.`,
      remove_mcp: `Remove the ScholarGateway MCP server from ${client.name}?\nFile: ${client.mcp_path}`,
      install_skills: `Install the three bundled ScholarGateway skills into ${client.skills_path}?`,
      remove_skills: `Remove the bundled ScholarGateway skills from ${client.skills_path}?\nEach folder is moved to a recoverable archive, not deleted.`,
    };
    if (!window.confirm(confirmations[action])) return;
    setBusy(`${client.id}:${action}`);
    setError('');
    setMessage('');
    setTokenNote('');
    try {
      const updated = await invoke<AiClientStatus>('setup_ai_client', { client: client.id, action });
      setClients((prev) => prev.map((entry) => (entry.id === updated.id ? updated : entry)));
      if (updated.token_note) setTokenNote(updated.token_note);
      setMessage(
        action === 'install_mcp'
          ? `${client.name}: MCP server added. ${client.note}`
          : action === 'remove_mcp'
            ? `${client.name}: MCP server removed. ${client.note}`
            : action === 'install_skills'
              ? `${client.name}: skills installed into ${updated.skills_path}. Reload skills in the client.`
              : `${client.name}: skills archived.`
      );
    } catch (e) {
      setError(`${client.name}: ${String(e)}`);
    } finally {
      setBusy('');
    }
  };

  if (!native) {
    return (
      <div className="alert alert-warning" role="status">
        Reading and editing AI client configuration is only available in the desktop app; a browser tab
        cannot reach these files.
      </div>
    );
  }

  return (
    <section style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, flexWrap: 'wrap' }}>
        <p style={{ fontSize: 12, color: 'var(--text-muted)', margin: 0, maxWidth: 640 }}>
          Connects this gateway to the AI clients on this machine. Installing writes one MCP server entry
          named <code>scholargateway</code> and copies the three bundled skills; the previous config file is
          backed up first, and entries this app did not create are never modified.
        </p>
        <button id="ai-clients-refresh" className="action-btn" onClick={() => void refresh()} disabled={loading}>
          <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
          <span>Re-check</span>
        </button>
      </div>

      {error && <div className="alert alert-danger" role="alert" style={{ overflowWrap: 'anywhere' }}>{error}</div>}
      {message && <div className="alert alert-success" role="status" style={{ overflowWrap: 'anywhere' }}>{message}</div>}
      {tokenNote && (
        <div className="alert alert-warning" role="status" style={{ overflowWrap: 'anywhere' }}>
          <AlertTriangle size={14} style={{ flexShrink: 0, marginTop: 1 }} />
          <span>{tokenNote}</span>
        </div>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(320px, 1fr))', gap: 12 }}>
        {clients.map((client) => {
          const skillsInstalled = client.skills.filter((skill) => skill.installed).length;
          const managedSkills = client.skills.filter((skill) => skill.managed).length;
          return (
            <article
              key={client.id}
              id={`ai-client-${client.id}`}
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: 8,
                padding: 12,
                border: '1px solid var(--cockpit-border)',
                borderRadius: 'var(--radius-sm)',
                background: client.detected ? '#ffffff' : '#fafafa',
                opacity: client.detected ? 1 : 0.75,
              }}
            >
              <header style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
                <span style={{ fontSize: 13, fontWeight: 700 }}>{client.name}</span>
                <StatusPill ok={client.detected} okLabel="DETECTED" offLabel="NOT INSTALLED" />
              </header>

              {!client.detected && (
                <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0, overflowWrap: 'anywhere' }}>
                  No configuration directory for this client was found on this machine.
                </p>
              )}

              {client.detected && (
                <>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <Plug size={13} style={{ color: 'var(--primary-cyan)' }} />
                      <span style={{ fontSize: 12, fontWeight: 600 }}>MCP server</span>
                      <StatusPill ok={client.mcp_installed} okLabel="CONNECTED" offLabel="NOT CONNECTED" />
                      <span className="cockpit-badge" style={{ fontSize: 9 }}>{client.mcp_format.toUpperCase()}</span>
                    </div>
                    <code style={{ fontSize: 10, color: 'var(--text-dim)', overflowWrap: 'anywhere' }}>{client.mcp_path}</code>
                    {client.mcp_installed && client.mcp_entry && client.mcp_entry !== 'scholargateway' && (
                      <span style={{ fontSize: 10.5, color: 'var(--status-amber)' }}>
                        Already present under the name “{client.mcp_entry}”, which this app did not create.
                      </span>
                    )}
                    {client.mcp_error && (
                      <span style={{ fontSize: 10.5, color: 'var(--status-rose)', display: 'flex', gap: 4, alignItems: 'flex-start' }}>
                        <AlertTriangle size={11} style={{ flexShrink: 0, marginTop: 1 }} />
                        <span style={{ overflowWrap: 'anywhere' }}>{client.mcp_error}</span>
                      </span>
                    )}
                    <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
                      <button
                        id={`ai-client-${client.id}-install-mcp`}
                        className="action-btn action-btn-primary"
                        style={{ padding: '4px 10px', fontSize: 11 }}
                        disabled={!!busy || !!client.mcp_error || (client.mcp_installed && !client.mcp_managed)}
                        onClick={() => void run(client, 'install_mcp')}
                      >
                        {busy === `${client.id}:install_mcp` ? <RefreshCw size={12} className="animate-spin" /> : <Plug size={12} />}
                        <span>{client.mcp_managed ? 'Reinstall' : 'Install MCP'}</span>
                      </button>
                      {client.mcp_managed && (
                        <button
                          id={`ai-client-${client.id}-remove-mcp`}
                          className="action-btn"
                          style={{ padding: '4px 10px', fontSize: 11, color: 'var(--status-rose)' }}
                          disabled={!!busy}
                          onClick={() => void run(client, 'remove_mcp')}
                        >
                          <Trash2 size={12} />
                          <span>Remove</span>
                        </button>
                      )}
                    </div>
                  </div>

                  <div style={{ display: 'flex', flexDirection: 'column', gap: 5, borderTop: '1px solid #f1f5f9', paddingTop: 8 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <Package size={13} style={{ color: 'var(--primary-cyan)' }} />
                      <span style={{ fontSize: 12, fontWeight: 600 }}>Skills</span>
                      {client.skills_path ? (
                        <StatusPill
                          ok={skillsInstalled === client.skills.length && client.skills.length > 0}
                          okLabel={`${skillsInstalled}/${client.skills.length} INSTALLED`}
                          offLabel={`${skillsInstalled}/${client.skills.length} INSTALLED`}
                        />
                      ) : (
                        <span className="cockpit-badge" style={{ fontSize: 9 }}>NOT SUPPORTED</span>
                      )}
                    </div>
                    {client.skills_path && (
                      <>
                        <code style={{ fontSize: 10, color: 'var(--text-dim)', overflowWrap: 'anywhere' }}>{client.skills_path}</code>
                        <div style={{ display: 'flex', gap: 5, flexWrap: 'wrap' }}>
                          {client.skills.map((skill) => (
                            <span
                              key={skill.name}
                              className={`settings-source-pill ${skill.installed ? 'active' : 'inactive'}`}
                              title={
                                skill.installed
                                  ? skill.managed
                                    ? `${skill.name} installed by ScholarGateway${skill.enabled ? '' : ' (disabled)'}`
                                    : `${skill.name} exists but was not installed by this app`
                                  : `${skill.name} not installed`
                              }
                              style={{ cursor: 'default' }}
                            >
                              {skill.installed && <CheckCircle2 size={10} />}
                              <span>{skill.name}</span>
                            </span>
                          ))}
                        </div>
                        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
                          <button
                            id={`ai-client-${client.id}-install-skills`}
                            className="action-btn"
                            style={{ padding: '4px 10px', fontSize: 11 }}
                            disabled={!!busy || skillsInstalled === client.skills.length}
                            onClick={() => void run(client, 'install_skills')}
                          >
                            {busy === `${client.id}:install_skills` ? <RefreshCw size={12} className="animate-spin" /> : <Package size={12} />}
                            <span>Install skills</span>
                          </button>
                          {managedSkills > 0 && (
                            <button
                              id={`ai-client-${client.id}-remove-skills`}
                              className="action-btn"
                              style={{ padding: '4px 10px', fontSize: 11, color: 'var(--status-rose)' }}
                              disabled={!!busy}
                              onClick={() => void run(client, 'remove_skills')}
                            >
                              <Trash2 size={12} />
                              <span>Archive skills</span>
                            </button>
                          )}
                        </div>
                      </>
                    )}
                    {client.skills_error && (
                      <span style={{ fontSize: 10.5, color: 'var(--text-muted)', overflowWrap: 'anywhere' }}>{client.skills_error}</span>
                    )}
                  </div>

                  <p style={{ fontSize: 10.5, color: 'var(--text-muted)', margin: 0 }}>{client.note}</p>
                </>
              )}
            </article>
          );
        })}
      </div>
      {!loading && clients.length === 0 && (
        <p style={{ fontSize: 12, color: 'var(--text-muted)' }}>No supported AI client was found on this machine.</p>
      )}
    </section>
  );
};
