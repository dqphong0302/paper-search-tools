import React, { useEffect, useRef, useState } from 'react';
import { Award, Download, Loader2, Upload, X } from 'lucide-react';
import { importRankingsCsv, rankingStatus, updateRankings } from '../lib/rankings';

const DISMISS_KEY = 'sg_rankings_banner_dismissed';

/**
 * Shown until a journal ranking is loaded: without it no paper gets a Q1–Q4
 * badge, which otherwise looks like the feature is broken.
 */
export const RankingsBanner: React.FC<{ onLoaded?: () => void }> = ({ onLoaded }) => {
  const [missing, setMissing] = useState(false);
  const [dismissed, setDismissed] = useState(() => {
    try { return localStorage.getItem(DISMISS_KEY) === '1'; } catch { return false; }
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (dismissed) return;
    void rankingStatus().then((status) => setMissing(status.journals === 0)).catch(() => undefined);
  }, [dismissed]);

  if (!missing || dismissed) return null;

  const run = async (action: () => ReturnType<typeof updateRankings>) => {
    setBusy(true); setError('');
    try {
      const status = await action();
      if (status.journals > 0) { setMissing(false); onLoaded?.(); }
    } catch (cause) {
      setError((cause as Error).message);
    } finally {
      setBusy(false);
      if (fileRef.current) fileRef.current.value = '';
    }
  };

  const dismiss = () => {
    setDismissed(true);
    try { localStorage.setItem(DISMISS_KEY, '1'); } catch { /* per-viewer convenience only */ }
  };

  return (
    <div className="alert alert-info rankings-banner" role="region" aria-label="Journal rankings">
      <Award size={16} style={{ flexShrink: 0, marginTop: 1 }} />
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8, flex: 1 }}>
        <span>
          <b>See journal quartiles (Q1–Q4).</b> Load the free SCImago journal ranking once to badge, filter and group papers by quartile.
        </span>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
          <button id="rankings-banner-download" type="button" className="action-btn action-btn-primary" disabled={busy} onClick={() => void run(updateRankings)}>
            {busy ? <Loader2 size={13} className="animate-spin" /> : <Download size={13} />} Load ranking
          </button>
          <button type="button" className="action-btn" disabled={busy} onClick={() => fileRef.current?.click()}>
            <Upload size={13} /> Import CSV…
          </button>
          <input
            ref={fileRef}
            type="file"
            accept=".csv,text/csv"
            hidden
            onChange={(event) => { const file = event.target.files?.[0]; if (file) void run(() => importRankingsCsv(file)); }}
          />
        </div>
        {error && <span style={{ color: 'var(--status-rose)' }}>{error}</span>}
      </div>
      <button type="button" className="action-btn" onClick={dismiss} aria-label="Dismiss" style={{ padding: '4px 6px', alignSelf: 'flex-start' }}>
        <X size={13} />
      </button>
    </div>
  );
};
