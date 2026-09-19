import React, { useState } from 'react';
import { Archive, Download, Loader2, Upload } from 'lucide-react';
import { invoke, isTauri } from '@tauri-apps/api/core';

export const BackupRestore: React.FC = () => {
  const [busy, setBusy] = useState<'backup' | 'restore' | null>(null);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');

  const backup = async () => {
    setBusy('backup'); setError(''); setMessage('');
    try {
      const path = await invoke<string | null>('export_backup');
      if (path) setMessage(`Backup saved to ${path}`);
    } catch (cause) { setError((cause as Error).message || String(cause)); }
    finally { setBusy(null); }
  };
  const restore = async () => {
    if (!window.confirm('Replace current research data with a ScholarGate backup? Agent access will be revoked. PDF files and API keys are not changed.')) return;
    setBusy('restore'); setError(''); setMessage('');
    try {
      const path = await invoke<string | null>('restore_backup');
      if (path) {
        setMessage(`Restored ${path}. Reloading research data…`);
        window.setTimeout(() => window.location.reload(), 500);
      }
    } catch (cause) { setError((cause as Error).message || String(cause)); }
    finally { setBusy(null); }
  };

  return <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
      <button id="export-backup" type="button" className="action-btn action-btn-primary" disabled={!isTauri() || busy !== null} onClick={() => void backup()}>{busy === 'backup' ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}<span>Back up research data</span></button>
      <button id="restore-backup" type="button" className="action-btn" disabled={!isTauri() || busy !== null} onClick={() => void restore()}>{busy === 'restore' ? <Loader2 size={14} className="animate-spin" /> : <Upload size={14} />}<span>Restore backup</span></button>
    </div>
    <div style={{ display: 'flex', gap: 8, fontSize: 11, color: 'var(--text-muted)' }}><Archive size={14} /><span>Includes workspaces, papers, notes, tags, reading state, searches and download records. API keys, tokens, agent credentials, cache and PDF binaries are excluded.</span></div>
    {message && <div className="alert alert-info" role="status">{message}</div>}
    {error && <div className="alert alert-danger" role="alert">{error}</div>}
  </div>;
};
