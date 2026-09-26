import React, { useEffect, useRef, useState } from 'react';
import { Award, Download, Loader2, Upload } from 'lucide-react';
import { gatewayFetch } from '../lib/gateway';

interface RankingStatus {
  journals: number;
  year?: number | null;
  imported_at?: number | null;
  download_url: string;
}

/** Settings card: load the SCImago journal ranking that powers the Q1–Q4 badges. */
export const JournalRankings: React.FC = () => {
  const [status, setStatus] = useState<RankingStatus | null>(null);
  const [busy, setBusy] = useState<'update' | 'import' | null>(null);
  const [error, setError] = useState('');
  const fileRef = useRef<HTMLInputElement>(null);

  const load = async () => {
    try {
      const res = await gatewayFetch('/api/rankings');
      if (res.ok) setStatus(await res.json());
    } catch {
      /* gateway offline; the card simply shows no status */
    }
  };
  useEffect(() => { void load(); }, []);

  const apply = async (res: Response) => {
    const json = await res.json().catch(() => null);
    if (!res.ok) throw new Error(json?.error || `The gateway returned status ${res.status}`);
    setStatus(json);
  };

  const update = async () => {
    setBusy('update'); setError('');
    try { await apply(await gatewayFetch('/api/rankings/update', { method: 'POST' })); }
    catch (cause) { setError((cause as Error).message); }
    finally { setBusy(null); }
  };

  const importFile = async (file: File) => {
    setBusy('import'); setError('');
    try {
      const csv = await file.text();
      await apply(await gatewayFetch('/api/rankings/import', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ csv }),
      }));
    } catch (cause) { setError((cause as Error).message); }
    finally { setBusy(null); if (fileRef.current) fileRef.current.value = ''; }
  };

  return <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
    <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
      {status?.journals
        ? <>
            <Award size={13} style={{ verticalAlign: -2 }} /> {status.journals.toLocaleString()} ranked journals
            {status.year ? ` · SJR ${status.year}` : ''}
            {status.imported_at ? ` · loaded ${new Date(status.imported_at * 1000).toLocaleDateString()}` : ''}
          </>
        : 'No journal ranking loaded yet — papers show no Q1–Q4 badge until you load one.'}
    </div>
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
      <button id="rankings-update" type="button" className="action-btn action-btn-primary" disabled={busy !== null} onClick={() => void update()}>
        {busy === 'update' ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
        <span>{status?.journals ? 'Update from SCImago' : 'Download from SCImago'}</span>
      </button>
      <button type="button" className="action-btn" disabled={busy !== null} onClick={() => fileRef.current?.click()}>
        {busy === 'import' ? <Loader2 size={14} className="animate-spin" /> : <Upload size={14} />}
        <span>Import SCImago CSV…</span>
      </button>
      <input
        ref={fileRef}
        id="rankings-file"
        type="file"
        accept=".csv,text/csv"
        hidden
        onChange={(event) => { const file = event.target.files?.[0]; if (file) void importFile(file); }}
      />
    </div>
    <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
      Quartiles are each journal&apos;s best SJR quartile, matched by ISSN or exact journal title. If the download is blocked,
      open <a href={status?.download_url ?? 'https://www.scimagojr.com/journalrank.php'} target="_blank" rel="noreferrer">scimagojr.com</a>,
      use “Download data”, and import the CSV here.
    </div>
    {error && <div className="alert alert-danger" role="alert">{error}</div>}
  </div>;
};
