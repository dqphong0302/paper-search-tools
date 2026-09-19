import React, { useEffect, useRef, useState } from 'react';
import { CheckCircle2, Download, RefreshCw, RotateCcw } from 'lucide-react';
import { getVersion } from '@tauri-apps/api/app';
import { isTauri } from '@tauri-apps/api/core';
import type { Update } from '@tauri-apps/plugin-updater';

type Status = 'idle' | 'checking' | 'available' | 'current' | 'downloading' | 'error';

export const UpdateCenter: React.FC = () => {
  const updateRef = useRef<Update | null>(null);
  const [currentVersion, setCurrentVersion] = useState('—');
  const [status, setStatus] = useState<Status>('idle');
  const [version, setVersion] = useState('');
  const [notes, setNotes] = useState('');
  const [progress, setProgress] = useState<number | null>(null);
  const [message, setMessage] = useState('Automatic checks run shortly after ScholarGate starts.');

  useEffect(() => {
    if (!isTauri()) return;
    void getVersion().then(setCurrentVersion).catch(() => setCurrentVersion('Unknown'));
    return () => { void updateRef.current?.close(); };
  }, []);

  const checkNow = async () => {
    if (!isTauri()) { setStatus('error'); setMessage('Update checks are available only in the installed desktop app.'); return; }
    setStatus('checking');
    setMessage('Checking the signed release feed…');
    setProgress(null);
    try {
      await updateRef.current?.close();
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check({ timeout: 30_000 });
      updateRef.current = update;
      localStorage.setItem('scholargate_last_update_check', new Date().toISOString());
      if (!update) {
        setStatus('current');
        setVersion('');
        setNotes('');
        setMessage(`ScholarGate ${currentVersion} is the latest stable release.`);
        return;
      }
      setStatus('available');
      setVersion(update.version);
      setNotes(update.body?.trim() || 'No release notes were provided.');
      setMessage(`Signed update ${update.version} is ready to download.`);
    } catch (cause) {
      setStatus('error');
      setMessage(`Update check failed: ${(cause as Error).message}`);
    }
  };

  const install = async () => {
    const update = updateRef.current;
    if (!update) return;
    setStatus('downloading');
    setMessage('Downloading signed update…');
    let downloaded = 0;
    let total = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') total = event.data.contentLength ?? 0;
        if (event.event === 'Progress') downloaded += event.data.chunkLength;
        if (total > 0) setProgress(Math.min(100, Math.round((downloaded / total) * 100)));
        if (event.event === 'Finished') setMessage('Update installed. Restarting ScholarGate…');
      });
      localStorage.removeItem('scholargate_update_later');
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    } catch (cause) {
      setStatus('error');
      setMessage(`Update installation failed: ${(cause as Error).message}`);
    }
  };

  const postpone = () => {
    if (version) localStorage.setItem('scholargate_update_later', version);
    setMessage(`Update ${version} postponed until the next version or a manual check.`);
  };

  return <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, alignItems: 'center' }}>
      <span className="cockpit-badge badge-cyan">Installed {currentVersion}</span>
      <span className="cockpit-badge">Channel: Stable</span>
      <button id="check-for-updates" type="button" className="action-btn" onClick={() => void checkNow()} disabled={status === 'checking' || status === 'downloading'}>
        <RefreshCw size={14} className={status === 'checking' ? 'animate-spin' : ''} /><span>Check for updates</span>
      </button>
      {status === 'available' && <>
        <button id="install-update" type="button" className="action-btn action-btn-primary" onClick={() => void install()}><Download size={14} /><span>Install {version}</span></button>
        <button type="button" className="action-btn" onClick={postpone}><RotateCcw size={14} /><span>Later</span></button>
      </>}
    </div>
    {progress !== null && <div><progress max={100} value={progress} style={{ width: '100%' }} /><div style={{ fontSize: 11 }}>{progress}%</div></div>}
    <div role="status" className={`alert ${status === 'error' ? 'alert-danger' : status === 'available' ? 'alert-info' : ''}`}>
      {status === 'current' && <CheckCircle2 size={15} />}<span>{message}</span>
    </div>
    {notes && <details><summary>Release notes for {version}</summary><div style={{ whiteSpace: 'pre-wrap', marginTop: 8, fontSize: 12, lineHeight: 1.55 }}>{notes}</div></details>}
  </div>;
};
