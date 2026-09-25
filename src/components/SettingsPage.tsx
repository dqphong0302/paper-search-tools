import React, { useState } from 'react';
import { CheckCircle2, Layers, Plug, RefreshCw, Save, Server, Settings, Terminal } from 'lucide-react';
import { AiClients } from './AiClients';
import { useSettingsForm } from './settings/useSettingsForm';
import { SettingsFormApi } from './settings/model';
import { Card } from './settings/ui';
import { SourcesTab } from './settings/SourcesTab';
import { ConnectionsTab } from './settings/ConnectionsTab';
import { GatewayTab } from './settings/GatewayTab';

type Tab = 'sources' | 'connections' | 'clients' | 'gateway';

const TABS: [Tab, string, React.ReactNode][] = [
  ['sources', 'Search Sources', <Layers size={14} />],
  ['connections', 'Connections & Keys', <Terminal size={14} />],
  ['clients', 'AI Clients', <Plug size={14} />],
  ['gateway', 'Gateway & Security', <Server size={14} />],
];

export const SettingsPage: React.FC = () => {
  const [tab, setTab] = useState<Tab>('sources');
  const form = useSettingsForm();
  const api: SettingsFormApi = { config: form.config, set: form.set, update: form.update, onError: form.setError };

  return (
    <div className="page-container settings-page">
      <div className="settings-header">
        <div>
          <h2 className="settings-title">
            <Settings size={18} className="icon-accent" />
            <span>Settings</span>
          </h2>
          <p className="settings-note">Search sources, API credentials, and local agent gateway</p>
        </div>
        <div className="u-row">
          <button id="refresh-settings" className="action-btn" disabled={form.loading || form.saving} onClick={form.reload}>
            <RefreshCw size={14} className={form.loading ? 'animate-spin' : ''} /><span>Refresh</span>
          </button>
          <button id="save-settings" className="action-btn action-btn-primary" onClick={() => void form.save()} disabled={!form.loaded || form.saving}>
            {form.saved ? <><CheckCircle2 size={15} /><span>Saved!</span></> : <><Save size={15} /><span>{form.saving ? 'Saving…' : 'Save Settings'}</span></>}
          </button>
        </div>
      </div>

      {form.error && <div className="alert alert-warning" role="alert"><div>{form.error}</div></div>}
      {!form.loaded && (
        <button id="settings-retry-load" className="action-btn u-self-start" disabled={form.loading} onClick={form.reload}>
          <RefreshCw size={14} /> {form.loading ? 'Loading settings…' : 'Retry loading settings'}
        </button>
      )}

      <div className="u-row u-gap-6">
        {TABS.map(([id, label, icon]) => (
          <button key={id} className={`action-btn ${tab === id ? 'action-btn-primary' : ''}`} onClick={() => setTab(id)}>
            {icon}
            <span>{label}</span>
          </button>
        ))}
      </div>

      {tab === 'sources' && <SourcesTab {...api} />}
      {tab === 'connections' && <ConnectionsTab {...api} />}
      {tab === 'clients' && (
        <Card title="AI Clients on this machine" subtitle="Install the ScholarGate MCP server and bundled skills into supported local AI clients" icon={<Plug size={16} className="icon-accent" />}>
          <AiClients />
        </Card>
      )}
      {tab === 'gateway' && <GatewayTab {...api} />}
    </div>
  );
};
