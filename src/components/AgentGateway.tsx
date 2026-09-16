import React, { useState } from 'react';
import { Activity, Terminal } from 'lucide-react';
import { TelemetryStats } from '../types';
import { AgentMonitor } from './AgentMonitor';
import { IntegrationsPage } from './IntegrationsPage';
import { AgentAccess } from './AgentAccess';

interface AgentGatewayProps {
  telemetry: TelemetryStats | null;
  isOnline: boolean;
  onRefresh: () => void;
  port: number;
}

type Section = 'status' | 'connect' | 'access';

export const AgentGateway: React.FC<AgentGatewayProps> = ({ telemetry, isOnline, onRefresh, port }) => {
  const [section, setSection] = useState<Section>('access');

  const tabs: { id: Section; label: string; icon: React.ReactNode }[] = [
    { id: 'access', label: 'Access', icon: <Terminal size={14} /> },
    { id: 'connect', label: 'MCP & Skills', icon: <Terminal size={14} /> },
    { id: 'status', label: 'Activity', icon: <Activity size={14} /> },
  ];

  return (
    <div className="page-container" style={{ gap: 14 }}>
      <div role="tablist" aria-label="Agent Gateway" className="segmented" style={{ alignSelf: 'flex-start' }}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            id={`gateway-tab-${tab.id}`}
            type="button"
            role="tab"
            aria-selected={section === tab.id}
            className={`segmented-item ${section === tab.id ? 'active' : ''}`}
            onClick={() => setSection(tab.id)}
          >
            {tab.icon}
            <span>{tab.label}</span>
          </button>
        ))}
      </div>

      {section === 'status' && (
        <>
          {(!isOnline || (telemetry?.total_queries ?? 0) === 0) && (
            <div className="alert alert-info" role="status">
              <div>
                <div className="alert-title">
                  {isOnline ? 'No AI Agent calls recorded yet' : 'Gateway is offline'}
                </div>
                <div>
                  Copy the MCP configuration into Claude Desktop, Cursor, or your client, then run a search
                  or execute "Send Test" below to verify connectivity.
                </div>
              </div>
              <button
                type="button"
                className="action-btn action-btn-primary"
                style={{ marginLeft: 'auto' }}
                onClick={() => setSection('connect')}
              >
                Open Setup Guide
              </button>
            </div>
          )}
          <AgentMonitor telemetry={telemetry} isOnline={isOnline} onRefresh={onRefresh} port={port} />
        </>
      )}
      {section === 'connect' && <IntegrationsPage port={port} />}
      {section === 'access' && <AgentAccess />}
    </div>
  );
};
